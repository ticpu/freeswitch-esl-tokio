# Driving a live call from a daemon

Four ways a Rust daemon can put a prompt in front of a caller and act on what
they press, plus the switch-side machinery all four share. They are not ranked:
each answers a different question about where the logic runs and what happens
when the process running it dies.

Everything below was measured against a live FreeSWITCH, including one call to
a real handset. Where a claim comes from reading C rather than from the wire,
it says so.

## The four shapes

### Inline dialplan, daemon watches and transfers

The originate carries the whole IVR as an application list — answer, prompt,
park — and the daemon does nothing until `CHANNEL_EXECUTE_COMPLETE` tells it
what was collected, at which point it transfers the parked leg.

No new ESL surface: the daemon needs only the inbound connection and the event
subscription it already has. `park` holds the leg, so both switch-side
deadlines are available (see [Deadlines](#deadlines)). Nothing the daemon does
is in the call's path, so a daemon that dies leaves a parked leg that its own
deadline still resolves.

The cost is that the logic is a flat list decided before the call exists. It
cannot branch (see [Conditions are resolved at hunt
time](#conditions-are-resolved-at-hunt-time)), so anything conditional becomes
a `transfer` to a computed destination and a second dialplan hunt.

### Inbound `sendmsg` with a uuid

The daemon parks the leg and drives it with `sendmsg` addressed to the uuid,
over the connection it already holds.

The catch is acknowledgement. `parse_command` locates the session and calls
`switch_core_session_queue_private_event`; `+OK` means the event was queued and
nothing more. The only failures are `-ERR invalid session id` and `-ERR memory
error`, and past that point there is no negative acknowledgement at all. A
queued private event runs when the session next reaches a
`switch_ivr_parse_all_events` call site — the park loop, playback, bridge, or
the state machine between applications — and is discarded silently if the
channel hangs up first.

To know an application ran, put an `Event-UUID` header on the `sendmsg`.
`switch_ivr_parse_event` copies it into the `app_uuid` channel variable and
`switch_core_session_execute_application` emits it as `Application-UUID` on
both `CHANNEL_EXECUTE` and `CHANNEL_EXECUTE_COMPLETE`. Correlate on that, never
on the application name — a channel can execute `play_and_get_digits` for
reasons that are not yours. The absence of `CHANNEL_EXECUTE` within your own
timeout is the negative acknowledgement you have to build.

### Outbound socket, the daemon is the application

The dialplan (or an originate's `&socket()`) points at the daemon, which
becomes the session's application for the call's duration.

Session-scoped delivery comes free, but only via `myevents` — see
[outbound-esl-quirks.md](outbound-esl-quirks.md), where the mode's other traps
live. The real cost is that a long-lived supervisor is now in the call's path:
a restart during a call drops it, and a socket that cannot be reached leaves
the dialplan to fall through to whatever was written after it.

### Outbound socket, a spawned child per call

The daemon spawns a short-lived child that binds an ephemeral port; the
originate points `&socket()` at that port. The child is the session's
application for one prompt and then transfers.

This keeps the supervisor out of the call path while keeping the acknowledgement
properties of a socket: in static mode — neither `async` nor `full` — `+OK`
arrives only once the application returns, so no correlation is needed at all,
and `getvar` reads the result back without `full`. A parent that dies leaves an
unsupervised child that can still finish the call.

What it costs is a second process, a per-call connection, and no `park`, so the
deadline must come from outside. Do not `fork()` a tokio process: only the
calling thread survives, and the runtime, its epoll registrations and any
locked allocator state do not. Spawn a separate binary.

[outbound_ivr_supervised.rs](../examples/outbound_ivr_supervised.rs) implements
this one end to end, including detecting parent loss by reading a pipe to EOF.

## What the survivor is, after a bowout

If the call reaches its destination through a loopback pair that bows out, the
channel the IVR runs on is **not** the one `originate` returned. The loopback
leg masquerades its remaining applications onto the real channel and hangs up,
and the real channel has a different uuid.

The chain that resolves it needs no filter and no extra API call:

- `bgapi`'s `BACKGROUND_JOB` body is `+OK <uuid>`, and that uuid is the
  loopback leg.
- The CUSTOM event `loopback::bowout` carries that same value as
  `Resigning-UUID`, alongside `Acquired-UUID` for the survivor. It is fired
  before the masquerade runs, so it is the earliest reliable point.

A consumer that tracks the originate's uuid instead receives exactly one useful
event from it: its own `CHANNEL_HANGUP_COMPLETE`, carrying a resignation marker
for a call that is still up.

The masquerade also copies every channel variable onto the survivor, so on that
channel `variable_uuid` names the leg that left and `variable_read_codec`
reports what the loopback spoke rather than what the channel negotiated. Only
the event headers describe the channel emitting them. Read identity from
`Unique-ID` and codecs from `Channel-Read-Codec-Name`.

[loopback-bowout.md](loopback-bowout.md) covers the rest, including which
resignation path you are on and why no variable test identifies a loopback
channel.

## Conditions are resolved at hunt time

FreeSWITCH hunts the dialplan in CS_ROUTING and produces a flat list of
applications with every `<condition>` already evaluated. A condition placed
after the application that sets the variable it tests is therefore evaluated
before that application has run, against an empty value.

Application *data* is different: it is expanded when the application executes.
So a computed destination works where a condition does not, and a second hunt
at the transfer target sees everything the first pass set:

```xml
<action application="transfer" data="noans_${noans_dtmf}"/>
```

This is also why an inline application list survives a bowout intact — the list
was already flattened before the masquerade moved it.

## Deadlines

Four mechanisms, and only the first two survive the daemon's death.

`park_timeout`, set before the leg parks in the `<seconds>[:<cause>]` form,
**hangs the channel up**. `switch_ivr_park` reads it at entry, computes an
absolute expiry, clears the variable, and calls `switch_channel_hangup` when it
fires. It does not return so that a following application can run, so
`park,transfer` never reaches the transfer. Re-arm it before every re-park, and
remember that anything executed from inside the park loop spends the same
clock.

`sched_transfer +N <uuid> <ext> XML <context>` breaks park by transferring
rather than by hanging up, which is what "connect them anyway" actually needs.
Use it for the soft deadline and keep `park_timeout` outside it as a hard floor
with a distinguishable cause.

`sched_hangup` is the equivalent for a leg that is not parked — a static-mode
outbound socket, for instance, which never parks. Cancelling by uuid takes the
whole task group, so two independently cancellable tasks cannot coexist on one
leg.

The socket's own lifetime bounds nothing useful: a dead child closes the socket,
but a wedged one does not.

## What the events tell you

`play_and_get_digits` reports four distinguishable outcomes through variables
on its `CHANNEL_EXECUTE_COMPLETE`:

| what happened | `read_result` | named variable | `read_terminator_used` |
|---|---|---|---|
| digit collected | `success` | the digit | the terminator |
| only a terminator pressed | `failure` | absent | the terminator |
| nothing pressed | `failure` | absent | absent |
| prompt file unopenable | absent | absent | absent |

The last row is worth alerting on rather than treating as a timeout: a broken
prompt sounds exactly like a working one nobody answered, and only the missing
`read_result` distinguishes them. A missing prompt file does not otherwise harm
the leg — execution continues to the next application.

Terminators are not validated against anything DTMF-producible, so a terminator
set of `z` means nothing can ever terminate early. An empty terminators field
does not mean that: `play_and_get_digits` substitutes `#` when the field is
empty, so omission is not how you spell "no terminator".

The `DTMF` event arrives before the application returns, and it arrives in the
terminator-only case where the application yields no variable at all. A
consumer that acts on whichever of `DTMF` and `CHANNEL_EXECUTE_COMPLETE` comes
first therefore reacts sooner and covers a case the application drops — at the
price of accepting keys the collector's regex would have rejected, and of
having to ignore every press after the first.

## Media is decided before you know the destination

A leg with no IVR on it can bridge straight through, and the two ends negotiate
with each other. Put a prompt on that leg and FreeSWITCH must have a media path
it can play audio into, which pins the codec at prompt time — before the
destination is known.

Get it wrong and the call fails at setup with `488 Not Acceptable Here`, or,
worse, succeeds and then drops at the transfer because the target speaks
something else. So `absolute_codec_string` becomes an input the daemon must
know *before* it originates, and a prompt costs transcoding for its duration.

This is the one cost that applies to every shape above equally, and the one an
IVR running on the far switch does not pay.
