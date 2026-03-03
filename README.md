# freeswitch-esl-tokio

[![CI](https://github.com/ticpu/freeswitch-esl-tokio/actions/workflows/ci.yml/badge.svg)][ci]
[![Tests][tests-badge]][ci]
[![crates.io](https://img.shields.io/crates/v/freeswitch-esl-tokio)](https://crates.io/crates/freeswitch-esl-tokio)
[![docs.rs](https://img.shields.io/docsrs/freeswitch-esl-tokio)][docs]

| C-verified enums | Typed API |
|---|---|
| [![EslEventType][evt-badge]][ci] [![HangupCause][hc-badge]][ci] | [![EventHeader][eh-badge]][docs] [![ChannelVariable][cv-badge]][docs] |
| [![ChannelState][cs-badge]][ci] [![CallState][ccs-badge]][ci] | [![HeaderLookup][hl-badge]][docs] |
| [![SipInviteHeader][sih-badge]][ci] | [![SofiaVariable][sv-badge]][docs] |

[ci]: https://github.com/ticpu/freeswitch-esl-tokio/actions/workflows/ci.yml
[docs]: https://docs.rs/freeswitch-esl-tokio
[tests-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/test-count.json
[evt-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/event-type-count.json
[hc-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/hangup-cause-count.json
[cs-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/channel-state-count.json
[ccs-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/call-state-count.json
[eh-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/event-header-count.json
[cv-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/channel-var-count.json
[hl-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/header-lookup-count.json
[sih-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/sip-invite-header-count.json
[sv-badge]: https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/ticpu/def178758b6a88effff310aca87b6b50/raw/sofia-variable-count.json

Async Rust client for FreeSWITCH
[ESL](https://developer.signalwire.com/freeswitch/FreeSWITCH-Explained/Client-and-Developer-Interfaces/Event-Socket-Library/).
Typed endpoints, typed events, serde support, split reader/writer, liveness
detection.

```rust
use std::time::Duration;
use freeswitch_esl_tokio::*;
use freeswitch_esl_tokio::commands::*;

#[tokio::main]
async fn main() -> Result<(), EslError> {
    let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

    client.subscribe_events(EventFormat::Plain, &[
        EslEventType::BackgroundJob,
        EslEventType::ChannelCreate,
        EslEventType::ChannelDestroy,
    ]).await?;

    let cmd = Originate::application(
        Endpoint::SofiaGateway(SofiaGateway::new("my_provider", "18005551234")),
        Application::new(
            "playback",
            Some("/usr/share/freeswitch/sounds/en/us/callie/ivr/ivr-welcome.wav"),
        ),
    )
    .timeout(Duration::from_secs(30));

    let response = client.bgapi(&cmd.to_string()).await?;
    response.into_result()?;

    while let Some(Ok(event)) = events.recv().await {
        let Some(event_type) = event.event_type() else { continue };
        match event_type {
            EslEventType::ChannelCreate => {
                println!("channel created: {}", event.channel_name().unwrap_or("?"));
            }
            EslEventType::ChannelDestroy => {
                let cause = match event.hangup_cause() {
                    Ok(Some(c)) => c.to_string(),
                    _ => "unknown".into(),
                };
                println!("channel destroyed: {} ({})",
                    event.channel_name().unwrap_or("?"), cause);
                break;
            }
            EslEventType::BackgroundJob => {
                println!("bgapi result: {}", event.body().unwrap_or(""));
            }
            _ => {}
        }
    }
    Ok(())
}
```

```toml
[dependencies]
freeswitch-esl-tokio = "2"
tokio = { version = "1.0", features = ["full"] }
```

## Features

- **Split reader/writer** -- `EslClient` is `Clone + Send`, events arrive on
  a separate channel. Send commands from any task without blocking the event loop.
- **Typed endpoints** -- `SofiaEndpoint`, `SofiaGateway`, `LoopbackEndpoint`,
  `UserEndpoint`, `SofiaContact`, `GroupCall`, `ErrorEndpoint` with a
  `DialString` trait. Extensible by downstream crates.
- **Typed events** -- `ChannelState`, `CallDirection`, `EventHeader`,
  `ChannelVariable` enums. `HeaderLookup` trait gives typed accessors to any
  key-value store, not just `EslEvent`.
- **Command builders** -- `Originate`, `BridgeDialString`, `UuidKill`,
  `ConferenceDtmf`, dptools -- all `Display`/`FromStr`, no transport coupling.
- **Serde** -- all builder types implement `Serialize`/`Deserialize`.
  Config-driven originate and bridge from YAML/JSON.
- **Connection health** -- liveness detection, command timeouts (default 5s),
  `is_connection_error()` / `is_recoverable()` error classification.
- **Correct wire format** -- two-part framing, percent-decoded headers,
  Content-Type detection. Matches `mod_event_socket.c`.
- **Re-exec support** (Unix) -- `teardown_for_reexec()` extracts the socket fd
  and residual parser bytes; `adopt_stream()` reconstructs the client in the
  new binary. Zero-downtime upgrades without dropping the ESL connection.

## Architecture

```
connect() -> (EslClient, EslEventStream)

EslClient (Clone + Send)         EslEventStream
|- send commands from any task    |- events via mpsc channel
|- writer half behind Arc<Mutex>  '- connection status via watch
'- replies via oneshot channel

Background reader task
|- owns the read half + parser
|- routes CommandReply/ApiResponse -> pending oneshot
|- routes Event -> mpsc channel
|- tracks liveness (any TCP traffic resets timer)
'- broadcasts ConnectionStatus on disconnect
```

See [docs/design-rationale.md](docs/design-rationale.md) for the full story.

## Usage

### Inbound connection

```rust
let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

let response = client.api("status").await?;
println!("{}", response.body_string());
```

Multi-tenant with per-user ACL:

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
use freeswitch_esl_tokio::HeaderLookup;

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
use freeswitch_esl_tokio::{EslClient, AppCommand, EventFormat, EventHeader, HeaderLookup};
use tokio::net::TcpListener;

let listener = TcpListener::bind("0.0.0.0:8040").await?;
let (client, mut events) = EslClient::accept_outbound(&listener).await?;

// Must be the first command after accept, returns channel info as an EslResponse
let channel_data = client.connect_session().await?;
// Channel-Name is always present in connect response
println!("Channel: {}", channel_data.header(EventHeader::ChannelName).unwrap());

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

## Command builders

Typed builders for FreeSWITCH API commands. All implement `Display`, are
independent of `EslClient`, and can be unit tested without a connection.

### Endpoint types

Each endpoint type is a concrete struct implementing the `DialString` trait.
The `Endpoint` enum wraps them for polymorphic storage and serde.

| Type | Wire format | Description |
|---|---|---|
| `SofiaEndpoint` | `sofia/{profile}/{destination}` | Direct SIP profile routing |
| `SofiaGateway` | `sofia/gateway/{gateway}/{destination}` | SIP gateway routing |
| `LoopbackEndpoint` | `loopback/{extension}/{context}` | Internal loopback |
| `UserEndpoint` | `user/{name}@{domain}` | Directory-based dial-string lookup |
| `SofiaContact` | `${sofia_contact(user@domain)}` | Resolve registered contacts (FS runtime expression) |
| `GroupCall` | `${group_call(group@domain+A)}` | Resolve group members (FS runtime expression) |
| `ErrorEndpoint` | `error/{cause}` | Bridge to hangup cause |

```rust
use freeswitch_esl_tokio::commands::*;

// Direct SIP profile routing
let ep = Endpoint::Sofia(SofiaEndpoint::new("internal", "1000@domain.com"));
assert_eq!(ep.to_string(), "sofia/internal/1000@domain.com");

// SIP gateway routing
let ep = Endpoint::SofiaGateway(SofiaGateway::new("my_provider", "18005551234"));
assert_eq!(ep.to_string(), "sofia/gateway/my_provider/18005551234");

// Parse from wire format
let ep: Endpoint = "sofia/gateway/my_provider/18005551234".parse().unwrap();

// Downstream crates can implement DialString on custom endpoint types
```

### Originate

```rust
use freeswitch_esl_tokio::commands::*;

let gw = || Endpoint::SofiaGateway(SofiaGateway::new("my_provider", "18005551234"));

// Inline applications
let cmd = Originate::inline(gw(), vec![
    Application::new("conference", Some("room1")),
]).unwrap();
// -> "originate sofia/gateway/my_provider/18005551234 conference:room1 inline"

// Extension target with dialplan and context
let ext_cmd = Originate::extension(gw(), "1000")
    .dialplan(DialplanType::Xml).unwrap()
    .context("default");
// -> "originate sofia/gateway/my_provider/18005551234 1000 XML default"
client.bgapi(&cmd.to_string()).await?;

// Round-trip: parse <-> display
let parsed: Originate = cmd.to_string().parse().unwrap();
assert_eq!(parsed.to_string(), cmd.to_string());
```

### Bridge dial strings

`BridgeDialString` builds multi-endpoint bridge arguments with simultaneous
ring (`,`) and sequential failover (`|`):

```rust
use freeswitch_esl_tokio::commands::*;

// Try primary and secondary simultaneously, then failover to backup
let bridge = BridgeDialString::new(vec![
    vec![
        Endpoint::SofiaGateway(SofiaGateway::new("primary", "18005551234")),
        Endpoint::SofiaGateway(SofiaGateway::new("secondary", "18005551234")),
    ],
    vec![Endpoint::SofiaGateway(SofiaGateway::new("backup", "18005551234"))],
]);
// -> "sofia/gateway/primary/18005551234,sofia/gateway/secondary/18005551234|sofia/gateway/backup/18005551234"

// Use with the bridge dptools application
client.send_command(AppCommand::bridge(bridge)).await?;
```

See [docs/dial-string-format.md](docs/dial-string-format.md) for the complete
dial string reference (variable scoping, `^^:` custom delimiters, enterprise
`:_:` originate).

### UUID and conference commands

```rust
use freeswitch_esl_tokio::commands::*;
use freeswitch_esl_tokio::HangupCause;

// UUID commands
let kill = UuidKill::with_cause(uuid, HangupCause::NormalClearing);
// -> "uuid_kill <uuid> NORMAL_CLEARING"
client.api(&kill.to_string()).await?;

// Conference commands
let dtmf = ConferenceDtmf::new("room1", "all", "1");
// -> "conference room1 dtmf all 1"
client.api(&dtmf.to_string()).await?;
```

> Output strings verified by unit tests in
> [`commands/originate.rs`](freeswitch-types/src/commands/originate.rs),
> [`commands/endpoint/`](freeswitch-types/src/commands/endpoint/),
> [`commands/bridge.rs`](freeswitch-types/src/commands/bridge.rs),
> [`commands/channel.rs`](freeswitch-types/src/commands/channel.rs), and
> [`commands/conference.rs`](freeswitch-types/src/commands/conference.rs).

See [docs/command-builders.md](docs/command-builders.md) for the full builder
architecture, all channel/conference command types, and escaping rules.

## Config-driven commands (serde)

All command builder types implement `Serialize`/`Deserialize`, so originate
and bridge commands can be driven entirely from config files:

```json
{
  "endpoint": {
    "sofia_gateway": {
      "gateway": "my_provider",
      "destination": "18005551234"
    }
  },
  "application": {"name": "park"},
  "timeout_secs": 30
}
```

```rust
let originate: Originate = serde_json::from_str(json)?;
client.bgapi(&originate.to_string()).await?;
```

`Variables` deserializes ergonomically -- a flat map defaults to `Default` scope:

```json
{"originate_timeout": "600", "sip_h_X-Custom": "value"}
```

Other scopes use the explicit form:

```json
{"scope": "enterprise", "vars": {"key": "value"}}
```

### YAML serialization

The `Endpoint` enum uses serde's externally tagged newtype variants. With
`serde_json` this produces `{"sofia": {...}}`. YAML libraries that use YAML
tags for externally tagged enums (e.g. `serde_yml`) will produce:

```yaml
endpoint: !sofia
  profile: internal
  destination: "1000@domain.com"
```

This is valid YAML and round-trips correctly, but differs from the
`{"sofia": {...}}` mapping format that `serde_json` produces.

## Variable parsers

```rust
use freeswitch_esl_tokio::variables::{EslArray, MultipartBody, SipInviteHeader};
use freeswitch_esl_tokio::HeaderLookup;

// ARRAY:: delimited values (used by FreeSWITCH for repeating SIP headers)
let arr = EslArray::parse("ARRAY::item1|:item2|:item3").unwrap();
assert_eq!(arr.items(), &["item1", "item2", "item3"]);

// Raw SIP INVITE headers (requires parse-all-invite-headers on the sofia profile)
// ARRAY headers like P-Asserted-Identity may have multiple values
let pai = event.variable(SipInviteHeader::PAssertedIdentity);
if let Some(raw) = pai {
    if let Some(arr) = EslArray::parse(raw) {
        for identity in arr.items() {
            println!("P-Asserted-Identity: {}", identity);
        }
    }
}

// SIP multipart body extraction
let body = MultipartBody::parse(raw_multipart).unwrap();
let pidf = body.by_mime_type("application/pidf+xml");
```

> Verified in [`variables/esl_array.rs`](freeswitch-types/src/variables/esl_array.rs),
> [`variables/sip_invite.rs`](freeswitch-types/src/variables/sip_invite.rs), and
> [`variables/sip_multipart.rs`](freeswitch-types/src/variables/sip_multipart.rs).

## Typed event accessors

`EslEvent` provides typed accessors that parse header values into enums
instead of returning raw strings:

```rust
use freeswitch_esl_tokio::{ChannelState, CallDirection, HeaderLookup};

// Typed enums parsed from headers, no string matching needed
if let Ok(Some(state)) = event.channel_state() {
    match state {
        ChannelState::CsExecute => println!("Executing app"),
        ChannelState::CsHangup => println!("Hanging up"),
        _ => {}
    }
}

// String accessors return Option<&str>: None if the header is absent
let cid = event.caller_id_number();     // Option<&str>
// Typed accessors return Result<Option<T>, ParseErr>
let direction = event.call_direction(); // Result<Option<CallDirection>, _>
let cause = event.hangup_cause();       // Result<Option<HangupCause>, _>
```

### Channel timetable

Call lifecycle timestamps via `ChannelTimetable`:

```rust
use freeswitch_esl_tokio::{HeaderLookup, TimetablePrefix};

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

### Header and variable enums

Compile-time header and variable name enums via `HeaderLookup`:

```rust
use freeswitch_esl_tokio::{HeaderLookup, EventHeader, ChannelVariable};

// HeaderLookup trait provides typed enum lookups on EslEvent
let uid = event.header(EventHeader::UniqueId);             // Option<&str>
let codec = event.variable(ChannelVariable::ReadCodec);    // Option<&str>
```

### Custom channel tracker with `HeaderLookup`

The `HeaderLookup` trait lets any `HashMap<String, String>` wrapper share
the same typed accessors as `EslEvent`. Implement two methods, get all typed
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

- `cargo fmt --check` -- formatting
- `cargo clippy -- -D warnings` -- lint warnings as errors
- `RUSTDOCFLAGS="-D missing_docs" cargo doc` -- all public items documented
- `hooks/check-event-types.sh` -- `EslEventType` enum matches C ESL `EVENT_NAMES[]`
- `hooks/check-sip-invite-headers.sh` -- `SipInviteHeader` enum matches `sofia_parse_all_invite_headers()`

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

## Other Rust ESL crates

- [freeswitch-esl](https://crates.io/crates/freeswitch-esl) -- async/tokio,
  JSON-only events, no split reader/writer, no liveness detection, no command
  builders or typed state. Stale since 2023.
- [eslrs](https://crates.io/crates/eslrs) -- async, still in RC. Unified
  stream (not split), silently discards unexpected responses, no timeouts.
- [freeswitch-esl-rs](https://crates.io/crates/freeswitch-esl-rs) --
  synchronous/blocking, inbound only, plain events only.

None of them offer typed endpoints, serde support, command builders,
`HeaderLookup`, or connection health monitoring.

## License

MIT OR Apache-2.0 -- see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
