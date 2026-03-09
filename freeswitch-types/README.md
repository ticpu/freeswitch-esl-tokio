# freeswitch-types

[![crates.io](https://img.shields.io/crates/v/freeswitch-types)](https://crates.io/crates/freeswitch-types)
[![docs.rs](https://img.shields.io/docsrs/freeswitch-types)](https://docs.rs/freeswitch-types)

FreeSWITCH protocol types: channel state, events, headers, command builders,
and variable parsers. No async runtime dependency.

Use this crate standalone for CDR parsing, config generation, command building,
or channel variable validation without pulling in tokio.

For async ESL transport (connecting to FreeSWITCH, sending commands, receiving
events), see [`freeswitch-esl-tokio`](https://crates.io/crates/freeswitch-esl-tokio)
which re-exports everything from this crate.

## What's included

| Module | Contents |
|--------|----------|
| `channel` | `ChannelState`, `CallState`, `AnswerState`, `CallDirection`, `HangupCause`, `ChannelTimetable` |
| `event` | `EslEvent`, `EslEventType`, `EventFormat`, `EslEventPriority` |
| `headers` | `EventHeader` enum (typed event header names) |
| `lookup` | `HeaderLookup` trait (typed accessors for any key-value store) |
| `commands` | `Originate`, `BridgeDialString`, `UuidKill`, `UuidBridge`, endpoint types, `DialString` trait |
| `variables` | `ChannelVariable`, `SofiaVariable`, `EslArray`, `MultipartBody` |

## Features

- **`serde`** (enabled by default) — adds `Serialize`/`Deserialize` impls for
  all public types. Disable with `default-features = false` if you only need
  wire-format parsing (`Display`/`FromStr`) without pulling in serde.

## Usage

```toml
[dependencies]
freeswitch-types = "1"
```

Without serde:

```toml
[dependencies]
freeswitch-types = { version = "1", default-features = false }
```

### Command builders

```rust
use std::time::Duration;
use freeswitch_types::commands::*;

let cmd = Originate::application(
    Endpoint::SofiaGateway(SofiaGateway::new("my_provider", "18005551234")),
    Application::simple("park"),
)
.cid_name("Outbound Call")
.cid_num("5551234")
.timeout(Duration::from_secs(30));

// All builders implement Display, producing the FreeSWITCH wire format
assert!(cmd.to_string().starts_with("originate sofia/gateway/"));

// All builders implement FromStr for round-trip parsing
let parsed: Originate = cmd.to_string().parse().unwrap();
assert_eq!(parsed.to_string(), cmd.to_string());
```

### Typed event accessors

```rust
use freeswitch_types::{HeaderLookup, EventHeader, ChannelVariable};

// HeaderLookup works with any key-value store, not just EslEvent
struct MyHeaders(std::collections::HashMap<String, String>);

impl HeaderLookup for MyHeaders {
    fn header_str(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(|s| s.as_str())
    }
    fn variable_str(&self, name: &str) -> Option<&str> {
        self.0.get(&format!("variable_{}", name)).map(|s| s.as_str())
    }
}

// Now MyHeaders has all typed accessors:
// h.channel_state(), h.call_direction(), h.hangup_cause(),
// h.header(EventHeader::UniqueId), h.variable(ChannelVariable::ReadCodec), etc.
```

### Serde support (requires `serde` feature, enabled by default)

All builder types implement `Serialize`/`Deserialize` for config-driven usage:

```rust
use freeswitch_types::Originate;

let json = r#"{
    "endpoint": {"sofia_gateway": {"gateway": "carrier", "destination": "18005551234"}},
    "application": {"name": "park"},
    "timeout_secs": 30
}"#;
let cmd: Originate = serde_json::from_str(json).unwrap();
println!("{}", cmd);
```

### Variable parsers

```rust
use freeswitch_types::variables::{EslArray, MultipartBody};

let arr = EslArray::parse("ARRAY::item1|:item2|:item3").unwrap();
assert_eq!(arr.items(), &["item1", "item2", "item3"]);
```

## Relationship to freeswitch-esl-tokio

This crate contains all domain types extracted from the
[`freeswitch-esl-tokio`](https://crates.io/crates/freeswitch-esl-tokio)
workspace. The ESL crate re-exports everything, so users of `freeswitch-esl-tokio`
don't need to depend on this crate directly.

Depend on `freeswitch-types` directly when you need FreeSWITCH types without
async transport (CDR processors, config validators, CLI tools, test harnesses).

## License

MIT OR Apache-2.0
