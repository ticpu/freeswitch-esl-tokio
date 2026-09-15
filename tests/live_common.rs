//! Shared helpers for the live-FreeSWITCH test binaries: connection setup,
//! the session-throttle raise, bgapi/originate correlation, and channel
//! cleanup, and the dial-string escaping cases. Each live binary declares
//! `mod live_common;` and uses a subset.
#![allow(dead_code)]

use freeswitch_esl_tokio::commands::originate::{Variables, VariablesType};
use freeswitch_esl_tokio::commands::{
    BlockParse, DialStringCarrier, DialStringTarget, UuidGetVar, UuidKill,
};
use freeswitch_esl_tokio::{
    parse_api_body, EslClient, EslConnectOptions, EslEvent, EslEventPriority, EslEventStream,
    EslEventType, EventFormat, EventHeader, HeaderLookup, Originate, DEFAULT_ESL_PASSWORD,
    UNDEF_VALUE,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::{OnceCell, Semaphore};
use tokio::time::Instant;

pub const ESL_HOST: &str = "localhost";
pub const ESL_PORT: u16 = 8022;
pub const ESL_PASSWORD: &str = DEFAULT_ESL_PASSWORD;
pub const MAX_CONCURRENT_CONNECTIONS: usize = 5;
pub const REQUIRED_SPS: u32 = 1000;
pub const REQUIRED_MAX_SESSIONS: u32 = 1000;

pub static CONN_SEMAPHORE: Semaphore = Semaphore::const_new(MAX_CONCURRENT_CONNECTIONS);
pub static SPS_RAISED: OnceCell<()> = OnceCell::const_new();

/// Raise the switch's session admission rate and session cap for the whole suite.
///
/// Each loopback originate costs two sessions and the bowout pair costs four,
/// so a parallel run bursts far past a stock `sessions-per-second` and holds
/// more concurrent sessions than a small `max-sessions`. Past either,
/// `switch_core_session_request_uuid` returns NULL and the originate comes
/// back `-ERR DESTINATION_OUT_OF_ORDER` -- surfacing as a random unrelated
/// test failing, a different one each run.
///
/// Raised once per binary and deliberately left raised: a parallel suite has
/// no reliable last-test-finished hook to restore it from, and a
/// half-restored throttle would reintroduce exactly the flakiness this
/// removes. Both `fsctl` settings are idempotent, so each live binary raising
/// them again on its own first connection is harmless.
pub async fn raise_session_throttle(client: &EslClient) {
    SPS_RAISED
        .get_or_init(|| async {
            for command in [
                format!("fsctl sps {}", REQUIRED_SPS),
                format!("fsctl max_sessions {}", REQUIRED_MAX_SESSIONS),
            ] {
                let resp = client
                    .api(&command)
                    .await
                    .unwrap_or_else(|e| panic!("{command}: transport error: {e:?}"));
                resp.api_result()
                    .unwrap_or_else(|e| {
                        panic!("{command} rejected -- the ESL user needs fsctl in esl-allowed-api: {e:?}")
                    });
            }
        })
        .await;
}

pub async fn connect() -> (
    EslClient,
    EslEventStream,
    tokio::sync::SemaphorePermit<'static>,
) {
    let permit = CONN_SEMAPHORE
        .acquire()
        .await
        .expect("semaphore closed");
    let opts = EslConnectOptions::new().with_connect_timeout(Duration::from_secs(30));
    let (client, events) = EslClient::connect_with_options(ESL_HOST, ESL_PORT, ESL_PASSWORD, opts)
        .await
        .expect("failed to connect to FreeSWITCH");
    client.set_command_timeout(Duration::from_secs(10));
    raise_session_throttle(&client).await;
    (client, events, permit)
}

/// bgapi originate via the builder, wait for BACKGROUND_JOB, return the UUID.
pub async fn bgapi_originate_ok(
    client: &EslClient,
    events: &mut EslEventStream,
    cmd: &Originate,
) -> String {
    let resp = client
        .bgapi(&cmd.to_string())
        .await
        .expect("bgapi originate transport error");
    let job_uuid = resp
        .job_uuid()
        .expect("bgapi should return Job-UUID header")
        .to_string();

    // Wait for the BACKGROUND_JOB event with our Job-UUID
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(Ok(evt))) => {
                if evt.event_type() == Some(EslEventType::BackgroundJob)
                    && evt.job_uuid() == Some(&job_uuid)
                {
                    let body = evt
                        .body()
                        .expect("BACKGROUND_JOB should have a body");
                    let uuid = parse_api_body(body).expect("originate failed");
                    return uuid.to_string();
                }
            }
            Ok(Some(Err(e))) => panic!("event error: {}", e),
            Ok(None) => panic!("event stream closed"),
            Err(_) => break,
        }
    }
    panic!("timeout waiting for BACKGROUND_JOB {}", job_uuid);
}

/// Wait for `event_type` on `uuid`'s channel, ignoring every other channel's.
///
/// `None` means the deadline passed, so a caller holding channels can still
/// reap before it asserts. A stream error or a closed stream panics: the
/// connection is gone and nothing can be reaped through it anyway.
pub async fn wait_for_own_event(
    events: &mut EslEventStream,
    uuid: &str,
    event_type: EslEventType,
    deadline: Instant,
) -> Option<EslEvent> {
    while Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(Ok(evt))) => {
                if evt.event_type() == Some(event_type) && evt.unique_id() == Some(uuid) {
                    return Some(evt);
                }
            }
            Ok(Some(Err(e))) => panic!("event error waiting for {event_type} on {uuid}: {e}"),
            Ok(None) => panic!("event stream closed waiting for {event_type} on {uuid}"),
            Err(_) => break,
        }
    }
    None
}

/// Send a CUSTOM event on a subclass no other test uses and return the copy the
/// switch delivers back.
///
/// Repeating a name in `headers` stacks it into an `ARRAY::` value, the way
/// FreeSWITCH carries a repeated SIP header.
pub async fn custom_roundtrip(
    client: &EslClient,
    events: &mut EslEventStream,
    headers: &[(&str, &str)],
) -> EslEvent {
    static NEXT_SUBCLASS: AtomicU32 = AtomicU32::new(0);
    let subclass = format!(
        "esl_test::rt_{}_{}",
        std::process::id(),
        NEXT_SUBCLASS.fetch_add(1, Ordering::Relaxed)
    );

    client
        .subscribe_events_raw(EventFormat::Plain, &format!("CUSTOM {subclass}"))
        .await
        .expect("subscribe to the test subclass");

    let mut event = EslEvent::with_type(EslEventType::Custom);
    event.set_header("Event-Name", "CUSTOM");
    event.set_header("Event-Subclass", subclass.clone());
    event.set_priority(EslEventPriority::Normal);
    for (name, value) in headers {
        event
            .push_header(name, value)
            .unwrap_or_else(|e| panic!("{name} does not stack: {e}"));
    }

    client
        .sendevent(event)
        .await
        .expect("sendevent transport error")
        .check()
        .expect("sendevent rejected");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(Ok(evt))) => {
                if evt.header(EventHeader::EventSubclass) == Some(subclass.as_str()) {
                    return evt;
                }
            }
            Ok(Some(Err(e))) => panic!("event error: {e}"),
            Ok(None) => panic!("event stream closed"),
            Err(_) => break,
        }
    }
    panic!("did not receive the CUSTOM event sent on {subclass}");
}

/// Kill a channel by UUID, ignoring errors (channel may already be gone).
pub async fn kill_channel(client: &EslClient, uuid: &str) {
    let cmd = UuidKill::new(uuid);
    if let Err(e) = client
        .api(&cmd.to_string())
        .await
    {
        eprintln!("cleanup: uuid_kill {} failed: {}", uuid, e);
    }
}

/// Poll a channel variable until it is set, or until the deadline passes.
///
/// The switch announces no event for "this variable exists now", so a test
/// that has to wait for one waits for the value itself rather than for an
/// interval it guessed.
pub async fn wait_for_var(
    client: &EslClient,
    uuid: &str,
    name: &str,
    deadline: Instant,
) -> Option<String> {
    loop {
        if let Some(value) = getvar(client, uuid, name).await {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The channels a test created, so it can kill them before it asserts.
///
/// Cleanup has to run *before* the assertions. A panic between creating a
/// channel and killing it strands that channel for the rest of the run, and
/// stranded channels burn the switch's session budget until later originates
/// start failing with `-ERR DESTINATION_OUT_OF_ORDER` -- which surfaces as some
/// unrelated test failing, not this one.
pub struct ChannelReaper<'a> {
    client: &'a EslClient,
    uuids: Vec<String>,
}

impl<'a> ChannelReaper<'a> {
    pub fn new(client: &'a EslClient) -> Self {
        Self {
            client,
            uuids: Vec::new(),
        }
    }

    /// Register a channel to kill. Repeats are ignored, so a uuid learned from
    /// several events is still killed once.
    pub fn track(&mut self, uuid: impl Into<String>) {
        let uuid = uuid.into();
        if !self
            .uuids
            .contains(&uuid)
        {
            self.uuids
                .push(uuid);
        }
    }

    pub async fn reap(&mut self) {
        for uuid in self
            .uuids
            .drain(..)
        {
            kill_channel(self.client, &uuid).await;
        }
    }
}

/// Whether the switch still has this channel.
///
/// Cleanup swallows "already gone", so a test that needs to prove a channel
/// died has to ask before reaping.
pub async fn channel_exists(client: &EslClient, uuid: &str) -> bool {
    let resp = client
        .api(&format!("uuid_exists {}", uuid))
        .await
        .unwrap_or_else(|e| panic!("uuid_exists {}: transport error: {}", uuid, e));
    match resp.api_result() {
        Ok("true") => true,
        Ok("false") => false,
        Ok(other) => panic!("uuid_exists {}: unexpected reply {:?}", uuid, other),
        Err(e) => panic!("uuid_exists {}: {}", uuid, e),
    }
}

/// Read a channel variable, mapping "not set" to `None`.
///
/// `uuid_getvar` writes the literal `_undef_` when the variable is unset, so an
/// absent variable arrives as a successful reply rather than an error. Only
/// that is `None`: a dead channel answers `-ERR no such channel`, and folding
/// that into `None` too would let an assertion expecting an unset variable pass
/// against a channel that had already gone away.
pub async fn getvar(client: &EslClient, uuid: &str, name: &str) -> Option<String> {
    let cmd = UuidGetVar::new(uuid, name);
    let resp = client
        .api(&cmd.to_string())
        .await
        .unwrap_or_else(|e| panic!("uuid_getvar {} {}: transport error: {}", uuid, name, e));
    match resp.api_result() {
        Ok(UNDEF_VALUE) => None,
        Ok(value) => Some(value.to_string()),
        Err(e) => panic!("uuid_getvar {} {}: {}", uuid, name, e),
    }
}

// --- Dial-string escaping, per carrier ---

/// Values whose escaping is not obvious, each paired with a sentinel so a value
/// that eats its separator shows up as damage to a *later* variable.
///
/// Two quoted values, never one: a block carrying a single quote has no partner
/// for it to pair with and passes under encodings that corrupt a realistic
/// block. Two quotes in one value are the other pairing: the last pass keeps a
/// bare quote only while none follows it in the same field. The empty value is
/// absent because no dial string can express it — `Variables` refuses it at the
/// boundaries that can.
pub const ESCAPING_CASES: &[(&str, &[(&str, &str)])] = &[
    ("plain comma", &[("p1", "a,b"), ("p2", "SENTINEL")]),
    (
        "comma and space",
        &[("p1", "T-1001, urgent"), ("p2", "SENTINEL")],
    ),
    (
        "two quoted values",
        &[("p1", "it's"), ("p2", "don't"), ("p3", "SENTINEL")],
    ),
    (
        "two quotes in one value",
        &[("p1", "l'a'b"), ("p2", "SENTINEL")],
    ),
    (
        "space and two quotes",
        &[("p1", "Rue de l'Île d'Or"), ("p2", "SENTINEL")],
    ),
    (
        "backslash before an inert character",
        &[("p1", r"C:\path"), ("p2", "SENTINEL")],
    ),
    (
        "backslash before one the switch reads as an escape",
        &[("p1", r"a\nb"), ("p2", "SENTINEL")],
    ),
    ("pipe", &[("p1", "a|b"), ("p2", "SENTINEL")]),
    (
        "edge spaces in a value",
        &[("p1", " lead and trail "), ("p2", "SENTINEL")],
    ),
    (
        "argv separators in a value",
        &[("p1", "x~y!z!"), ("p2", "SENTINEL")],
    ),
    (
        "escaped argv separators",
        &[("p1", r"a\~b\!c"), ("p2", "SENTINEL")],
    ),
    (
        "a dollar pair where expansion runs",
        &[("p1", "pa$$word"), ("p2", r"C:\path"), ("p3", "SENTINEL")],
    ),
];

/// `lead` goes in first, so a value that eats its separator damages a variable
/// after it rather than the one the far side is found by.
pub fn escaping_block(
    lead: &[(&str, &str)],
    pairs: &[(&str, &str)],
    separator: Option<char>,
    scope: VariablesType,
) -> Variables {
    let mut vars = Variables::new(scope);
    for (k, v) in lead
        .iter()
        .chain(pairs.iter())
    {
        vars.insert(*k, *v);
    }
    match separator {
        None => vars,
        Some(sep) => vars
            .with_separator(sep)
            .unwrap_or_else(|e| panic!("{sep:?} rejected: {e}")),
    }
}

/// Appears in none of [`ESCAPING_CASES`], which is what `with_separator`
/// demands and what keeps a comma ordinary text inside the block.
pub const ESCAPING_SEPARATOR: char = ';';

/// `Variables` refuses a quote in channel scope, so those rows have no wire
/// form to measure there; `live_channel_scope_pairs_quotes_across_values` in
/// live_channel.rs pins why.
pub fn carried_in(scope: VariablesType, pairs: &[(&str, &str)]) -> bool {
    scope != VariablesType::Channel
        || pairs
            .iter()
            .all(|(_, v)| !v.contains('\''))
}

/// The revision these tests render for: `FREESWITCH_BLOCK_PARSE` names one to
/// measure a switch against, and unset is the crate default.
pub fn block_parse_under_test() -> BlockParse {
    match std::env::var("FREESWITCH_BLOCK_PARSE") {
        Ok(name) => name
            .parse()
            .unwrap_or_else(|e| panic!("FREESWITCH_BLOCK_PARSE: {e}")),
        Err(std::env::VarError::NotPresent) => BlockParse::default(),
        Err(e) => panic!("FREESWITCH_BLOCK_PARSE: {e}"),
    }
}

pub fn target_under_test(carrier: DialStringCarrier) -> DialStringTarget {
    DialStringTarget::new(carrier).with_block_parse(block_parse_under_test())
}
