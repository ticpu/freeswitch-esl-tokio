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

## Command builders as pure Display types

Command builders in `commands/`, `app/`, and `variables/` implement `Display`
and `FromStr` with no dependency on `EslClient`. They produce strings,
`EslClient` calls `.to_string()`. This enables:

- Unit testing without a FreeSWITCH connection
- Round-trip testing (`parse` ↔ `to_string`)
- Reuse in contexts beyond this library (logging, debugging, CLI tools)

## Truncated userauth response (mod_event_socket reply[512] overflow)

FreeSWITCH's `mod_event_socket.c` has a buffer overflow in the `userauth`
response path that silently truncates the reply and drops the `\n\n`
terminator our parser expects.

The code in `listener_run()` declares `char reply[512]` (line 2704) as the
final output buffer. The `userauth` handler at line 2009 formats the auth
response into it:

```c
switch_snprintf(reply, reply_len,
    "~Reply-Text: +OK accepted\n%s%s%s\n",
    event_reply, api_reply, log_reply);
```

Each of `event_reply`, `api_reply`, and `log_reply` is itself a 512-byte
buffer, but they all get concatenated into the same 512-byte `reply`.
With a realistic `esl-allowed-events` list (28 event types plus CUSTOM
subclasses), the `Allowed-Events` header alone consumes ~470 bytes of
the reply buffer. After the `~Reply-Text: +OK accepted\n` prefix (~28
bytes), only ~14 bytes remain for `Allowed-API` and `Allowed-LOG`.
`switch_snprintf` silently truncates without writing the trailing `\n`
that would complete the `\n\n` terminator.

The wire data reaches the client as a valid-looking header block that
simply never ends. The parser accumulates the data, never finds `\n\n`,
and eventually the connect timeout fires.

This only affects `userauth`, not `auth`. The plain `auth` command
response is a simple `+OK accepted` (14 bytes) that fits easily.

Our workaround lives in `authenticate()` in `connection.rs`. When the
auth response read times out and the parser buffer contains data, we
call `salvage_truncated_auth_response()` which:

1. Extracts the buffer contents as UTF-8
2. Finds the last complete line (ending with `\n`)
3. Drops any trailing fragment without a colon (mid-header-name truncation)
4. Parses the salvageable headers via the existing `parse_headers()` path
5. Validates `Content-Type: command/reply` to confirm it's actually an
   auth response
6. Returns the partial `EslMessage` and drains the parser buffer

The function is private to `connection.rs` and only called from
`authenticate()`. It is not a general parser recovery mechanism -- the
`Content-Type` validation ensures it won't accidentally salvage
non-auth data.

The auth is valid on the FreeSWITCH side -- `LFLAG_AUTHED` is set before
the reply is sent. The truncated headers (`Allowed-API`, `Allowed-LOG`)
are informational access-policy metadata, not required for the ESL
session to function. We log a WARN so operators can identify the issue
and either reduce their `esl-allowed-events` list or patch FreeSWITCH.
