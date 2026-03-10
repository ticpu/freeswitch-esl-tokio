# Design Rationale

Why this library exists and the architectural decisions behind it.

## Why a new ESL library

The existing Rust ESL crates each have fundamental limitations that make them
unsuitable for production telephony applications:

- **freeswitch-esl-rs** (6k downloads) — synchronous, single-threaded, blocking
  I/O. Cannot read events while sending commands. Not thread-safe.

- **freeswitch-esl** (3k downloads) — async/tokio but self-described WIP.
  JSON-only events, no liveness detection, no command timeouts, no structured
  command builders. Stale since September 2023.

- **eslrs** (300 downloads) — newest async contender, still in release candidate.
  Unified stream (not split reader/writer), silently discards unexpected
  responses, no liveness detection or timeouts.

None of them match the feature set of the C `libesl` library that ships with
FreeSWITCH, let alone the higher-level patterns from .NET's NEventSocket. We
needed a library that could handle production call control — concurrent commands
and events, connection health monitoring, structured command building, and
correct wire format handling.

## Split reader/writer architecture

Previous designs used a single handle that owned the TCP stream. Every method
took `&mut self`, making it impossible to send commands while receiving events.
The borrow checker enforced mutual exclusion: an event loop had to stop, send a
command, wait for the reply, then resume polling.

v1.0 splits the TCP stream and spawns a background reader task:

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

`EslClient` is `Clone` — pass it to multiple tasks. Commands are serialized
through the writer mutex (ESL is a sequential protocol). The reader task
determines event format from each message's `Content-Type` header rather than
storing state.

## Liveness detection

FreeSWITCH sends `HEARTBEAT` events every 20 seconds by default (configurable
via `event-heartbeat-interval` in `switch.conf`). The library does not implement
its own keepalive; instead it relies on the server's heartbeat as the
idle-traffic source — the same approach the C ESL library takes.

`set_liveness_timeout()` configures a threshold. Any inbound TCP traffic (not
just heartbeats) resets the timer. If the threshold is exceeded, the reader task
sets the connection status to `Disconnected(HeartbeatExpired)` and exits, which
closes the event channel.

The caller must subscribe to `HEARTBEAT` events for liveness detection to work
on idle connections. On busy connections, regular event traffic keeps the timer
alive.

## Disconnection and reconnection

The library detects disconnection but never reconnects automatically. The caller
sees disconnection through:

- `events.recv()` returning `None` (channel closed)
- `events.status()` / `client.is_connected()` returning the `DisconnectReason`
- `client.api()` returning `Err(NotConnected)` after disconnect

Reconnection is the caller's responsibility. This keeps the library predictable
— the caller controls backoff strategy, re-subscription, and state recovery.

## Correct wire format

The ESL `text/event-plain` format uses two-part framing: an outer envelope
(`Content-Length` + `Content-Type`) followed by a body containing URL-encoded
event headers. Header values are percent-decoded on parse. This matches the real
FreeSWITCH wire protocol as implemented in `mod_event_socket.c` and consumed by
the C ESL library in `esl.c`.

## Error classification

`EslError` variants carry `is_connection_error()` and `is_recoverable()` helpers
so callers can decide handling without matching every variant. Connection errors
(`Io`, `NotConnected`, `ConnectionClosed`, `HeartbeatExpired`) mean the TCP
session is dead. Recoverable errors (`Timeout`, `CommandFailed`,
`UnexpectedReply`, `QueueFull`) mean the connection is still usable.

## Protocol correctness vs NEventSocket

NEventSocket (.NET) is the most mature high-level ESL client. It works well in
practice but makes trade-offs that silently absorb protocol errors:

- **No Content-Type validation** — messages without Content-Type are accepted
  silently. A corrupted Content-Length that causes protocol desync produces
  garbage messages with no error signal. This library requires Content-Type on
  every message; its absence returns a protocol error so the caller can
  disconnect and reconnect from a known-good state.

- **No message or buffer size limits** — NEventSocket pre-allocates whatever
  Content-Length says with no upper bound. A malformed or malicious
  Content-Length can cause unbounded memory allocation. This library enforces
  8MB per message and 16MB total buffer.

- **Silent error recovery** — parse exceptions are caught with an empty handler
  and the stream continues. This masks protocol desync. This library propagates
  all parse errors to the caller via `EslResult`, with `is_connection_error()`
  and `is_recoverable()` helpers for classification.

These differences reflect a design choice: NEventSocket prioritizes resilience
(keep going), this library prioritizes correctness (stop and signal). For
telephony applications where a desynced connection produces wrong call control
decisions, explicit failure is safer than silent corruption.

## Typed state and header enums

FreeSWITCH is an entirely string-based system — channel state, call direction,
header names, and variable names are all plain strings on the wire. The C ESL
ecosystem and most client libraries preserve this. The problem: typos in header
names are silent, state comparisons are fragile string matches, and there's no
way to know at compile time whether `"Channal-State"` is a valid header.

v1.0 introduced typed state enums (`ChannelState`, `CallState`, `AnswerState`,
`CallDirection`) with `FromStr`/`Display`. v1.1 added `ChannelTimetable` for
call lifecycle timestamps. v1.2 added `EventHeader` and `ChannelVariable` enums
for compile-time header and variable name checking.

### Decoupling from EslEvent

`ChannelTimetable::from_lookup()` accepts a closure `|key| -> Option<&str>`
rather than requiring `&EslEvent`. This lets callers use the same extraction
logic with `HashMap`, `BTreeMap`, JSON objects, or any other key-value store.
`EslEvent` convenience methods (`caller_timetable()`, `other_leg_timetable()`)
are thin wrappers, not the primary API.

The same principle applies to the state enums: `ChannelState::from_str("CS_EXECUTE")`
works on any string, not just one pulled from an `EslEvent`. The `channel_tracker`
example demonstrates this — it stores headers in a flat `HashMap` and parses
typed state on demand without going through `EslEvent`.

### No silent failures on protocol data

Parsing channel state, timetable timestamps, and header names returns `Result`
or typed errors — never `.ok()` collapsing that hides parse failures.
`ParseTimetableError` includes the header name and unparseable value so callers
can diagnose protocol issues. This follows the crate's correctness-over-recovery
philosophy: if FreeSWITCH sends an unparseable timestamp, that's a signal, not
something to silently ignore.

### define_header_enum! macro

`EventHeader` and `ChannelVariable` are generated by a `define_header_enum!`
macro that produces `Display`, `FromStr` (case-insensitive), `as_str()`, and
`AsRef<str>` for each variant. Application-specific crates can use the same
macro to define their own header enums without depending on core types.

## Header key normalization

FreeSWITCH's C ESL library stores header names verbatim but looks them up
with `strcasecmp` and a case-insensitive hash (`esl_ci_hashfunc_default` in
`esl_event.c`). This means FreeSWITCH itself doesn't care about header
casing — but Rust's `HashMap<String, String>` does.

The problem is pervasive. Multiple C code paths emit the same logical header
with different casing:

- `switch_channel.c` (`switch_channel_event_set_basic_data`) emits
  `Unique-ID`, `Channel-State`, `Channel-Read-Codec-Bit-Rate` — Title-Case.
- `switch_event.c` emits `unique-id`, `channel-state`, `answer-state` —
  all lowercase.
- `switch_core_codec.c` is internally inconsistent: read codec headers are
  all lowercase (`channel-read-codec-bit-rate`), write codec headers are
  mixed (`Channel-Write-Codec-Name` but `Channel-Write-codec-bit-rate`),
  video codec headers are all lowercase.

Because `switch_event_add_header` doesn't deduplicate, a CODEC event can
contain *both* `Channel-Read-Codec-Bit-Rate` and `channel-read-codec-bit-rate`
as separate entries. The C library finds whichever comes first via its
linked-list scan with `strcasecmp`. A Rust `HashMap` stores both as distinct
keys, and `event.header(EventHeader::ChannelReadCodecBitRate)` silently picks
whichever one `HashMap::get` hashes to.

`normalize_header_key()` canonicalizes header keys at parse time so that all
casing variants collapse to a single `HashMap` entry:

1. **Known `EventHeader` match** — the key is parsed through
   `EventHeader::from_str()` (already case-insensitive). If it matches, the
   canonical `as_str()` form is returned. This preserves acronyms (`Unique-ID`,
   `DTMF-Digit`, `Channel-Call-UUID`) and special-case names (`priority`,
   `pl_data`) exactly as defined in the enum.

2. **Underscore passthrough** — keys containing underscores are returned
   unchanged. These are channel variables (`variable_sip_call_id`) or
   `sip_h_*` passthrough headers (`variable_sip_h_X-My-Custom-Header`) where
   the suffix preserves the original SIP header casing from the wire.
   FreeSWITCH emits all `variable_*` keys from a single code path
   (`switch_channel_event_set_extended_data`), so casing is already consistent.

3. **Title-Case fallback** — unknown dash-separated keys are Title-Cased
   (capitalize first letter of each segment, lowercase the rest). This matches
   FreeSWITCH's dominant convention for event and framing headers.

The underscore passthrough is critical for `sip_h_*` variables. `sofia.c`
and `sofia_glue.c` store raw SIP header names verbatim after the `sip_h_`
prefix — `sip_h_X-My-Custom-Header` preserves the exact casing from the
SIP peer. Lowercasing these would break outbound header passthrough, since
`sofia_glue_get_extra_headers()` strips the prefix and emits the remainder
as the SIP header name on the wire.

Normalization applies at every entry point: the wire parser, `set_header()`,
and serde deserialization all funnel through `normalize_header_key()`.
`EslEvent` maintains an `original_keys` alias map (`original → normalized`)
populated when the original key differs from its normalized form, so that
`header_str("unique-id")` resolves to the `"Unique-ID"` entry without
allocating on every lookup — one extra hash probe in the fallback path.
The alias map is derived state (`#[serde(skip)]`), rebuilt during
deserialization by routing all headers through `set_header()`.

## Command builders as pure Display types

Command builders in `commands/`, `app/`, and `variables/` implement `Display`
and `FromStr` with no dependency on `EslClient`. They produce strings,
`EslClient` calls `.to_string()`. This enables:

- Unit testing without a FreeSWITCH connection
- Round-trip testing (`parse` ↔ `to_string`)
- Reuse in contexts beyond this library (logging, debugging, CLI tools)

### Why serde on command builders

The serde derives on `Originate`, `Endpoint`, `Variables`, and
`BridgeDialString` exist because production callers need **config-driven
command construction**. A deployment's originate command — which gateway,
which SIP headers, which timeout — varies between environments and should
live in a YAML config file, not hardcoded in Rust.

The concrete driver was an NG911 abandoned-call callback daemon which
previously hardcoded deployment-specific SIP headers in a
`build_originate_command()` function. After adding serde to the command
builders, the entire originate command became a YAML block with `${placeholder}`
template substitution:

```yaml
originate:
  command:
    endpoint:
      sofia:
        profile: internal
        destination: "${contact}"
        variables:
          sip_h_X-Incident-Id: "${incident_id}"
    applications:
    - name: park
```

This is the pattern: **the library provides typed builders with serde, the
caller deserializes from config and calls `.to_string()` at originate time**.
No FreeSWITCH-specific knowledge is needed in the config layer.

## Why freeswitch-types is a separate crate

The domain types crate (`freeswitch-types`) has **zero async dependencies** —
no tokio, no futures. This split exists because the types are useful without
a network connection:

- CLI tools that parse and validate originate strings
- Config parsers that deserialize `Originate` from YAML
- Logging and debugging tools that format dial strings
- Other ESL transport implementations (sync, other runtimes)

Pulling in `freeswitch-esl-tokio` for types alone would force tokio as a
transitive dependency — unacceptable for a config parser or a static analysis
tool. The split keeps the dependency boundary clean: `freeswitch-types` is
pure data, `freeswitch-esl-tokio` is transport.

## Wire security: newline injection prevention

ESL is a text protocol where `\n\n` terminates a command. Any user-provided
string that reaches the wire without validation can inject arbitrary ESL
commands. For example, `api("status\n\nevent plain ALL")` would execute
`status` then silently subscribe to all events.

This was discovered during the pre-v1.0 security review. The fix:
`to_wire_format()` validates all user-supplied fields (command strings,
header names/values, passwords, app names/args) and returns
`EslError::ProtocolError` if `\n` or `\r` is present. The validation
happens at the wire boundary, not at construction time, because command
builders are `Display` types (infallible formatting) and the wire format
is the only place where newlines are dangerous.

The same principle applies to `CommandBuilder::header()` and `body()` —
they reject newlines in both names and values.

## Credential safety

ESL authentication sends passwords in cleartext over TCP. Two protections
prevent accidental exposure in logs:

1. **Manual `Debug` on `EslCommand`** — the derived `Debug` would print
   `Auth { password: "ClueCon" }` in any debug log. The manual impl redacts
   the password field.

2. **`redact_wire()` for wire logging** — debug-level wire logging uses
   `redact_wire()` which replaces the password in `auth` and `userauth`
   commands and strips the `\n\n` terminator for cleaner output.

These exist because production ESL daemons run with debug logging enabled
during incident investigation. A sysadmin grepping logs should not find
ESL passwords.

## Sequential command serialization

ESL is a strictly sequential protocol: one command in flight, one reply.
There are no request IDs, no multiplexing, no out-of-order replies. The
server processes commands in the order received and responds in the same
order.

The writer half is behind `Arc<Mutex>`, and the lock is held through the
entire send-and-wait-for-reply cycle — not just through the write. This was
a deliberate fix for a race condition found in the pre-v1.0 review: if the
lock was released after writing (before the reply arrived), two concurrent
`send_command()` calls could interleave, and the second caller's
`pending_reply` oneshot would overwrite the first's, causing misrouted
replies.

The simpler approach (hold the lock longer) was chosen over a queue-based
design because ESL doesn't support pipelining anyway — a command queue would
add complexity with no throughput benefit.

## Error classification: auth vs transient

`EslError` carries three classification helpers:

- `is_connection_error()` — TCP session is dead, must reconnect
- `is_recoverable()` — connection is still usable, retry the command
- `is_auth_error()` — permanent configuration error, do not retry

The `is_auth_error()` helper was added after observing production ESL daemons
(fs-eventd, noans-worker) spinning in infinite reconnect loops on auth failure
— retrying every 500ms with exponential backoff to 30s, forever. The fix was
a pattern: auth failure exits with code 78 (`EX_CONFIG`), and systemd's
`RestartPreventExitStatus=78` keeps it down. Transient failures (connection
lost, timeout) exit with code 1, and systemd restarts normally.

The library's job is to classify the error accurately. The caller's job is to
decide what to do with it. This is why the library never reconnects
automatically — it can't know whether a failure is permanent or transient in
the caller's context.

## Re-exec support: why it exists

Production ESL daemons like fs-eventd maintain a persistent TCP connection to
FreeSWITCH and track live channel state (active calls, channel variables,
timetables). A normal service restart loses the connection and all tracked
state. The state can be rebuilt from `show channels as json`, but events
during the reconnection gap are lost — missed hangups, missed creates, stale
channels in the tracking map.

The re-exec mechanism (`teardown_for_reexec()` + `adopt_stream()`) preserves
the TCP socket file descriptor across `exec()`, so the new binary image
inherits the already-authenticated, already-subscribed ESL connection. No
events are lost because the kernel TCP receive buffer holds data during the
brief exec window.

The drain protocol is the critical detail: the reader loop must stop at a
clean message boundary. ESL's two-part framing means that if the parser is
mid-body (headers consumed, waiting for body bytes), the residual would be
a partial body without headers — corrupt and unusable. The drain logic
continues reading until the parser returns to `WaitingForHeaders` state,
then returns the residual bytes for the new process to pre-seed its parser.

See [docs/reexec.md](docs/reexec.md) for the full API and drain protocol.

## HeaderLookup trait: why a trait, not methods on EslEvent

Production ESL daemons don't keep `EslEvent` objects around. fs-eventd's
channel tracker stores headers in a flat `HashMap<String, String>` and
accumulates them from multiple events over a channel's lifetime. The
`HeaderLookup` trait provides typed accessor methods (`channel_state()`,
`call_direction()`, `hangup_cause()`, etc.) that work on any type
implementing two methods: `header_str(&str) -> Option<&str>` and
`variable_str(&str) -> Option<&str>`.

This means the same accessors work on:

- `EslEvent` — direct event from the wire
- `EslResponse` — connect_session response with channel data
- `TrackedChannel` — accumulated state in a HashMap
- Any custom type the caller defines

The alternative — putting accessors only on `EslEvent` — would force callers
to either keep `EslEvent` objects alive or reimplement the accessors on their
own types. The trait makes the typed API composable.

## NEventSocket comparison: specific lessons

NEventSocket (.NET) is the most mature high-level ESL client and the
primary reference for what a "complete" ESL library looks like. The Rust
library deliberately diverges in three areas, each driven by a specific
failure mode observed or reviewed:

**Content-Type validation.** NEventSocket accepts messages without
Content-Type silently. A corrupted Content-Length that causes protocol
desync produces garbage messages with no error signal — the stream
continues with corrupt data. In telephony, a desynced connection produces
wrong call control decisions (hanging up the wrong call, bridging to the
wrong destination). This library requires Content-Type on every message;
its absence is a protocol error.

**Buffer size limits.** NEventSocket pre-allocates whatever Content-Length
claims with no upper bound. A malformed or malicious Content-Length can
cause unbounded memory allocation. This library enforces 8MB per message
and 16MB total buffer. These limits are generous for any legitimate ESL
traffic (the largest normal messages are `show channels` responses with
thousands of active calls).

**Parse error propagation.** NEventSocket catches parse exceptions with an
empty handler and continues. This masks protocol desync — the stream
produces garbage silently. This library propagates all parse errors to the
caller via `EslResult`. The caller can classify them (`is_connection_error()`
vs `is_recoverable()`) and decide whether to disconnect and reconnect from
a known-good state.

## RFC 4575 conference-info XML namespace handling

RFC 4575 documents use the XML namespace `urn:ietf:params:xml:ns:conference-info`,
but producers choose their own prefix: Bell's BCF uses `confInfo:`, others use
`ci:`, and some declare it as the default namespace (no prefix). The element
names are identical in all cases — only the prefix varies.

quick-xml's serde deserializer matches element names literally, including any
prefix. A field annotated `#[serde(rename = "users")]` matches `<users>` but
not `<confInfo:users>`. The serde layer has no namespace awareness.

quick-xml does provide `NsReader` for namespace-aware event-based parsing, but
it cannot be combined with serde. Using it would mean writing a manual
event-driven parser for every RFC 4575 type — hundreds of lines of brittle code
that discards the entire value of serde derivation.

The chosen approach: **pre-process the XML to strip namespace prefixes** before
feeding it to `quick_xml::de::from_str()`. An internal `normalize` function
uses quick-xml's `Reader`/`Writer` event loop to rewrite element names
(`confInfo:users` → `users`), remove `xmlns` declarations, and preserve all
other attributes. This is correct because RFC 4575 uses a single namespace —
there are no competing prefixes to disambiguate.

The normalizer is an internal implementation detail behind `ConferenceInfo::from_xml()`.
Callers never see it. Serialization with `to_xml()` emits prefix-free XML,
which is valid RFC 4575 (using the default namespace).

## BgJobTracker: bgapi correlation as a data structure

Every production ESL daemon we've built — fs-eventd's channel tracker,
noans-worker's originate monitor, the bgapi benchmark — contained the same
boilerplate: a `HashMap<String, Context>` mapping Job-UUID to application
state, a check for `BackgroundJob` type + Job-UUID match on every event,
removal from the map, and `parse_api_body()` on the body. The pattern was
identical each time, differing only in what context was attached (channel
UUID, send timestamp, call ID).

`BgJobTracker<C>` extracts this into a generic `HashMap` wrapper. The type
parameter `C` is caller-defined context attached at send time and returned
by `try_complete()` when the matching event arrives. The caller's match arm
in the event loop is the handler — the same code that would have lived in
the manual `BackgroundJob` branch, but without the UUID bookkeeping.

A callback-based dispatcher was considered but rejected: `Box<dyn FnOnce>`
handlers run inside `dispatch()` which holds `&mut self`, preventing the
handler from accessing the surrounding `&mut app_state` without
`Arc<Mutex<>>`. A future-based design (oneshot-backed handles) creates two
consumption paths for one result — the caller must both drive `dispatch()`
and `.await` handles elsewhere, hanging silently if they forget one side.
The context-return approach sidesteps both problems because the caller
already has `&mut app_state` at the `try_complete` call site.

`BgJobResult<'a>` borrows from the event rather than cloning, matching the
library's general pattern where `event.body()` and `event.job_uuid()`
return `Option<&str>`. The result is always consumed in the same event loop
iteration. The tracker lives in `freeswitch-esl-tokio` rather than
`freeswitch-types` because its `bgapi()` convenience method calls
`EslClient::bgapi()`.
