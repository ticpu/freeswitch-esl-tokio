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
- **Typed header/variable enums** — `EventHeader` and `ChannelVariable` enums
  for compile-time header and variable name checking. The `HeaderLookup` trait
  provides typed accessors to any key-value store that implements two methods —
  not just `EslEvent`.
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
    // connect() returns a client for sending commands and a stream for receiving events
    let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

    // api() sends a command and waits for the response
    let response = client.api("status").await?;
    // EslResponse has is_success(), reply_text(), body(), header(), etc.
    // Some commands return data in body(), others only set reply_text().
    // body_string() is a shorthand that returns "" when body() is None.
    println!("{}", response.body_string());

    client.disconnect().await?;
    Ok(())
}
```

Multi-tenant setups can authenticate as a specific ACL user:

```rust
let (client, mut events) =
    EslClient::connect_with_user("localhost", 8021, "admin@default", "ClueCon").await?;
```

### Event loop with liveness detection

```rust
use freeswitch_esl_tokio::{EslClient, EslEventType, EventFormat};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

    // Without liveness detection, a dead TCP connection hangs silently.
    // With it, the library fires Disconnected(HeartbeatExpired) after the timeout.
    client.set_liveness_timeout(Duration::from_secs(60));

    // Subscribe to HEARTBEAT so FreeSWITCH sends periodic traffic even when
    // no calls are active, this is what the liveness timer watches for
    client.subscribe_events(EventFormat::Plain, &[
        EslEventType::Heartbeat,
        EslEventType::ChannelAnswer,
        EslEventType::ChannelHangup,
    ]).await?;

    // recv() returns None when the reader task exits (disconnect, EOF, or
    // liveness timeout). Some(Err(_)) is a parse error on a single event,
    // the connection is still alive, so keep looping.
    while let Some(Ok(event)) = events.recv().await {
        println!("{:?}", event.event_type());
    }

    // After the loop, status() tells you why the connection ended
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

// bgapi queues the command and returns immediately with a Job-UUID
let response = client.bgapi("sofia xmlstatus profile internal").await?;
let job_uuid = response.job_uuid().expect("bgapi returns Job-UUID");

// The result arrives later as a BACKGROUND_JOB event
while let Some(Ok(event)) = events.recv().await {
    if event.is_event_type(EslEventType::BackgroundJob)
        && event.job_uuid() == Some(&job_uuid)
    {
        // BACKGROUND_JOB always has a body; most other event types don't
        println!("{}", event.body().unwrap());
        break;
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

// Must be the first command after accept, returns channel info as an EslEvent
let channel_data = client.connect_session().await?;
// Channel-Name is always present in connect response
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

See [docs/outbound-esl-quirks.md](docs/outbound-esl-quirks.md) for outbound
mode gotchas (`connect_session` ordering, `async full` requirement, socket app
quoting).

### Command builders

Typed builders for FreeSWITCH API commands. All implement `Display`, are
independent of `EslClient`, and can be unit tested without a connection:

```rust
use freeswitch_esl_tokio::commands::*;

// Originate with typed endpoint
let cmd = Originate {
    endpoint: Endpoint::SofiaGateway(SofiaGateway {
        gateway: "my-provider".into(),
        destination: "18005551212".into(),
        profile: None,
        variables: None,
    }),
    applications: ApplicationList(vec![
        Application::new("conference", Some("room1")),
    ]),
    dialplan: Some(DialplanType::Inline),
    context: None, cid_name: None, cid_num: None, timeout: None,
};
// → "originate sofia/gateway/my-provider/18005551212 conference:room1 inline"
client.bgapi(&cmd.to_string()).await?;

// Round-trip: parse ↔ display
let parsed: Originate = cmd.to_string().parse().unwrap();
assert_eq!(parsed.to_string(), cmd.to_string());

// UUID commands
let kill = UuidKill { uuid: uuid.into(), cause: Some("NORMAL_CLEARING".into()) };
// → "uuid_kill <uuid> NORMAL_CLEARING"
client.api(&kill.to_string()).await?;

// Conference commands
let dtmf = ConferenceDtmf { name: "room1".into(), member: "all".into(), dtmf: "1".into() };
// → "conference room1 dtmf all 1"
client.api(&dtmf.to_string()).await?;
```

> Output strings verified by unit tests in
> [`commands/originate.rs`](src/commands/originate.rs),
> [`commands/channel.rs`](src/commands/channel.rs), and
> [`commands/conference.rs`](src/commands/conference.rs).

See [docs/command-builders.md](docs/command-builders.md) for the full builder
architecture, all channel/conference command types, and escaping rules.

### Variable parsers

```rust
use freeswitch_esl_tokio::variables::{EslArray, MultipartBody};

// ARRAY:: delimited values
let arr = EslArray::parse("ARRAY::item1|:item2|:item3").unwrap();
assert_eq!(arr.items(), &["item1", "item2", "item3"]);

// SIP multipart body extraction
let body = MultipartBody::parse(raw_multipart).unwrap();
let pidf = body.by_mime_type("application/pidf+xml");
```

> Verified in [`variables/esl_array.rs`](src/variables/esl_array.rs) and
> [`variables/sip_multipart.rs`](src/variables/sip_multipart.rs).

### Typed event accessors

`EslEvent` provides typed accessors that parse header values into enums
instead of returning raw strings:

```rust
use freeswitch_esl_tokio::{ChannelState, CallDirection};

// Typed enums parsed from headers, no string matching needed
if let Some(state) = event.channel_state() {
    match state {
        ChannelState::CsExecute => println!("Executing app"),
        ChannelState::CsHangup => println!("Hanging up"),
        _ => {}
    }
}

// All accessors return Option: None if the header is absent from this event
let cid = event.caller_id_number();     // Option<&str>
let direction = event.call_direction(); // Option<CallDirection>
let cause = event.hangup_cause();       // Option<&str>
```

Call lifecycle timestamps via `ChannelTimetable`:

```rust
use freeswitch_esl_tokio::TimetablePrefix;

// Extracts Caller-Channel-*-Time headers from the event
let timetable = event.caller_timetable()?;

// Also works with any key-value store, not coupled to EslEvent
let timetable = ChannelTimetable::from_lookup(
    TimetablePrefix::Caller,
    |key| headers.get(key).map(|v| v.as_str()),
)?;
if let Some(tt) = timetable {
    println!("Created: {:?}, Answered: {:?}", tt.created, tt.answered);
}
```

Compile-time header and variable name enums via `HeaderLookup`:

```rust
use freeswitch_esl_tokio::{HeaderLookup, EventHeader, ChannelVariable};

// HeaderLookup trait provides typed enum lookups on EslEvent
let uid = event.header(EventHeader::UniqueId);             // Option<&str>
let codec = event.variable(ChannelVariable::ReadCodec);    // Option<&str>
```

### Custom channel tracker with `HeaderLookup`

The `HeaderLookup` trait lets any `HashMap<String, String>` wrapper share
the same typed accessors as `EslEvent`. Implement two methods, get ~17
accessors for free:

```rust
use std::collections::HashMap;
use freeswitch_esl_tokio::HeaderLookup;

struct TrackedChannel {
    data: HashMap<String, String>,
}

impl HeaderLookup for TrackedChannel {
    fn header_str(&self, name: &str) -> Option<&str> {
        self.data.get(name).map(|s| s.as_str())
    }
    fn variable_str(&self, name: &str) -> Option<&str> {
        self.data.get(&format!("variable_{}", name)).map(|s| s.as_str())
    }
}

// Now TrackedChannel has all the same typed accessors:
// ch.channel_state(), ch.call_direction(), ch.hangup_cause(),
// ch.caller_timetable(), ch.header(EventHeader::UniqueId), etc.
```

See `cargo run --example channel_tracker` for a complete reference
implementation using `HeaderLookup` for channel lifecycle monitoring.

## Development

```sh
./hooks/install.sh   # symlinks pre-commit hook
```

The pre-commit hook enforces:

- `cargo fmt --check` — formatting
- `cargo clippy -- -D warnings` — lint warnings as errors
- `RUSTDOCFLAGS="-D missing_docs" cargo doc` — all public items documented
- `hooks/check-event-types.sh` — `EslEventType` enum matches C ESL `EVENT_NAMES[]`

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
| Typed header enums | ![EventHeader](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/event-header-count.json)<br>![ChannelVariable](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/channel-var-count.json)<br>![HeaderLookup](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/header-lookup-count.json) | no | no | no |
| Channel timetable | yes (decoupled from event type) | no | no | no |
| Command builders | 13 typed structs | none | basic | none |
| Event types | ![Event Types](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/event-type-count.json) | — | — | — |
| Test count | ![Tests](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/test-count.json) | — | — | — |

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
