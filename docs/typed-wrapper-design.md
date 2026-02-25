# Typed ESL Wrapper Crate Design

This document outlines the architecture for a future typed wrapper crate that
builds on `freeswitch-esl-tokio` to provide compile-checked ESL usage.

## Goal

Replace stringly-typed ESL interactions with fully typed Rust APIs:

```rust
// Current (transport-layer only):
client.api(&UuidKill { uuid: id.into(), cause: Some("NORMAL_CLEARING".into()) }.to_string()).await?;

// Future wrapper:
client.channel(&uuid).kill(HangupCause::NormalClearing).await?;
```

## Architectural Boundary

`freeswitch-esl-tokio` (this crate) is **transport only**: wire format, framing,
event delivery, and raw `api()`/`bgapi()`. It does not parse API response bodies
into typed structs.

The wrapper crate depends on this crate and adds:

- Typed command methods that send commands and parse responses
- Value enums (`HangupCause`, `ApplicationName`) for command parameters
- Response parsers (`StatusResponse`, `SofiaProfile`, `ShowChannels`)
- Handle objects (`ChannelHandle`, `ConferenceHandle`) with scoped methods

## How This Crate Enables the Wrapper

### VariableName trait extensibility

The wrapper can define its own variable enums for module-specific variables:

```rust
// In wrapper crate:
define_header_enum! {
    error_type: ParseConferenceVariableError,
    pub enum ConferenceVariable {
        ConferenceName => "conference_name",
        ConferenceMemberFlags => "conference_member_flags",
        // ...
    }
}

impl VariableName for ConferenceVariable {
    fn as_str(&self) -> &str { ConferenceVariable::as_str(self) }
}

// Works with EslEvent::variable() from freeswitch-esl-tokio:
event.variable(ConferenceVariable::ConferenceName)
```

### Command builders use Display

`UuidKill`, `Originate`, `AppCommand`, etc. all produce strings via `Display`.
The wrapper wraps them, not replaces them. A typed `HangupCause` enum converts
to the string the builder needs:

```rust
impl fmt::Display for HangupCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HangupCause::NormalClearing => write!(f, "NORMAL_CLEARING"),
            // ...
        }
    }
}
```

### No circular dependency

`commands/` doesn't import `connection.rs`. The wrapper depends on both the
command builders and the transport layer independently.

## Planned Typed Enums

These are **not** in `freeswitch-esl-tokio` yet but are natural next steps.
They could live in either crate depending on whether the transport layer
benefits from them directly.

### HangupCause (Q.850 causes)

Used by `UuidKill::cause`, `AppCommand::hangup()`, and the `hangup_cause()`
accessor. FreeSWITCH defines these in `switch_channel.h`:

```rust
pub enum HangupCause {
    NormalClearing,         // NORMAL_CLEARING
    UserBusy,               // USER_BUSY
    NoAnswer,               // NO_ANSWER
    CallRejected,           // CALL_REJECTED
    UnallocatedNumber,      // UNALLOCATED_NUMBER
    NoRouteDestination,     // NO_ROUTE_DESTINATION
    OriginatorCancel,       // ORIGINATOR_CANCEL
    // ~50 more Q.850 causes
}
```

**Strong candidate for this crate** since `UuidKill` and `AppCommand` already
accept cause strings. Changing `cause: Option<String>` to
`cause: Option<impl Display>` is backward-compatible.

### ApplicationName (dptools)

For type-safe `AppCommand` construction:

```rust
pub enum ApplicationName {
    Answer,
    Hangup,
    Bridge,
    Playback,
    Set,
    Park,
    Transfer,
    // ...
}
```

### Typed Response Parsers

These belong in the wrapper crate since they depend on parsing strategy
(XML output, regex, etc.) and may pull in heavier dependencies:

- `StatusResponse` -- from `api status`
- `SofiaProfile` -- from `api sofia status`
- `ShowChannels` -- from `api show channels as xml`
- `OriginateResult` -- parsed originate response with UUID extraction

## Wrapper API Sketch

```rust
pub struct TypedClient {
    inner: EslClient,
}

impl TypedClient {
    pub fn channel(&self, uuid: &str) -> ChannelHandle<'_> {
        ChannelHandle { client: &self.inner, uuid: uuid.to_string() }
    }

    pub async fn status(&self) -> Result<StatusResponse, EslError> {
        let resp = self.inner.api("status").await?;
        StatusResponse::parse(resp.body().unwrap_or(""))
    }

    pub async fn originate(&self, cmd: &Originate) -> Result<OriginateResult, EslError> {
        let resp = self.inner.api(&cmd.to_string()).await?;
        OriginateResult::parse(&resp)
    }
}

pub struct ChannelHandle<'a> {
    client: &'a EslClient,
    uuid: String,
}

impl ChannelHandle<'_> {
    pub async fn kill(&self, cause: HangupCause) -> Result<EslResponse, EslError> {
        let cmd = UuidKill { uuid: self.uuid.clone(), cause: Some(cause.to_string()) };
        self.client.api(&cmd.to_string()).await
    }

    pub async fn set_var(&self, name: impl VariableName, value: &str) -> Result<EslResponse, EslError> {
        let cmd = UuidSetVar { uuid: self.uuid.clone(), name: name.as_str().into(), value: value.into() };
        self.client.api(&cmd.to_string()).await
    }

    pub async fn bridge(&self, other: &str) -> Result<EslResponse, EslError> {
        let cmd = UuidBridge { uuid: self.uuid.clone(), other: other.into() };
        self.client.api(&cmd.to_string()).await
    }
}
```

## String-Typed Fields to Migrate

Current command builders that accept raw strings where typed enums would help:

| Field | Current Type | Future Type |
|-------|-------------|-------------|
| `UuidKill::cause` | `Option<String>` | `Option<HangupCause>` |
| `AppCommand::hangup(cause)` | `Option<&str>` | `Option<HangupCause>` |
| `EslCommand::Execute { app }` | `String` | `ApplicationName` or `impl Display` |
| `ErrorEndpoint::cause` | `String` | `HangupCause` |

These can be migrated with `impl Display` or `impl Into<String>` generics to
maintain backward compatibility.
