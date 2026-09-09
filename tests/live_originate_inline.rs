//! Integration tests against a live FreeSWITCH instance: an inline action list
//! whose arguments contain the separator.
//!
//! `inline_dialplan_hunt` splits the list on `,` — or on whatever an `m:<d>:`
//! prefix names — and a bare separator inside an argument is read as an action
//! boundary. The switch reports nothing when that happens: it builds the
//! applications the split produced and runs them. Only a live switch can prove
//! the escaping this crate emits is undone the way `cleanup_separated_string`
//! is expected to undo it, which is why these are not unit tests.
//!
//! These tests require FreeSWITCH ESL on localhost:8022 with password ClueCon.
//! Run with: cargo test --test 'live_*' -- --ignored

mod live_common;

use freeswitch_esl_tokio::commands::originate::{Variables, VariablesType};
use freeswitch_esl_tokio::commands::LoopbackEndpoint;
use freeswitch_esl_tokio::{
    Application, Endpoint, EslEventType, EventFormat, EventHeader, HeaderLookup, Originate,
};
use live_common::{bgapi_originate_ok, connect, getvar, ChannelReaper};
use std::time::Duration;
use tokio::time::Instant;

/// Long enough for the list to reach `park`, short enough that a test which
/// panics before reaping does not strand the channel for the whole run.
const PARK_TIMEOUT_SECS: &str = "8";

/// Holds the leg while the test reads back what the application received.
fn parked_loopback() -> Endpoint {
    let mut vars = Variables::new(VariablesType::Default);
    vars.insert("park_timeout", PARK_TIMEOUT_SECS);
    Endpoint::Loopback(
        LoopbackEndpoint::new("9199")
            .with_context("test")
            .with_variables(vars),
    )
}

/// The value the application actually received, or `None` if it never ran.
///
/// Reads the variable rather than the event's `Application-Data`, because the
/// data header shows what the dialplan parsed while the variable shows what
/// reached the application — and it is the second one that a split in the
/// wrong place corrupts.
async fn run_and_read_back(cmd: &Originate, variable: &str) -> Option<String> {
    let (client, mut events, _permit) = connect().await;

    client
        .subscribe_events(
            EventFormat::Plain,
            &[
                // bgapi_originate_ok reads the originated uuid off this one.
                EslEventType::BackgroundJob,
                EslEventType::ChannelExecuteComplete,
            ],
        )
        .await
        .expect("subscribe failed");

    let uuid = bgapi_originate_ok(&client, &mut events, cmd).await;
    let mut reaper = ChannelReaper::new(&client);
    reaper.track(&uuid);

    // `set` completing is the point the variable exists; reading before that
    // races the application rather than the wire.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut ran = false;
    while !ran && Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(Ok(evt))) => {
                ran = evt.event_type() == Some(EslEventType::ChannelExecuteComplete)
                    && evt.unique_id() == Some(uuid.as_str())
                    && evt.header(EventHeader::Application) == Some("set");
            }
            Ok(Some(Err(e))) => panic!("event error: {e}"),
            Ok(None) => panic!("event stream closed"),
            Err(_) => break,
        }
    }

    let value = if ran {
        getvar(&client, &uuid, variable).await
    } else {
        None
    };

    reaper
        .reap()
        .await;
    assert!(ran, "the set application never ran on {uuid}");
    value
}

/// A comma in an argument is the case that shipped broken: the list would have
/// become three applications, two of them fragments of a tone spec.
#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_inline_argument_keeps_its_commas() {
    let spec = "tone_stream://%(500,0,800)";
    let cmd = Originate::inline(
        parked_loopback(),
        [
            Application::new("set", Some(format!("probe_comma={spec}"))),
            Application::park(),
        ],
    )
    .expect("inline builder rejected a valid list");

    assert_eq!(
        run_and_read_back(&cmd, "probe_comma").await,
        Some(spec.to_string())
    );
}

/// The same for a separator the caller named, which the arguments also use.
#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_inline_argument_keeps_a_named_separator() {
    let value = "a|b|c";
    let cmd = Originate::inline_with_delimiter(
        parked_loopback(),
        [
            Application::new("set", Some(format!("probe_pipe={value}"))),
            Application::park(),
        ],
        '|',
    )
    .expect("inline builder rejected a valid separator");

    assert_eq!(
        run_and_read_back(&cmd, "probe_pipe").await,
        Some(value.to_string())
    );
}

/// Every separator at once, since none of them can make a list unrenderable.
#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_inline_argument_keeps_a_hostile_value() {
    let value = ",|;~^!";
    let cmd = Originate::inline(
        parked_loopback(),
        [
            Application::new("set", Some(format!("probe_hostile={value}"))),
            Application::park(),
        ],
    )
    .expect("inline builder rejected a valid list");

    assert_eq!(
        run_and_read_back(&cmd, "probe_hostile").await,
        Some(value.to_string())
    );
}

/// The boundary the builder's refusal is drawn at: one quote arrives, so
/// refusing it would be over-strict. A second is stripped, which is why two are
/// refused — that half cannot be asserted here, because the builder will not
/// produce the command that would demonstrate it.
///
/// The argument carries a space so the action list is wrapped in quotes, which
/// is the case where the escaping is load-bearing rather than incidental.
#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_inline_argument_keeps_a_single_quote() {
    let value = "it's one value";
    let cmd = Originate::inline(
        parked_loopback(),
        [
            Application::new("set", Some(format!("probe_quote={value}"))),
            Application::park(),
        ],
    )
    .expect("one quote is deliverable and must not be refused");

    assert_eq!(
        run_and_read_back(&cmd, "probe_quote").await,
        Some(value.to_string())
    );
}

/// An argument rewritten after construction still renders correctly, which is
/// the property that separator selection could not offer.
#[tokio::test]
#[ignore = "needs FreeSWITCH ESL on :8022; see docs/live-test-switch.md"]
async fn live_inline_argument_rewritten_after_construction() {
    use freeswitch_esl_tokio::commands::originate::OriginateTarget;

    let mut cmd = Originate::inline(
        parked_loopback(),
        [
            Application::new("set", Some("probe_late=${placeholder}")),
            Application::park(),
        ],
    )
    .expect("inline builder rejected a valid list");

    let rendered = "{absolute_codec_string=G722,PCMU}";
    let OriginateTarget::InlineApplications(apps) = cmd.target_mut() else {
        panic!("expected InlineApplications");
    };
    *apps[0].args_mut() = Some(format!("probe_late={rendered}"));

    assert_eq!(
        run_and_read_back(&cmd, "probe_late").await,
        Some(rendered.to_string())
    );
}
