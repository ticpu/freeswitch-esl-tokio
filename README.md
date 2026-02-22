# freeswitch-esl-tokio

[![CI](https://github.com/ticpu/freeswitch-esl-tokio/actions/workflows/ci.yml/badge.svg)](https://github.com/ticpu/freeswitch-esl-tokio/actions/workflows/ci.yml)
[![Tests](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/test-count.json)](https://github.com/ticpu/freeswitch-esl-tokio/actions/workflows/ci.yml)
[![Event Types](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/event-type-count.json)](https://github.com/ticpu/freeswitch-esl-tokio/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/freeswitch-esl-tokio)](https://crates.io/crates/freeswitch-esl-tokio)
[![docs.rs](https://img.shields.io/docsrs/freeswitch-esl-tokio)](https://docs.rs/freeswitch-esl-tokio)

Production-grade async Rust client for FreeSWITCH's
[Event Socket Library](https://developer.signalwire.com/freeswitch/FreeSWITCH-Explained/Client-and-Developer-Interfaces/Event-Socket-Library/).
Built on Tokio with a split reader/writer architecture that lets you send
commands and receive events concurrently — something no other Rust ESL crate
offers.

## Why this crate

- **Concurrent by design** — `EslClient` is `Clone + Send`. Pass it to any
  Tokio task. Events arrive on a separate `EslEventStream` channel. No mutex
  juggling, no blocking the event loop to send a command.
- **Complete ESL coverage** — all protocol commands, event types verified
  against the C ESL `EVENT_NAMES[]` array, inbound and outbound modes,
  plain/JSON/XML event formats.
- **Typed command builders** — `Originate`, `UuidKill`, `ConferenceDtmf`,
  dptools (`answer`, `bridge`, `playback`, ...) — all implement `Display` with
  no transport coupling. Build commands, unit test them, use them with
  `client.api()` when ready.
- **Typed channel state** — `ChannelState`, `CallState`, `AnswerState`,
  `CallDirection` enums with `FromStr`/`Display`. `ChannelTimetable` extracts
  call lifecycle timestamps. All decoupled from `EslEvent` — works with any
  key-value store via closure-based lookup.
- **Typed header/variable enums** — `EventHeader` (26 variants) and
  `ChannelVariable` (54 variants) for compile-time header and variable name
  checking. No more typos in `"Caller-Caller-ID-Number"` strings.
- **Connection health** — liveness detection via HEARTBEAT subscription,
  configurable command timeouts (default 5s), structured `DisconnectReason`,
  `is_connection_error()` / `is_recoverable()` error classification.
- **Correct wire format** — two-part event framing, percent-decoded headers,
  Content-Type-based format detection. Matches `mod_event_socket.c` exactly.
- **Extensively tested** — round-trip `parse` ↔ `to_string` on all builders,
  mock-server integration tests, and live FreeSWITCH tests.

## Architecture

```
connect() → (EslClient, EslEventStream)

EslClient (Clone + Send)         EslEventStream
├ send commands from any task    ├ events via mpsc channel
├ writer half behind Arc<Mutex>  └ connection status via watch
└ replies via oneshot channel

Background reader task
├ owns the read half + parser
├ routes CommandReply/ApiResponse → pending oneshot
├ routes Event → mpsc channel
├ tracks liveness (any TCP traffic resets timer)
└ broadcasts ConnectionStatus on disconnect
```

See [docs/design-rationale.md](docs/design-rationale.md) for the full
architecture story.

## Quick start

```toml
[dependencies]
freeswitch-esl-tokio = "1"
tokio = { version = "1.0", features = ["full"] }
```

### Connect and run a command

```rust
use freeswitch_esl_tokio::{EslClient, EslError};

#[tokio::main]
async fn main() -> Result<(), EslError> {
    // Password-only authentication (default user)
    let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

    // Or authenticate as a specific user
    let (client, mut events) =
        EslClient::connect_with_user("localhost", 8021, "admin@default", "ClueCon").await?;

    let response = client.api("status").await?;
    println!("{}", response.body_string());

    client.disconnect().await?;
    Ok(())
}
```

### Event loop with liveness detection

```rust
use freeswitch_esl_tokio::{EslClient, EslEventType, EventFormat};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

    // 60s without any TCP traffic → Disconnected(HeartbeatExpired)
    client.set_liveness_timeout(Duration::from_secs(60));

    // HEARTBEAT subscription ensures traffic on idle connections
    client.subscribe_events(EventFormat::Plain, &[
        EslEventType::Heartbeat,
        EslEventType::ChannelAnswer,
        EslEventType::ChannelHangup,
    ]).await?;

    while let Some(Ok(event)) = events.recv().await {
        println!("{:?}", event.event_type());
    }

    // None → reader task exited (disconnect, EOF, or liveness timeout)
    println!("Disconnected: {:?}", events.status());
    Ok(())
}
```

### Background API calls

`api()` blocks until FreeSWITCH finishes the command (subject to command
timeout). `bgapi()` returns immediately with a Job-UUID; the result arrives
as a `BACKGROUND_JOB` event:

```rust
client.subscribe_events(EventFormat::Plain, &[
    EslEventType::BackgroundJob,
]).await?;

let response = client.bgapi("originate user/1000 &park").await?;
let job_uuid = response.job_uuid().expect("bgapi returns Job-UUID");

// In the event loop:
if event.is_event_type(EslEventType::BackgroundJob) {
    if event.job_uuid() == Some(&job_uuid) {
        println!("{}", event.body().unwrap_or(""));
    }
}
```

### Outbound mode

FreeSWITCH connects to your application via the `socket` dialplan app.
After accepting, send `connect` to establish the session:

```rust
use freeswitch_esl_tokio::{EslClient, AppCommand, EventFormat};
use tokio::net::TcpListener;

let listener = TcpListener::bind("0.0.0.0:8040").await?;
let (client, mut events) = EslClient::accept_outbound(&listener).await?;

// Required first command — returns channel data
let channel_data = client.connect_session().await?;
println!("Channel: {}", channel_data.header("Channel-Name").unwrap());

// Subscribe, enable linger, resume dialplan
client.myevents(EventFormat::Plain).await?;
client.linger(None).await?;
client.resume().await?;

// Control the call
client.send_command(AppCommand::answer()).await?;
client.send_command(AppCommand::playback("ivr/ivr-welcome.wav")).await?;

while let Some(Ok(event)) = events.recv().await {
    // handle events...
}
```

### Command builders

Typed builders for FreeSWITCH API commands. All implement `Display`, are
independent of `EslClient`, and can be unit tested without a connection:

```rust
use freeswitch_esl_tokio::commands::*;

// Originate with typed endpoint
let cmd = Originate {
    endpoint: Endpoint::SofiaGateway {
        gateway: "my-provider".into(),
        uri: "18005551212".into(),
        profile: None,
        variables: None,
    },
    applications: ApplicationList(vec![
        Application::new("conference", Some("room1")),
    ]),
    dialplan: Some(DialplanType::Inline),
    context: None, cid_name: None, cid_num: None, timeout: None,
};
client.bgapi(&cmd.to_string()).await?;

// Round-trip: parse ↔ display
let parsed: Originate = cmd.to_string().parse().unwrap();
assert_eq!(parsed.to_string(), cmd.to_string());

// UUID commands
let kill = UuidKill { uuid: uuid.into(), cause: Some("NORMAL_CLEARING".into()) };
client.api(&kill.to_string()).await?;

// Conference commands
let dtmf = ConferenceDtmf { name: "room1".into(), member: "all".into(), dtmf: "1".into() };
client.api(&dtmf.to_string()).await?;
```

Channel variable parsers for FreeSWITCH-specific formats:

```rust
use freeswitch_esl_tokio::variables::{EslArray, MultipartBody};

// ARRAY:: delimited values
let arr = EslArray::parse("ARRAY::item1|:item2|:item3").unwrap();
assert_eq!(arr.items(), &["item1", "item2", "item3"]);

// SIP multipart body extraction
let body = MultipartBody::parse(raw_multipart).unwrap();
let pidf = body.by_mime_type("application/pidf+xml");
```

### Typed event accessors

`EslEvent` provides typed accessors that parse header values into enums
instead of returning raw strings:

```rust
use freeswitch_esl_tokio::{ChannelState, CallDirection};

// Typed enums parsed from headers — no string matching
if let Some(state) = event.channel_state() {
    match state {
        ChannelState::CsExecute => println!("Executing app"),
        ChannelState::CsHangup => println!("Hanging up"),
        _ => {}
    }
}

// Convenience accessors for common headers
let cid = event.caller_id_number();
let direction = event.call_direction(); // Option<CallDirection>
let cause = event.hangup_cause();       // Option<&str>
```

Call lifecycle timestamps via `ChannelTimetable`:

```rust
use freeswitch_esl_tokio::TimetablePrefix;

// From an EslEvent — extracts Caller-Channel-*-Time headers
let timetable = event.caller_timetable()?;

// From any key-value store — decoupled from EslEvent
let timetable = ChannelTimetable::from_lookup(
    TimetablePrefix::Caller,
    |key| headers.get(key).map(|v| v.as_str()),
)?;
if let Some(tt) = timetable {
    println!("Created: {:?}, Answered: {:?}", tt.created, tt.answered);
}
```

Compile-time header and variable name enums:

```rust
use freeswitch_esl_tokio::{EventHeader, ChannelVariable};

// No typos — compiler checks the name
let uid = event.header(EventHeader::UniqueId.as_str());
let codec = event.variable(ChannelVariable::ReadCodec.as_str());
```

See `cargo run --example channel_tracker` for a complete reference
implementation using typed accessors for channel lifecycle monitoring.

## Protocol commands

| Method | ESL command |
|---|---|
| `api()` / `bgapi()` | `api`, `bgapi` |
| `subscribe_events()` / `nixevent()` / `noevents()` | `event`, `nixevent`, `noevents` |
| `filter_events()` / `filter_delete()` | `filter`, `filter delete` |
| `myevents()` / `myevents_uuid()` | `myevents` |
| `linger()` / `nolinger()` | `linger`, `nolinger` |
| `resume()` | `resume` |
| `divert_events()` | `divert_events` |
| `execute()` / `sendmsg()` | `sendmsg` |
| `sendevent()` | `sendevent` |
| `connect_session()` | `connect` (outbound) |
| `log()` / `nolog()` | `log`, `nolog` |
| `getvar()` | `getvar` (outbound) |
| `exit()` / `disconnect()` | `exit` |

## How it compares

| | freeswitch-esl-tokio | [freeswitch-esl](https://crates.io/crates/freeswitch-esl) | [eslrs](https://crates.io/crates/eslrs) | [freeswitch-esl-rs](https://crates.io/crates/freeswitch-esl-rs) |
|---|---|---|---|---|
| Async (Tokio) | yes | yes | yes | no (blocking) |
| Split reader/writer | yes | no | no | n/a |
| Inbound + outbound | both | both | both | inbound only |
| Event formats | plain, JSON, XML | JSON only | plain, JSON, XML | plain only |
| Liveness detection | yes | no | no | no |
| Command timeout | yes (default 5s) | no | no | no |
| Error classification | yes | no | no | no |
| Typed state enums | 5 (`ChannelState`, `CallState`, ...) | no | no | no |
| Typed header enums | `EventHeader` (26) + `ChannelVariable` (54) | no | no | no |
| Channel timetable | yes (decoupled from event type) | no | no | no |
| Command builders | 13 typed structs | none | basic | none |
| Event types | ![Event Types](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/event-type-count.json) | — | — | — |
| Test count | ![Tests](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/test-count.json) | — | — | — |

## Development

```sh
./hooks/install.sh   # symlinks pre-commit hook
```

The pre-commit hook runs `cargo fmt --check`, `cargo clippy`, and
`hooks/check-event-types.sh` which verifies the `EslEventType` enum matches
the C ESL `EVENT_NAMES[]` array.

### Testing

Unit and mock-server tests run without external dependencies:

```sh
cargo test --lib
cargo test --test integration_tests --test connection_tests
```

Live integration tests require FreeSWITCH ESL on `127.0.0.1:8022`
(password `ClueCon`). They are `#[ignore]` by default:

```sh
cargo test --test live_freeswitch -- --ignored
```

## Requirements

- Rust 1.70+
- Tokio async runtime

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
