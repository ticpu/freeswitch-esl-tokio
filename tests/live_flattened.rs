//! Integration tests against a live FreeSWITCH instance: `group_call` output
//! against the committed fixtures, and `FlattenedDialString`'s typed view
//! against what each leg's channel receives, per carrier.
//!
//! These tests require FreeSWITCH ESL on localhost:8022 with password ClueCon
//! and the flattened-probe directory groups; see docs/live-test-switch.md.
//! Run with: cargo test --test live_flattened -- --ignored

mod live_common;

use freeswitch_esl_tokio::channel::CallDirection;
use freeswitch_esl_tokio::commands::originate::VariablesType;
use freeswitch_esl_tokio::commands::{
    originate_split, CauseReading, DialStringCarrier, FlattenedDialString,
    FlattenedDialStringError, FlattenedLeg, LegTarget, LegWarning, OriginateError, UuidGetVar,
};
use freeswitch_esl_tokio::variables::VariableName;
use freeswitch_esl_tokio::{
    CommandFailure, Endpoint, EslClient, EslEvent, EslEventStream, EslEventType, EslResult,
    EventFormat, ExecuteOptions, HangupCause, HeaderLookup, UNDEF_VALUE,
};
use live_common::{
    carried_in, channel_exists, connect, escaping_block, getvar, kill_channel, target_under_test,
    wait_for_var, ChannelReaper, ESCAPING_CASES, ESCAPING_SEPARATOR,
};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::Instant;

const API: DialStringCarrier = DialStringCarrier::EslApi;
const DIALPLAN: DialStringCarrier = DialStringCarrier::Dialplan;

/// The directory domain the flattened-probe groups live in.
const DOMAIN: &str = "default";

/// Every key the flattened-probe groups and these tests set.
const PROBE_KEYS: &[&str] = &[
    "fp_marker",
    "originate_timeout",
    "presence_id",
    "sip_invite_domain",
    "k",
    "pe",
    "pa",
    "b",
    "local_var_clobber",
    "sentinel",
    "nv",
    "path",
    "codecs",
    "q",
    "from",
];

/// Names outside every typed variable enum.
struct Var<'a>(&'a str);

impl VariableName for Var<'_> {
    fn as_str(&self) -> &str {
        self.0
    }
}

fn parse(input: &str, carrier: DialStringCarrier) -> FlattenedDialString {
    FlattenedDialString::parse_for(input, target_under_test(carrier))
        .unwrap_or_else(|e| panic!("{input:?} at {carrier:?}: {e}"))
}

/// Unique across the run, so a `hupall` on it reaps this case's channels only.
fn marker(case: &str) -> String {
    format!("fp-{}-{case}", std::process::id())
}

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("freeswitch-types/tests/fixtures/flattened")
}

fn group_call_expr(group: &str, flag: &str) -> String {
    match flag {
        "none" => format!("${{group_call({group}@{DOMAIN})}}"),
        flag => format!("${{group_call({group}@{DOMAIN}+{flag})}}"),
    }
}

async fn live_group_call(client: &EslClient, group: &str, flag: &str) -> String {
    let expr = group_call_expr(group, flag);
    client
        .api(&format!("eval {expr}"))
        .await
        .unwrap_or_else(|e| panic!("eval {expr}: transport error: {e}"))
        .body()
        .unwrap_or_else(|| panic!("eval {expr} answered no body"))
        .to_owned()
}

/// Replace `from` wherever the next character does not continue a word.
fn replace_at_word_end(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        out.push_str(&rest[..at]);
        let after = &rest[at + from.len()..];
        let continues = after
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        out.push_str(if continues { from } else { to });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The fixture README's substitution, in its order.
fn sanitise(live: &str) -> String {
    let text = live
        .replace("127.0.0.1", "192.0.2.1")
        .replace("[::1]", "[2001:db8::1]");
    replace_at_word_end(&text, "@default", "@pbx.example.com").replace(
        "sip_invite_domain=default",
        "sip_invite_domain=pbx.example.com",
    )
}

fn is_registered_contact(leg: &FlattenedLeg) -> bool {
    matches!(leg.target(), LegTarget::Endpoint(Endpoint::Sofia(_)))
}

/// Registered-contact legs sorted in place, every other byte kept, since the
/// registration rows come back in no stable order.
fn registered_legs_sorted(text: &str) -> String {
    let Ok(list) = FlattenedDialString::parse_for(text, target_under_test(API)) else {
        return text.to_owned();
    };
    let mut spans = Vec::new();
    let mut cursor = 0;
    for leg in list.legs() {
        let at = cursor
            + text[cursor..]
                .find(leg.raw())
                .unwrap_or_else(|| panic!("leg {:?} is not in {text:?}", leg.raw()));
        let end = at
            + leg
                .raw()
                .len();
        spans.push((at, end, is_registered_contact(leg)));
        cursor = end;
    }
    let mut contacts: Vec<&str> = spans
        .iter()
        .filter(|(_, _, registered)| *registered)
        .map(|&(at, end, _)| &text[at..end])
        .collect();
    contacts.sort_unstable();
    let mut contacts = contacts.into_iter();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, end, registered) in spans {
        out.push_str(&text[last..at]);
        match registered {
            true => out.push_str(
                contacts
                    .next()
                    .expect("one sorted contact per registered span"),
            ),
            false => out.push_str(&text[at..end]),
        }
        last = end;
    }
    out.push_str(&text[last..]);
    out
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_group_call_output_matches_the_fixtures() {
    let (client, _events, _permit) = connect().await;

    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixture directory")
        .map(|entry| {
            entry
                .expect("fixture directory entry")
                .file_name()
                .into_string()
                .expect("fixture names are UTF-8")
        })
        .filter_map(|name| {
            name.strip_suffix(".txt")
                .map(str::to_owned)
        })
        .filter(|name| !name.starts_with("pbx-"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixture found in {:?}", fixture_dir());

    let mut drifted = Vec::new();
    let mut unrendered = Vec::new();
    let mut parsed = 0;
    for name in &names {
        let (group, flag) = name
            .rsplit_once('.')
            .unwrap_or_else(|| panic!("{name} is not <group>.<flag>"));
        let fixture = std::fs::read_to_string(fixture_dir().join(format!("{name}.txt")))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let live = live_group_call(&client, group, flag).await;

        if registered_legs_sorted(&sanitise(&live)) != registered_legs_sorted(&fixture) {
            drifted.push(format!("{name}: live {live:?}, fixture {fixture:?}"));
        }
        // A list the switch itself cannot read on a carrier has nothing to render there.
        for carrier in [API, DIALPLAN] {
            let Ok(list) = FlattenedDialString::parse_for(&live, target_under_test(carrier)) else {
                continue;
            };
            parsed += 1;
            let raw = list
                .display_raw()
                .to_string();
            if raw != live {
                unrendered.push(format!("{name} at {carrier:?}: {live:?} rendered {raw:?}"));
            }
        }
    }

    assert!(parsed > 0, "no live body parsed on either carrier");
    assert!(
        drifted.is_empty(),
        "group_call output drifted: {drifted:#?}"
    );
    assert!(
        unrendered.is_empty(),
        "display_raw is not the input: {unrendered:#?}"
    );
}

/// Captures whose legs the typed view is checked against, as `(group, flag)`.
const TYPED_CAPTURES: &[(&str, &str)] = &[
    ("g-fp-static", "A"),
    ("g-fp-scopes-all", "A"),
    ("g-fp-scopes-both", "A"),
    ("g-fp-scopes-ent", "A"),
    ("g-fp-scopes-noleg", "A"),
    ("g-fp-scopes-none", "A"),
    ("g-fp-pipe", "A"),
    ("g-fp-ent", "none"),
    ("g-fp-esc-bs-exp", "A"),
    ("g-fp-esc-comma", "A"),
    ("g-fp-esc-emptyval", "A"),
    ("g-fp-esc-quotedempty", "A"),
    ("g-fp-esc-nested", "A"),
    ("g-fp-quote-member", "A"),
    ("g-fp-quote-pair", "A"),
    ("g-fp-gds", "A"),
    ("g-fp-reg", "A"),
    ("fp-one-empty", "A"),
];

/// The legs that get a channel. A later group rings only once every leg of the
/// one before has failed, and each first group here has a leg that answers.
fn dialed_legs(list: &FlattenedDialString) -> Vec<&FlattenedLeg> {
    list.threads()
        .filter_map(|thread| {
            thread
                .groups()
                .next()
        })
        .flat_map(|group| group.legs())
        .filter(|leg| matches!(leg.target(), LegTarget::Endpoint(_)))
        .collect()
}

type LegVars = BTreeMap<&'static str, Option<String>>;

/// What the typed view says the leg's channel carries for every probe key.
fn expected_vars(leg: &FlattenedLeg) -> LegVars {
    for warning in leg.warnings() {
        let key = match warning {
            LegWarning::PairCleared { key, .. }
            | LegWarning::NestedVarsRefused { key, .. }
            | LegWarning::PairIgnored { key, .. } => key,
            other => panic!("{:?}: unexpected warning {other}", leg.raw()),
        };
        assert!(
            leg.variable(Var("sentinel"))
                .is_some(),
            "{:?} warns about {key} but has no sentinel to prove the leg exists",
            leg.raw()
        );
        assert!(
            PROBE_KEYS.contains(&key.as_str()),
            "{key} is not a probe key"
        );
    }
    PROBE_KEYS
        .iter()
        .map(|key| {
            (
                *key,
                leg.variable(Var(key))
                    .map(str::to_owned),
            )
        })
        .collect()
}

fn received_vars(event: &EslEvent) -> LegVars {
    PROBE_KEYS
        .iter()
        .map(|key| {
            (
                *key,
                event
                    .variable_str(key)
                    .map(str::to_owned),
            )
        })
        .collect()
}

/// CHANNEL_CREATE of each outbound leg carrying `marker`: a loopback pair's `-a`
/// half and a sofia leg's outbound side, never the inbound legs they cause.
async fn outbound_creates(
    events: &mut EslEventStream,
    marker: &str,
    want: usize,
    deadline: Instant,
) -> Vec<EslEvent> {
    let mut created = Vec::new();
    let mut deadline = deadline;
    while Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(Ok(evt))) => {
                if evt.event_type() == Some(EslEventType::ChannelCreate)
                    && evt.variable_str("fp_marker") == Some(marker)
                    && evt.call_direction() == Ok(Some(CallDirection::Outbound))
                {
                    created.push(evt);
                    if created.len() == want {
                        // A short settle catches a leg the typed view did not expect.
                        deadline = Instant::now() + Duration::from_millis(500);
                    }
                }
            }
            Ok(Some(Err(e))) => panic!("event error collecting {marker}: {e}"),
            Ok(None) => panic!("event stream closed collecting {marker}"),
            Err(_) => break,
        }
    }
    created
}

async fn hupall(client: &EslClient, variable: &str, value: &str) {
    let cmd = format!("hupall NORMAL_CLEARING {variable} {value}");
    match client
        .api(&cmd)
        .await
    {
        Ok(resp) => {
            if let Err(e) = resp.api_result() {
                eprintln!("cleanup: {cmd}: {e}");
            }
        }
        Err(e) => eprintln!("cleanup: {cmd}: transport error: {e}"),
    }
}

/// The Call-ID tying a sofia leg to the inbound leg it caused, while it is up.
async fn call_id(client: &EslClient, uuid: &str) -> Option<String> {
    let cmd = UuidGetVar::new(uuid, "sip_call_id");
    let resp = match client
        .api(&cmd.to_string())
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("cleanup: {cmd}: transport error: {e}");
            return None;
        }
    };
    match resp.api_result() {
        Ok(UNDEF_VALUE) => None,
        Ok(value) => Some(value.to_owned()),
        // A losing leg is already gone, and its CANCEL took the inbound leg.
        Err(e)
            if e.command_failure()
                .and_then(CommandFailure::payload)
                .is_some_and(|p| p.contains("No such channel")) =>
        {
            None
        }
        Err(e) => {
            eprintln!("cleanup: {cmd}: {e}");
            None
        }
    }
}

fn uuids_of(created: &[EslEvent]) -> Vec<String> {
    created
        .iter()
        .filter_map(|evt| {
            evt.unique_id()
                .map(str::to_owned)
        })
        .collect()
}

/// Kill every leg `marker` tagged, and the inbound legs its sofia legs caused.
async fn reap_marked(client: &EslClient, marker: &str, created: &[EslEvent]) {
    for evt in created {
        let Some(uuid) = evt.unique_id() else {
            continue;
        };
        if evt
            .channel_name()
            .is_some_and(|name| name.starts_with("sofia/"))
        {
            if let Some(id) = call_id(client, uuid).await {
                hupall(client, "sip_call_id", &id).await;
            }
        }
    }
    hupall(client, "fp_marker", marker).await;
}

fn assert_legs_match(label: &str, list: &FlattenedDialString, created: &[EslEvent]) {
    let mut expected: Vec<LegVars> = dialed_legs(list)
        .into_iter()
        .map(expected_vars)
        .collect();
    let mut received: Vec<LegVars> = created
        .iter()
        .map(received_vars)
        .collect();
    assert!(!expected.is_empty(), "{label}: the typed view dials no leg");
    expected.sort();
    received.sort();
    assert_eq!(
        received, expected,
        "{label}: legs disagree with the typed view"
    );
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_equals_what_each_leg_received_over_the_api() {
    let (client, mut events, _permit) = connect().await;
    client
        .subscribe_events(EventFormat::Plain, &[EslEventType::ChannelCreate])
        .await
        .expect("subscribe CHANNEL_CREATE");

    for (group, flag) in TYPED_CAPTURES {
        let label = format!("{group}.{flag} over the API");
        let m = marker(&format!("api-{group}-{flag}"));
        let body = live_group_call(&client, group, flag).await;
        let dial_string = format!("<fp_marker={m},originate_timeout=4>{body}");
        let list = parse(&dial_string, API);
        let want = dialed_legs(&list).len();

        let reply = client
            .api(&format!("originate {dial_string} &park()"))
            .await
            .unwrap_or_else(|e| panic!("{label}: originate transport error: {e}"));
        let created = outbound_creates(
            &mut events,
            &m,
            want,
            Instant::now() + Duration::from_secs(10),
        )
        .await;
        reap_marked(&client, &m, &created).await;
        wait_gone(&client, &label, &uuids_of(&created)).await;

        reply
            .api_result()
            .unwrap_or_else(|e| panic!("{label}: originate {dial_string} rejected: {e}"));
        assert_legs_match(&label, &list, &created);
    }
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_equals_what_each_leg_received_over_the_dialplan() {
    let (client, mut events, _permit) = connect().await;
    client
        .subscribe_events(EventFormat::Plain, &[EslEventType::ChannelCreate])
        .await
        .expect("subscribe CHANNEL_CREATE");

    for (group, flag) in TYPED_CAPTURES {
        let label = format!("{group}.{flag} over the dialplan");
        let m = marker(&format!("dp-{group}-{flag}"));
        let body = live_group_call(&client, group, flag).await;
        let dial_string = format!("<fp_marker={m}>{{fp_marker={m}}}{body}");
        let list = parse(&dial_string, DIALPLAN);
        let want = dialed_legs(&list).len();

        let anchor = client
            .api("originate null/anchor &park()")
            .await
            .expect("anchor transport error")
            .api_result()
            .expect("anchor originate failed")
            .to_owned();
        let mut reaper = ChannelReaper::new(&client);
        reaper.track(&anchor);

        let bridge = client
            .execute_with_options(
                "bridge",
                Some(&dial_string),
                Some(&anchor),
                ExecuteOptions::new().with_async(),
            )
            .await
            .and_then(|resp| resp.into_result());
        let created = match bridge {
            Ok(_) => {
                outbound_creates(
                    &mut events,
                    &m,
                    want,
                    Instant::now() + Duration::from_secs(10),
                )
                .await
            }
            Err(_) => Vec::new(),
        };
        reap_marked(&client, &m, &created).await;
        reaper
            .reap()
            .await;
        let mut channels = uuids_of(&created);
        channels.push(anchor);
        wait_gone(&client, &label, &channels).await;

        if let Err(e) = bridge {
            panic!("{label}: execute bridge {dial_string} rejected: {e}");
        }
        assert_legs_match(&label, &list, &created);
    }
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_error_legs_end_with_the_cause_the_reading_predicts() {
    let (client, _events, _permit) = connect().await;

    let cases: &[(&str, HangupCause)] = &[
        ("g-fp-err-upper", HangupCause::UserBusy),
        ("g-fp-err-lower", HangupCause::UserBusy),
        ("g-fp-err-num", HangupCause::UserBusy),
        ("g-fp-err-prefix", HangupCause::UserBusy),
        ("g-fp-err-zero", HangupCause::DestinationOutOfOrder),
        ("g-fp-err-bogus", HangupCause::NormalClearing),
        ("g-fp-err-empty", HangupCause::NormalClearing),
        ("fp-none", HangupCause::NoRouteDestination),
        ("g-fp-unreg", HangupCause::UserNotRegistered),
    ];

    for (group, measured) in cases {
        let m = marker(&format!("err-{group}"));
        let body = live_group_call(&client, group, "A").await;
        let dial_string = format!("<fp_marker={m},originate_timeout=4>{body}");
        let list = parse(&dial_string, API);
        let legs: Vec<&FlattenedLeg> = list
            .legs()
            .collect();
        let [leg] = legs[..] else {
            panic!("{group}: {body:?} is not a single leg");
        };
        let LegTarget::Error(error) = leg.target() else {
            panic!("{group}: {body:?} is not an error leg");
        };
        let predicted = match error.reading() {
            CauseReading::Name(cause) => cause,
            CauseReading::Number(0) => HangupCause::DestinationOutOfOrder,
            CauseReading::Number(n) => u16::try_from(n)
                .ok()
                .and_then(HangupCause::from_number)
                .unwrap_or_else(|| panic!("{group}: {n} names no cause")),
            CauseReading::Unrecognized => HangupCause::NormalClearing,
            other => panic!("{group}: unexpected reading {other:?}"),
        };

        let reply: EslResult<String> = client
            .api(&format!("originate {dial_string} &park()"))
            .await
            .and_then(|resp| {
                resp.api_result()
                    .map(str::to_owned)
            });
        hupall(&client, "fp_marker", &m).await;
        if let Ok(uuid) = &reply {
            kill_channel(&client, uuid).await;
        }

        let err = reply.expect_err(&format!("{group}: originate {dial_string} succeeded"));
        let Some(CommandFailure::Err(cause)) = err.command_failure() else {
            panic!("{group}: expected -ERR <cause>, got {err}");
        };
        assert_eq!(
            cause,
            predicted.as_str(),
            "{group}: reading {:?}",
            error.reading()
        );
        assert_eq!(cause, measured.as_str(), "{group}");
    }
}

/// Wait until each channel is destroyed, so a leg the reap missed fails here
/// rather than burning a later test's session budget.
async fn wait_gone(client: &EslClient, label: &str, uuids: &[String]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    for uuid in uuids {
        while channel_exists(client, uuid).await {
            assert!(
                Instant::now() < deadline,
                "{label}: {uuid} survived its reap"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// The values of `keys` on the channel `tagged` originates over `carrier`,
/// and every channel that took, for the caller to reap.
async fn received_over(
    client: &EslClient,
    carrier: DialStringCarrier,
    tagged: &str,
    keys: &[&str],
) -> (Result<Vec<Option<String>>, String>, Vec<String>) {
    match carrier {
        DialStringCarrier::EslApi => received_over_the_api(client, tagged, keys).await,
        _ => received_over_the_dialplan(client, tagged, keys).await,
    }
}

async fn received_over_the_api(
    client: &EslClient,
    tagged: &str,
    keys: &[&str],
) -> (Result<Vec<Option<String>>, String>, Vec<String>) {
    let reply = client
        .api(&format!("originate {tagged} &park()"))
        .await
        .map_err(|e| format!("originate transport error: {e}"))
        .and_then(|resp| {
            resp.api_result()
                .map(str::to_owned)
                .map_err(|e| format!("originate rejected: {e}"))
        });
    match reply {
        Ok(uuid) => (Ok(read_vars(client, &uuid, keys).await), vec![uuid]),
        Err(e) => (Err(e), Vec::new()),
    }
}

async fn received_over_the_dialplan(
    client: &EslClient,
    tagged: &str,
    keys: &[&str],
) -> (Result<Vec<Option<String>>, String>, Vec<String>) {
    let anchor = client
        .api("originate null/anchor &park()")
        .await
        .expect("anchor transport error")
        .api_result()
        .expect("anchor originate failed")
        .to_owned();
    let bridged = client
        .execute_with_options(
            "bridge",
            Some(tagged),
            Some(&anchor),
            ExecuteOptions::new().with_async(),
        )
        .await
        .and_then(|resp| resp.into_result());
    if let Err(e) = bridged {
        return (Err(format!("execute bridge rejected: {e}")), vec![anchor]);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    match wait_for_var(client, &anchor, "bridge_uuid", deadline).await {
        Some(peer) => {
            let values = read_vars(client, &peer, keys).await;
            (Ok(values), vec![anchor, peer])
        }
        None => (Err("the anchor never bridged".to_owned()), vec![anchor]),
    }
}

async fn read_vars(client: &EslClient, uuid: &str, keys: &[&str]) -> Vec<Option<String>> {
    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        values.push(getvar(client, uuid, key).await);
    }
    values
}

/// `input` with `[fp_marker=<marker>]` ahead of its loopback endpoint.
fn tag(input: &str, marker: &str) -> String {
    let at = input
        .find("loopback/")
        .unwrap_or_else(|| panic!("{input:?} dials no loopback leg"));
    format!("{}[fp_marker={marker}]{}", &input[..at], &input[at..])
}

fn carrier_name(carrier: DialStringCarrier) -> &'static str {
    match carrier {
        DialStringCarrier::EslApi => "api",
        _ => "dialplan",
    }
}

/// Dial `input` over `carrier` and compare `keys` between the typed view of its
/// single leg and the channel.
async fn typed_view_against_channel(
    client: &EslClient,
    case: &str,
    input: &str,
    carrier: DialStringCarrier,
    keys: &[&str],
) -> Vec<Option<String>> {
    let m = marker(&format!("{case}-{}", carrier_name(carrier)));
    let tagged = tag(input, &m);
    let list = parse(&tagged, carrier);
    let legs: Vec<&FlattenedLeg> = list
        .legs()
        .collect();
    let [leg] = legs[..] else {
        panic!("{tagged:?} at {carrier:?} is not a single leg");
    };
    let typed: Vec<Option<String>> = keys
        .iter()
        .map(|key| {
            leg.variable(Var(key))
                .map(str::to_owned)
        })
        .collect();

    let (received, channels) = received_over(client, carrier, &tagged, keys).await;
    for uuid in &channels {
        kill_channel(client, uuid).await;
    }
    hupall(client, "fp_marker", &m).await;
    wait_gone(client, case, &channels).await;

    let received = received.unwrap_or_else(|e| panic!("{case}: {tagged:?} at {carrier:?}: {e}"));
    assert_eq!(
        received, typed,
        "{case}: {keys:?} of {tagged:?} at {carrier:?} disagree with the typed view"
    );
    received
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_empty_pairs_ignore_or_clear_by_arrival_depth() {
    let (client, _events, _permit) = connect().await;

    let cases: &[(&str, DialStringCarrier, bool)] = &[
        (r"{k=1}{sentinel=s,k=\\\'\\\'}loopback/9199/test", API, true),
        (r"{k=1}{sentinel=s,k=''}loopback/9199/test", API, false),
        (r"{k=1}{sentinel=s,k=}loopback/9199/test", API, false),
        (
            r"[k=1][sentinel=s,k=\\\\\\\\\\\\\\\'\\\\\\\\\\\\\\\']loopback/9199/test",
            API,
            true,
        ),
        (
            r"[k=1][sentinel=s,k=\\\\\\\'\\\\\\\']loopback/9199/test",
            API,
            false,
        ),
        (
            r"{k=1}{sentinel=s,k=\\'\\'}loopback/9199/test",
            DIALPLAN,
            true,
        ),
        (r"{k=1}{sentinel=s,k=''}loopback/9199/test", DIALPLAN, false),
        (
            r"[k=1][sentinel=s,k=\\\\\\\\\\\\\\'\\\\\\\\\\\\\\']loopback/9199/test",
            DIALPLAN,
            true,
        ),
        (
            r"[k=1][sentinel=s,k=\\\\\\'\\\\\\']loopback/9199/test",
            DIALPLAN,
            false,
        ),
    ];

    for (n, (input, carrier, cleared)) in cases
        .iter()
        .enumerate()
    {
        let received = typed_view_against_channel(
            &client,
            &format!("depth{n}"),
            input,
            *carrier,
            &["k", "sentinel"],
        )
        .await;
        let want_k = (!cleared).then(|| "1".to_owned());
        assert_eq!(
            received,
            [want_k, Some("s".to_owned())],
            "{input:?} at {carrier:?}"
        );
    }
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_wider_scope_wins_unless_the_leg_clobbers() {
    let (client, _events, _permit) = connect().await;

    let cases: &[(&str, Option<&str>)] = &[
        ("{k=g}[k=l]loopback/9199/test", Some("g")),
        (
            "{local_var_clobber=true,k=g}[k=l]loopback/9199/test",
            Some("l"),
        ),
        ("<k=e>[k=l]loopback/9199/test", Some("e")),
        ("<k=e>{k=g}loopback/9199/test", Some("g")),
        ("<k=e>{k=g}[k=l]loopback/9199/test", None),
    ];

    for (n, (input, measured)) in cases
        .iter()
        .enumerate()
    {
        for carrier in [API, DIALPLAN] {
            let received =
                typed_view_against_channel(&client, &format!("scope{n}"), input, carrier, &["k"])
                    .await;
            if let Some(measured) = measured {
                assert_eq!(
                    received,
                    [Some((*measured).to_owned())],
                    "{input:?} at {carrier:?}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_argv_split_matches_originate_split() {
    let (client, _events, _permit) = connect().await;

    let unclosed = r"originate {v=x\\'y z}loopback/9199/test &park()";
    assert!(
        matches!(
            originate_split(unclosed, ' '),
            Err(OriginateError::UnclosedQuote(_))
        ),
        "{unclosed:?}"
    );
    assert_eq!(
        FlattenedDialString::parse_for(r"{v=x\\'y z}loopback/9199/test", target_under_test(API)),
        Err(FlattenedDialStringError::ArgvSplit)
    );
    let reply: EslResult<String> = client
        .api(unclosed)
        .await
        .and_then(|resp| {
            resp.api_result()
                .map(str::to_owned)
        });
    if let Ok(uuid) = &reply {
        kill_channel(&client, uuid).await;
    }
    let err = reply.expect_err("an unclosed quote must not originate");
    assert!(
        matches!(err.command_failure(), Some(CommandFailure::Usage(_))),
        "{unclosed:?} answered {err}"
    );

    for (case, dial_string, value) in [
        ("argv-space", r"{v=a\ b}loopback/9199/test", r"a\ b"),
        ("argv-tab", "{v=a\tb}loopback/9199/test", "a\tb"),
    ] {
        let line = format!("originate {dial_string} &park()");
        assert_eq!(
            originate_split(&line, ' ').unwrap_or_else(|e| panic!("{line:?}: {e}")),
            ["originate", dial_string, "&park()"],
            "{case}"
        );
        let received = typed_view_against_channel(&client, case, dial_string, API, &["v"]).await;
        assert_eq!(received, [Some(value.to_owned())], "{case}");
    }
}

/// Every escaping case the renderer carries in `scope`, rendered for `carrier`,
/// read through the typed view and off the channel the same dial string creates.
async fn escaping_through_the_typed_view(carrier: DialStringCarrier, scope: VariablesType) {
    let (client, _events, _permit) = connect().await;

    for separator in [None, Some(ESCAPING_SEPARATOR)] {
        for (case, pairs) in ESCAPING_CASES {
            if !carried_in(scope, pairs) {
                continue;
            }
            let label =
                format!("{case} in {scope:?} scope, separator {separator:?}, at {carrier:?}");
            // The dialplan carrier finds its far leg by a pre-assigned uuid.
            let b_uuid = match carrier {
                DialStringCarrier::EslApi => None,
                _ => Some(
                    client
                        .api("create_uuid")
                        .await
                        .expect("create_uuid transport error")
                        .api_result()
                        .expect("create_uuid failed")
                        .to_owned(),
                ),
            };
            let lead: Vec<(&str, &str)> = b_uuid
                .iter()
                .map(|uuid| ("origination_uuid", uuid.as_str()))
                .collect();
            let vars = escaping_block(&lead, pairs, separator, scope);
            let dial_string = format!(
                "{}null/escaping",
                vars.display_for(target_under_test(carrier))
            );

            let list = parse(&dial_string, carrier);
            let legs: Vec<&FlattenedLeg> = list
                .legs()
                .collect();
            let [leg] = legs[..] else {
                panic!("{label}: {dial_string:?} is not a single leg");
            };
            let keys: Vec<&str> = pairs
                .iter()
                .map(|(key, _)| *key)
                .collect();
            let typed: Vec<Option<String>> = keys
                .iter()
                .map(|key| {
                    leg.variable(Var(key))
                        .map(str::to_owned)
                })
                .collect();

            let (received, channels) = received_over(&client, carrier, &dial_string, &keys).await;
            let mut reaper = ChannelReaper::new(&client);
            for uuid in channels
                .iter()
                .chain(&b_uuid)
            {
                reaper.track(uuid);
            }
            reaper
                .reap()
                .await;

            let received = received.unwrap_or_else(|e| panic!("{label}: {dial_string:?}: {e}"));
            if let Some(b_uuid) = &b_uuid {
                assert_eq!(
                    channels.last(),
                    Some(b_uuid),
                    "{label}: the far leg of {dial_string:?} is not its origination_uuid"
                );
            }
            for (((key, want), typed), got) in pairs
                .iter()
                .zip(typed)
                .zip(received)
            {
                assert!(
                    typed.as_deref() == Some(*want) && got.as_deref() == Some(*want),
                    "{label}: {key} of {dial_string:?}: expected {want:?}, typed {typed:?}, switch {got:?}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_reads_escaping_depths_over_the_api() {
    escaping_through_the_typed_view(API, VariablesType::Default).await;
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_reads_escaping_depths_over_the_dialplan() {
    escaping_through_the_typed_view(DIALPLAN, VariablesType::Default).await;
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_reads_escaping_depths_over_the_api_in_enterprise_scope() {
    escaping_through_the_typed_view(API, VariablesType::Enterprise).await;
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_reads_escaping_depths_over_the_dialplan_in_enterprise_scope() {
    escaping_through_the_typed_view(DIALPLAN, VariablesType::Enterprise).await;
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_reads_escaping_depths_over_the_api_in_channel_scope() {
    escaping_through_the_typed_view(API, VariablesType::Channel).await;
}

#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_typed_view_reads_escaping_depths_over_the_dialplan_in_channel_scope() {
    escaping_through_the_typed_view(DIALPLAN, VariablesType::Channel).await;
}
