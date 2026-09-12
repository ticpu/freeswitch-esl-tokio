# Command builders

Typed builders for FreeSWITCH API commands. All implement `Display`, are independent of `EslClient`, and can be unit tested without a connection.

See [command-builders.md](../command-builders.md) for the full builder architecture, all channel/conference command types, and escaping rules.

## Endpoint types

Each endpoint type is a concrete struct implementing the `DialString` trait. The `Endpoint` enum wraps them for polymorphic storage and serde.

- **SofiaEndpoint** -- `sofia/{profile}/{destination}`, direct SIP profile routing
- **SofiaGateway** -- `sofia/gateway/{gateway}/{destination}`, SIP gateway routing
- **LoopbackEndpoint** -- `loopback/{extension}/{context}`, internal loopback
- **UserEndpoint** -- `user/{name}@{domain}`, directory-based dial-string lookup
- **SofiaContact** -- `${sofia_contact(user@domain)}`, registered contacts resolved by the switch at runtime
- **GroupCall** -- `${group_call(group@domain+A)}`, group members resolved by the switch at runtime
- **ErrorEndpoint** -- `error/{cause}`, bridge to a hangup cause

```rust
use freeswitch_esl_tokio::commands::*;

// Direct SIP profile routing
let ep = Endpoint::Sofia(SofiaEndpoint::new("internal", "1000@example.com"));
assert_eq!(ep.to_string(), "sofia/internal/1000@example.com");

// SIP gateway routing
let ep = Endpoint::SofiaGateway(SofiaGateway::new("my_provider", "18005551234"));
assert_eq!(ep.to_string(), "sofia/gateway/my_provider/18005551234");

// Parse from wire format
let ep: Endpoint = "sofia/gateway/my_provider/18005551234".parse().unwrap();

// Downstream crates can implement DialString on custom endpoint types
```

## Originate

```rust,no_run
# use freeswitch_esl_tokio::{EslClient, EslError};
use freeswitch_esl_tokio::commands::*;
# #[tokio::main]
# async fn main() -> Result<(), EslError> {
# let (client, _events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

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
# let _ = ext_cmd;
# Ok(())
# }
```

## Bridge dial strings

`BridgeDialString` builds multi-endpoint bridge arguments with simultaneous ring (`,`) and sequential failover (`|`):

```rust,no_run
# use freeswitch_esl_tokio::{EslClient, EslError, AppCommand};
use freeswitch_esl_tokio::commands::*;
# #[tokio::main]
# async fn main() -> Result<(), EslError> {
# let (client, _events) = EslClient::connect("localhost", 8021, "ClueCon").await?;

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
# Ok(())
# }
```

See [dial-string-format.md](../dial-string-format.md) for the complete dial string reference (variable scoping, `^^:` custom delimiters, enterprise `:_:` originate).

## A value that must not cross the dial string

A large or free-text value — a PIDF-LO for `sip_multipart`, say — is set on the new channel by an `execute_on_originate` hook instead, which runs before the channel's session thread starts and so before a SIP leg builds its INVITE. `ExecuteOn` builds the hook's value and refuses the shapes the switch would misread; the block then carries paths and nothing the tokenizer can damage:

```rust
use freeswitch_esl_tokio::commands::*;
use freeswitch_esl_tokio::ChannelVariable;

let hook = ExecuteOn::lua("/run/app/load_multipart.lua", ["/run/app/call-42.xml"]).unwrap();
let mut vars = Variables::new(VariablesType::Default);
vars.insert(ChannelVariable::ExecuteOnOriginate.as_str(), hook.to_string());
assert_eq!(
    vars.to_string(),
    "{execute_on_originate='lua /run/app/load_multipart.lua /run/app/call-42.xml'}"
);
```

`cargo run --example originate_multipart_file` drives it end to end; the "Keeping a value out of the tokenizer entirely" section of the dial string reference has the measurements and the traps.

## UUID and conference commands

```rust,no_run
# use freeswitch_esl_tokio::{EslClient, EslError};
use freeswitch_esl_tokio::commands::*;
use freeswitch_esl_tokio::HangupCause;
# #[tokio::main]
# async fn main() -> Result<(), EslError> {
# let (client, _events) = EslClient::connect("localhost", 8021, "ClueCon").await?;
# let uuid = "11111111-1111-1111-1111-111111111111";

// UUID commands
let kill = UuidKill::with_cause(uuid, HangupCause::NormalClearing);
// -> "uuid_kill <uuid> NORMAL_CLEARING"
client.api(&kill.to_string()).await?;

// Conference commands
let dtmf = ConferenceDtmf::new("room1", "all", "1");
// -> "conference room1 dtmf all 1"
client.api(&dtmf.to_string()).await?;
# Ok(())
# }
```

Output strings are verified by unit tests in [originate.rs](../../freeswitch-types/src/commands/originate.rs), [endpoint/](../../freeswitch-types/src/commands/endpoint/), [bridge.rs](../../freeswitch-types/src/commands/bridge.rs), [channel.rs](../../freeswitch-types/src/commands/channel.rs), and [conference.rs](../../freeswitch-types/src/commands/conference.rs).
