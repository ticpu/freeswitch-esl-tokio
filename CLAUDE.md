## Project Type

This is a **library-first** crate. There is an examples/ folder buildable binaries
`Cargo.lock` is gitignored per Cargo convention for libraries.

## `#[non_exhaustive]` Policy

All public enums and public structs with public fields have `#[non_exhaustive]`.
When adding a new public struct or enum, **always** add `#[non_exhaustive]`.

Because `#[non_exhaustive]` prevents struct literal construction from external
crates (including `examples/`), every public struct must have a constructor
(`new()` or named constructors). Optional fields use builder methods
(`with_foo()`). **Always run `cargo build --examples`** after adding or
modifying public structs to verify external construction still works.

## API Boundary Rules

- **Never expose dependency types in public signatures.** Return `impl Iterator`
  (not `indexmap::map::Iter`), wrap dependency errors (not `#[from] serde_json::Error`).
  A dependency major-version bump becomes a semver break if its types leak.
- **`pub(crate)` modules can still leak types.** If a public function returns a
  type from a `pub(crate)` module, that type is visible but unnameable by callers.
  Either re-export the type or don't return it.
- **Struct fields that control behavior should be private.** Expose via accessor
  methods (e.g. `scope()` not `pub vars_type`). This prevents callers from
  mutating invariants after construction.
- **`constants` module is `pub(crate)`.** Only `DEFAULT_ESL_PORT` is re-exported.
  Internal protocol constants are implementation details.

## Method Signature Conventions

- **`impl Into<String>` for string setters.** Functions that store a string
  should accept `impl Into<String>` so callers can pass `&str` or `String`.
- **`Duration` for timeouts.** Never use raw `u32` seconds. Consistent with
  `set_liveness_timeout()` / `set_command_timeout()`.
- **`impl IntoIterator` with `Borrow` for Copy types.** When a method accepts
  a collection of `Copy` types (e.g. `EslEventType`), use
  `impl IntoIterator<Item = impl Borrow<T>>` so callers can pass `&[T]`,
  `Vec<T>`, arrays, `HashSet<T>`, or `&const_slice`.
- **Case-insensitive `FromStr`.** All `FromStr` impls for wire format types
  (`DialplanType`, `EventFormat`, `EslEventType`, etc.) use
  `eq_ignore_ascii_case`. `Display` emits the canonical form.
- **`HeaderLookup` on response types.** Any type with ESL headers should
  implement `HeaderLookup` for typed accessor access. `EslResponse` and
  `EslEvent` both implement it.
- **`From<Concrete> for Enum`.** Endpoint variant enums should have `From`
  impls for each concrete type (e.g. `From<SofiaEndpoint> for Endpoint`).
- **Typed + raw method pairs.** When an `EslClient` method accepts a typed
  enum (`EventHeader`, `EslEventType`), provide a `_raw(&str)` companion for
  headers/events not yet in the enum. Pattern: `filter()` / `filter_raw()`,
  `subscribe_events()` / `subscribe_events_raw()`.
- **Options structs for optional wire headers.** When a command has optional
  protocol headers (e.g. `event-lock`, `async`, `loops` for `sendmsg execute`),
  use an `Options` struct with builder methods rather than adding parameters.
  Keep the base method simple; add `_with_options()` variant.
- **Preserve wire context in error/status enums.** Disconnect notices, auth
  responses, and other protocol messages may carry useful context (headers,
  body). Always preserve it for the caller rather than discarding.
- **Crate root re-exports: core and dptools only.** Only re-export types from
  `lib.rs` that belong to FreeSWITCH core or dptools (channel state, events,
  originate, endpoints, `Uuid*` commands, `BridgeDialString`). Module-specific
  types like conference commands (`ConferenceMute`, `MuteAction`) stay in their
  submodule (`commands::conference`) and are not re-exported from the crate root.
  Similarly, Sofia-specific types (`SofiaVariable`, `SofiaEndpoint`, etc.) stay
  in `variables::sofia` / `commands::endpoint::sofia`.

## Build & Test Workflow

**Always run `cargo fmt` before every commit.** The pre-commit hook enforces
formatting, clippy warnings, all tests (including doctests), `-D missing_docs`
doc coverage, and EslEventType sync with C ESL.

**When adding new `EslEventType` variants**, check whether they belong in any
of the event group constants (`CHANNEL_EVENTS`, `MEDIA_EVENTS`,
`PRESENCE_EVENTS`, `SYSTEM_EVENTS`, `CONFERENCE_EVENTS`) in `src/event.rs`
and update them accordingly.

```sh
cargo fmt
cargo check --message-format=short
cargo clippy --fix --allow-dirty --message-format=short
cargo test --lib
```

## Release Workflow

Before tagging a release:

```sh
cargo clippy --release -- -D warnings
cargo test --release
cargo build --release
```

Tag with a signed annotated tag. Include a brief changelog in the tag message:

```sh
git tag -as v0.X.0 -m "v0.X.0

- Brief changelog entry
- Another change"
git push --tags
```

**Never `cargo publish` without completing these steps first:**

1. Create a signed annotated tag (`git tag -as`)
2. Push the tag (`git push --tags`)
3. Wait for CI to pass on the tagged commit
4. Only then `cargo publish`

## Documentation Style

All public items must have doc comments — the pre-commit hook enforces
`-D missing_docs`. Brief one-liners are fine for self-evident items.

No "captain obvious" docs. Don't restate the struct/function name as the doc comment.
Only document when it adds value: non-obvious behavior, FreeSWITCH-specific semantics,
wire format details, gotchas. Silence over noise. If the name and signature tell the
whole story, a brief one-liner suffices.

**No hardcoded counts in prose.** Don't write "26 variants" or "54 variables" in
markdown files or comments — these go stale when variants are added. Use dynamic
badges (CI-generated) in README or just omit the count.

## Logging Accuracy

Logged wire data must be accurate. Never use `.trim()` on wire content for
cosmetic reasons — it can eat meaningful whitespace. Strip only known protocol
suffixes by name (e.g. `strip_suffix(HEADER_TERMINATOR)`).

## Library Code Rules

**No `assert!`/`panic!`/`unwrap()` in library code** outside of tests. This is
a library crate — panics crash the caller's application. Return `Result` or
`Option` instead. The only exception is logic errors that truly cannot happen
(document why with a comment), and even then prefer `debug_assert!`.

## Correctness Over Recovery

Correctness is the highest priority. Never silently absorb protocol violations
or leave the system in an unknown state to "recover." If an invariant is broken
(e.g. missing mandatory header, impossible framing), return an error and let
the caller disconnect. A clean reconnection from a known-good state is always
preferable to continuing with a potentially corrupt stream.

Concretely: never use `unwrap_or` / default values to paper over missing
mandatory protocol fields. If the ESL spec says a field must be present,
its absence is a hard error — not a recoverable condition.

Never use `.parse().ok()` to silently discard parse errors on protocol
data. If a header is present but its value doesn't parse, that's a
protocol violation — return `Err`, don't collapse it into `None` where
it becomes indistinguishable from a missing header.

## Design Principles

### Single responsibility — no coupling to `EslEvent`

Data types and parsers must not depend on `EslEvent` when a generic interface
suffices. If a function only needs `header(&str) -> Option<&str>`, accept a
closure or trait — not `&EslEvent`. This lets callers use the same logic with
`HashMap`, `BTreeMap`, or any other key-value store without going through
`EslEvent`.

Concrete example: `ChannelTimetable::from_lookup(prefix, |k| map.get(k).map(…))`
works with any data source. `from_event()` is a convenience wrapper, not the
primary API.

### Transport layer (connection, protocol, event)

- **Split reader/writer**: Background reader task + channel-based event delivery.
  `EslClient` is Clone+Send for commands; `EslEventStream` for events.
- **Liveness detection**: Any inbound TCP traffic resets the timer. HEARTBEAT
  subscription ensures idle-connection traffic. `set_liveness_timeout()` to enable.
- **Command timeout**: Default 5s timeout on all commands. `set_command_timeout()`.
  Cleans up pending reply slot on timeout so subsequent commands aren't blocked.
- **No automatic reconnection**: The library detects disconnection via
  `ConnectionStatus`/`DisconnectReason`. The caller controls reconnection strategy.
- **Error classification**: `is_connection_error()` / `is_recoverable()` let callers
  decide handling without matching every variant.
- **Correct wire format**: Events use two-part framing (outer envelope + body).
  Header values are percent-decoded. Event format determined from Content-Type.

### Command builders (commands/, app/, variables/)

- **Pure `Display`/`FromStr`**: No transport coupling. Builders produce strings,
  `EslClient` calls `.to_string()`. Enables round-trip unit testing without ESL.
- **`app/`** = sendmsg-based dptools (answer, hangup, bridge, etc.)
- **`commands/`** = API command strings for `api()`/`bgapi()` (originate, uuid_*, conference)
- **`variables/`** = typed variable name enums (`ChannelVariable`, `SofiaVariable`,
  `VariableName` trait) and format parsers (ARRAY::, SIP multipart)
- **Foundation for extension**: Application-specific crates (NGCS, X-Call-Info, SIP
  URI) can depend on these base types without reimplementing escaping or parsing.

### Architectural boundary: core vs wrapper

This crate (`freeswitch-esl-tokio`) is **transport only**: wire format, framing,
event delivery, and raw `api()`/`bgapi()`. It does not parse API response bodies
into typed structs.

A future **wrapper crate** will own the typed command-and-response layer:

- Depends on this crate for transport and on `commands/` for command builders
- Provides typed methods (`client.status()`, `client.sofia_status()`,
  `client.show_channels()`) that send the command and parse the response
- Each method returns a parsed struct (`StatusResponse`, `SofiaProfile`, etc.)
- Uses XML output variants where available for reliable parsing
- Can pull in heavier deps (regex, serde) without bloating the core

**Do not add response parsing or high-level command methods to `EslClient`.**
Keep the boundary clean — `EslClient` sends strings and returns `EslResponse`.

## Source Layout

```
src/
├── lib.rs                 # Public API re-exports
├── connection.rs          # EslClient, EslEventStream, connect()/accept_outbound()
├── protocol.rs            # Wire format parser (framing, percent-decoding)
├── buffer.rs              # Streaming read buffer with Content-Length framing
├── command.rs             # EslCommand, CommandBuilder, EslResponse
├── event.rs               # EslEvent, EslEventType (synced with C ESL EVENT_NAMES[])
├── error.rs               # EslError, DisconnectReason, error classification
├── channel.rs             # ChannelState, CallState, AnswerState, CallDirection, ChannelTimetable
├── headers.rs             # EventHeader enum
├── lookup.rs              # HeaderLookup trait — typed accessors for any key-value store
├── constants.rs           # Wire format constants, timeouts, buffer sizes
├── macros.rs              # define_header_enum! macro
├── app/
│   ├── mod.rs
│   └── dptools.rs         # AppCommand — answer, hangup, bridge, playback, ...
├── commands/              # API command string builders (→ api()/bgapi())
│   ├── mod.rs             # Re-exports, originate_quote/unquote, originate_split()
│   ├── originate.rs       # Variables, Application, OriginateTarget, Originate
│   ├── endpoint/          # Endpoint types (DialString trait, Endpoint enum)
│   │   ├── mod.rs         # DialString trait, Endpoint enum, helpers
│   │   ├── sofia.rs       # SofiaEndpoint, SofiaGateway, SofiaContact
│   │   ├── loopback.rs    # LoopbackEndpoint
│   │   ├── user.rs        # UserEndpoint
│   │   ├── audio.rs       # AudioEndpoint (portaudio/pulseaudio/alsa)
│   │   ├── group_call.rs  # GroupCall
│   │   └── error.rs       # ErrorEndpoint
│   ├── channel.rs         # UuidAnswer, UuidBridge, UuidKill, UuidSetVar, ...
│   └── conference.rs      # ConferenceMute, ConferenceHold, ConferenceDtmf
└── variables/             # Channel variable format parsers and typed name enums
    ├── mod.rs             # VariableName trait, re-exports
    ├── core.rs            # ChannelVariable enum (core FreeSWITCH variables)
    ├── sofia.rs           # SofiaVariable enum (mod_sofia / SIP variables)
    ├── esl_array.rs       # ARRAY::item1|:item2 format
    └── sip_multipart.rs   # SIP multipart body extraction

tests/
├── integration_tests.rs   # Mock-server protocol tests
├── connection_tests.rs    # Connection lifecycle, timeouts, liveness
├── live_freeswitch.rs     # Real ESL tests (ignored without FreeSWITCH)
└── mock_server.rs         # Test harness simulating ESL server

examples/
├── channel_tracker.rs     # Channel lifecycle monitoring with typed accessors
├── event_listener.rs      # Subscribe and print events
├── event_filter.rs        # Event filtering demo
├── inbound_client.rs      # Basic inbound ESL client
├── outbound_server.rs     # Outbound ESL server
└── outbound_test.rs       # Outbound mode integration test
```

## Outbound ESL Mode

See [docs/outbound-esl-quirks.md](docs/outbound-esl-quirks.md) for details.

- `connect_session()` must be the first command after `accept_outbound()`
- `async full` mode required for api/bgapi/linger/event commands
- Socket app args need quoting in originate — `Originate` builder handles this
  via `originate_quote()`/`originate_unquote()` in `commands/mod.rs`
- `cargo run --example outbound_test` exercises outbound against real FS on port 8022

## Live Integration Tests

When FreeSWITCH ESL is available on `127.0.0.1:8022` (password `ClueCon`),
run the live tests after unit tests pass:

```sh
cargo test --test live_freeswitch -- --ignored
```

These tests exercise real ESL connections: auth, api commands, event
subscription, sendevent with priority/array headers, and round-trip
custom event delivery.

To check if FreeSWITCH is listening: `ss -tlnp sport = :8022`

**Always run live tests before committing** when FreeSWITCH is available.
Check with `ss -tlnp sport = :8022` — if listening, run them. If not
available, skip but note it in the commit process.

## Examples — Write for the New User

Examples are the first thing a new user reads. Write them for someone who has
never used this library before.

- **Comment the "why", not the "what".** A beginner can read `client.api("status")`
  but can't guess that `body()` is `None` for some commands, or that `recv()`
  returning `None` means disconnection.
- **Show return types** when they aren't obvious from context. Add
  `// Option<&str>` or `// Option<CallDirection>` inline so the reader doesn't
  have to look up docs to follow the example.
- **No em-dashes (—) in source code.** Use commas, periods, or reword.
  Em-dashes are fine in markdown prose.
- **Explain unwrap() calls.** If `unwrap()` is safe, say why in a comment
  (e.g. "BACKGROUND_JOB always has a body; most other event types don't").
  If it's not safe, use `?` or handle the `None`.

### Keep examples in sync with API changes

When adding or changing public API (new traits, new enum variants, renamed
methods), **always update examples/ to use the new API**. Examples are the
primary documentation for new users. Stale examples that use deprecated or
removed patterns are worse than no examples at all. Build all examples
(`cargo build --examples`) as part of every change that touches public API.

### Typed API, not C ESL patterns

- **`HeaderLookup` trait** is the primary typed header API. Import `HeaderLookup`
  when writing generic code or implementing it on custom types (like
  `TrackedChannel` in `channel_tracker.rs`).
- **`header(EventHeader)`** only accepts typed enum variants — the compiler
  enforces this. For custom headers without a variant, use `header_str("X-Custom")`.
- **`variable(impl VariableName)`** accepts `ChannelVariable`, `SofiaVariable`,
  or any type implementing `VariableName`. For custom variables without an enum
  variant, use `variable_str("custom_var")`.
- Use typed accessors (`event.caller_id_number()`, `event.hangup_cause()`,
  `event.call_direction()`, `event.channel_state()`) — never raw
  `event.header_str("Caller-Caller-ID-Number")` for headers that have accessors.
- Use `EslEventType`'s `Display` impl — never hardcode event name strings
  like `"CREATE"` or `"HANGUP"` when you have the enum value.
- Don't store fields that are already in the data you're accumulating. If
  headers are merged into a map, derive typed state on access (`parse().ok()`)
  rather than maintaining parallel fields to keep in sync.
- The C ESL ecosystem is entirely string-based. LLMs default to that pattern.
  Review generated example code specifically for this anti-pattern.
- Never suggest raw `starts_with("+OK")` / `starts_with("-ERR")` string parsing
  on responses. Use the typed API: `response.is_success()`, `response.into_result()`,
  `response.reply_status()`. If a typed accessor doesn't exist for a use case,
  that's a missing feature to implement — not a reason to fall back to string matching.

## Development Methodology — TDD

This project follows test-driven development:

1. Write failing tests that reproduce the bug or specify the new behavior
2. Confirm tests fail (`cargo test --lib`)
3. `cargo fmt && git commit --no-verify` (red phase — clippy/tests will fail, but code must be formatted)
4. Implement the fix/feature
5. Confirm all tests pass
6. Commit the implementation (hooks run normally)

### Test failures reveal bugs, not inconveniences

When a test fails against real FreeSWITCH, **assume the library has a bug**
until proven otherwise. Never work around a test failure by removing the
triggering input (e.g. dropping a timeout value, switching to a simpler
endpoint). If the library produces a command that FreeSWITCH rejects, the
serialization is wrong — fix the serializer, not the test.
