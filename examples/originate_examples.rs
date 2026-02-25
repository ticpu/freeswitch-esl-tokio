//! Originate builder reference -- all endpoint types and targeting modes.
//!
//! Part 1 (no FreeSWITCH required): builds each endpoint type and prints the
//! resulting wire string. Covers variable scoping ({} default, [] channel,
//! <> enterprise), all DialplanType variants, all OriginateTarget forms,
//! and JSON deserialization.
//!
//! Part 2: connects to FreeSWITCH, places a test call via bgapi, and reports
//! the BACKGROUND_JOB result and channel lifecycle events.
//!
//! Usage: RUST_LOG=info cargo run --example originate_examples [-- [host[:port]] [password]]
//!   Defaults: localhost:8021, ClueCon

use freeswitch_esl_tokio::commands::{
    AudioEndpoint, ErrorEndpoint, GroupCall, LoopbackEndpoint, SofiaContact, SofiaEndpoint,
    SofiaGateway, UserEndpoint,
};
use freeswitch_esl_tokio::{
    Application, DialplanType, Endpoint, EslClient, EslError, EslEventType, EventFormat,
    EventHeader, HeaderLookup, Originate, OriginateTarget, Variables, VariablesType,
    DEFAULT_ESL_PORT,
};
use tracing::{error, info};

fn print_endpoint_examples() {
    println!("=== Endpoint wire formats ===");

    // -----------------------------------------------------------------------
    // Direct SIP profile routing
    // -----------------------------------------------------------------------

    println!("\n-- SofiaEndpoint: sofia/profile/destination --");

    let cmd = Originate {
        endpoint: Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000@10.0.0.1".into(),
            variables: None,
        }),
        target: OriginateTarget::Extension("1000".into()),
        dialplan: None,
        context: None,
        cid_name: Some("Alice".into()),
        cid_num: Some("5551234".into()),
        timeout: Some(30),
    };
    // originate sofia/internal/1000@10.0.0.1 1000 XML default Alice 5551234 30
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // SIP gateway routing
    // -----------------------------------------------------------------------

    println!("\n-- SofiaGateway: sofia/gateway/name/destination --");

    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "my_provider".into(),
            destination: "18005551234".into(),
            // None: gateway name only; Some("external") qualifies as profile::gateway
            profile: None,
            variables: None,
        }),
        // Application::simple(name).into() converts Application -> OriginateTarget::Application
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: Some(60),
    };
    // originate sofia/gateway/my_provider/18005551234 &park() XML undef undef 60
    println!("{}", cmd);

    // With a profile qualifier: sofia/gateway/external::my_provider/destination
    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "my_provider".into(),
            destination: "18005551234".into(),
            profile: Some("external".into()),
            variables: None,
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate sofia/gateway/external::my_provider/18005551234 &park()
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // User endpoint -- FreeSWITCH resolves the contact via the directory
    // -----------------------------------------------------------------------

    println!("\n-- UserEndpoint: user/name@domain --");

    let cmd = Originate {
        endpoint: Endpoint::User(UserEndpoint {
            name: "1000".into(),
            domain: Some("pbx.example.com".into()),
            variables: None,
        }),
        target: OriginateTarget::Extension("5000".into()),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate user/1000@pbx.example.com 5000
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // sofia_contact -- FreeSWITCH runtime expression, resolved at call time
    // -----------------------------------------------------------------------

    println!("\n-- SofiaContact: ${{sofia_contact([profile/]user@domain)}} --");

    let cmd = Originate {
        endpoint: Endpoint::SofiaContact(SofiaContact {
            user: "bob".into(),
            domain: "pbx.example.com".into(),
            // "*" searches all profiles; use a profile name to limit the lookup
            profile: Some("*".into()),
            variables: None,
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: Some(20),
    };
    // originate ${sofia_contact(*/bob@pbx.example.com)} &park() XML undef undef 20
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // group_call -- FreeSWITCH runtime expression, resolves directory group
    // -----------------------------------------------------------------------

    println!("\n-- GroupCall: ${{group_call(group@domain[+order])}} --");

    let cmd = Originate {
        endpoint: Endpoint::GroupCall(GroupCall {
            group: "support".into(),
            domain: "pbx.example.com".into(),
            // A=all members simultaneously, F=first registered, E=enterprise
            order: Some("A".into()),
            variables: None,
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate ${group_call(support@pbx.example.com+A)} &park()
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // Loopback -- routes through the dialplan; useful for testing
    // -----------------------------------------------------------------------

    println!("\n-- LoopbackEndpoint: loopback/extension/context --");

    let cmd = Originate {
        endpoint: Endpoint::Loopback(LoopbackEndpoint {
            // 9196 = delay_echo in the default FreeSWITCH configuration
            extension: "9196".into(),
            context: "default".into(),
            variables: None,
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate loopback/9196/default &park()
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // error/ -- immediately fails with a specified hangup cause
    // -----------------------------------------------------------------------

    println!("\n-- ErrorEndpoint: error/cause --");

    let cmd = Originate {
        endpoint: Endpoint::Error(ErrorEndpoint {
            cause: "USER_BUSY".into(),
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate error/USER_BUSY &park()
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // Audio device endpoints (portaudio / pulseaudio / alsa)
    // -----------------------------------------------------------------------

    println!("\n-- AudioEndpoint: portaudio[/destination] --");

    let cmd = Originate {
        endpoint: Endpoint::PortAudio(AudioEndpoint {
            destination: Some("auto_answer".into()),
            variables: None,
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate portaudio/auto_answer &park()
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // Variable scoping
    // -----------------------------------------------------------------------

    println!("\n=== Variable scoping ===");

    // Default scope {}: applies to all legs of this originate
    println!("\n-- Default scope {{}} --");

    let mut vars = Variables::new(VariablesType::Default);
    vars.insert("hangup_after_bridge", "true");
    vars.insert("continue_on_fail", "true");
    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "carrier".into(),
            destination: "15551234567".into(),
            profile: None,
            variables: Some(vars),
        }),
        target: Application::simple("bridge").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate {hangup_after_bridge=true,continue_on_fail=true}sofia/gateway/carrier/15551234567 &bridge()
    println!("{}", cmd);

    // Channel scope []: applies only to the immediately following endpoint
    println!("\n-- Channel scope [] --");

    let mut vars = Variables::new(VariablesType::Channel);
    vars.insert("originate_timeout", "20");
    vars.insert("sip_h_X-Tenant", "acme");
    let cmd = Originate {
        endpoint: Endpoint::Sofia(SofiaEndpoint {
            profile: "external".into(),
            destination: "sip:alice@carrier.example.com".into(),
            variables: Some(vars),
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate [originate_timeout=20,sip_h_X-Tenant=acme]sofia/external/sip:alice@carrier.example.com &park()
    println!("{}", cmd);

    // Values containing commas are auto-escaped as \, -- required by the FS variable parser
    println!("\n-- Comma-containing values (auto-escaped) --");

    let mut vars = Variables::new(VariablesType::Default);
    vars.insert("absolute_codec_string", "PCMU,PCMA,G722");
    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "gw1".into(),
            destination: "1234".into(),
            profile: None,
            variables: Some(vars),
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate {absolute_codec_string=PCMU\,PCMA\,G722}sofia/gateway/gw1/1234 &park()
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // OriginateTarget variants
    // -----------------------------------------------------------------------

    println!("\n=== OriginateTarget variants ===");

    // Extension: routed through the XML dialplan engine
    println!("\n-- Extension (routes through XML dialplan) --");

    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "gw1".into(),
            destination: "18005551234".into(),
            profile: None,
            variables: None,
        }),
        target: OriginateTarget::Extension("1000".into()),
        dialplan: Some(DialplanType::Xml),
        context: Some("default".into()),
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate sofia/gateway/gw1/18005551234 1000 XML default
    println!("{}", cmd);

    // Application: single &app(args) XML form
    println!("\n-- Application (&app(args)) with spaces auto-quoted --");

    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "gw1".into(),
            destination: "18005551234".into(),
            profile: None,
            variables: None,
        }),
        // Args containing spaces are automatically single-quoted on the wire.
        // FreeSWITCH's originate parser requires this.
        target: Application::new("socket", Some("127.0.0.1:8040 async full")).into(),
        dialplan: None,
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate sofia/gateway/gw1/18005551234 '&socket(127.0.0.1:8040 async full)'
    println!("{}", cmd);

    // Inline applications: comma-separated app:args list
    println!("\n-- InlineApplications (app:args,app:args) --");

    let cmd = Originate {
        endpoint: Endpoint::SofiaGateway(SofiaGateway {
            gateway: "gw1".into(),
            destination: "18005551234".into(),
            profile: None,
            variables: None,
        }),
        // Vec<Application>::into() produces OriginateTarget::InlineApplications
        target: vec![
            Application::new("conference", Some("test_room")),
            Application::simple("hangup"),
        ]
        .into(),
        // DialplanType::Inline is emitted as "inline" on the wire
        dialplan: Some(DialplanType::Inline),
        context: None,
        cid_name: None,
        cid_num: None,
        timeout: None,
    };
    // originate sofia/gateway/gw1/18005551234 conference:test_room,hangup inline
    println!("{}", cmd);

    // -----------------------------------------------------------------------
    // JSON deserialization -- config-driven originate
    // -----------------------------------------------------------------------

    println!("\n=== JSON deserialization ===");

    // Originate commands can live entirely in config files and be deserialized
    // at runtime. The endpoint uses snake_case variant names.
    // A flat variable map defaults to VariablesType::Default ({} scope).
    let json = r#"{
        "endpoint": {
            "sofia_gateway": {
                "gateway": "my_provider",
                "destination": "18005551234",
                "variables": {"originate_timeout": "60", "sip_h_X-Custom": "value"}
            }
        },
        "application": {"name": "park"},
        "timeout": 30
    }"#;
    match serde_json::from_str::<Originate>(json) {
        Ok(cmd) => println!("from JSON: {}", cmd),
        Err(e) => println!("JSON parse error: {}", e),
    }

    // Wire format round-trip
    let wire = "originate sofia/gateway/carrier/15551234567 &bridge(user/1000) XML default Alice 5551234 60";
    match wire.parse::<Originate>() {
        Ok(cmd) => {
            // Parsed struct re-serializes to the identical wire string
            assert_eq!(cmd.to_string(), wire);
            println!("round-trip: {}", cmd);
        }
        Err(e) => println!("parse error: {}", e),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    print_endpoint_examples();

    // -----------------------------------------------------------------------
    // Part 2: live call via bgapi
    // -----------------------------------------------------------------------

    let args: Vec<String> = std::env::args().collect();
    let (host, port) = match args
        .get(1)
        .map(|s| s.as_str())
    {
        Some(h) if h.contains(':') => {
            let (h, p) = h
                .split_once(':')
                .unwrap(); // safe: contains(':') checked above
            (
                h.to_string(),
                p.parse::<u16>()
                    .expect("invalid port"),
            )
        }
        Some(h) => (h.to_string(), DEFAULT_ESL_PORT),
        None => ("localhost".to_string(), DEFAULT_ESL_PORT),
    };
    let password = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("ClueCon");

    let (client, mut events) = match EslClient::connect(&host, port, password).await {
        Ok(pair) => {
            info!("connected to {}:{}", host, port);
            pair
        }
        Err(EslError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            error!(
                "connection refused -- is FreeSWITCH running on {}:{}?",
                host, port
            );
            return Err(e.into());
        }
        Err(e) => return Err(e.into()),
    };

    client
        .subscribe_events(
            EventFormat::Plain,
            &[
                EslEventType::BackgroundJob,
                EslEventType::ChannelCreate,
                EslEventType::ChannelAnswer,
                EslEventType::ChannelHangup,
                EslEventType::ChannelDestroy,
            ],
        )
        .await?;

    // loopback/9196/default routes to the built-in delay_echo test in the
    // default FreeSWITCH configuration -- no registered phones required.
    let cmd = Originate {
        endpoint: Endpoint::Loopback(LoopbackEndpoint {
            extension: "9196".into(),
            context: "default".into(),
            variables: None,
        }),
        target: Application::simple("park").into(),
        dialplan: None,
        context: None,
        cid_name: Some("ESL Test".into()),
        cid_num: Some("0000000000".into()),
        timeout: Some(10),
    };

    println!("\n=== Live call via bgapi ===");
    println!("originate: {}", cmd);

    // bgapi returns immediately with a Job-UUID; the originate result arrives
    // later as a BACKGROUND_JOB event matching that UUID.
    let response = client
        .bgapi(&cmd.to_string())
        .await?;
    let job_uuid = response
        .job_uuid()
        // bgapi always returns a Job-UUID in the response headers
        .expect("bgapi always returns Job-UUID")
        .to_string();
    info!("job submitted: {}", job_uuid);

    let mut call_uuid: Option<String> = None;

    while let Some(Ok(event)) = events
        .recv()
        .await
    {
        match event.event_type() {
            Some(EslEventType::BackgroundJob) => {
                if event.job_uuid() != Some(job_uuid.as_str()) {
                    continue; // unrelated bgapi job
                }
                // The body of a BACKGROUND_JOB event is the raw API response text,
                // not an EslResponse -- raw "+OK"/ "-ERR" matching is appropriate here.
                let result = event
                    .body()
                    .unwrap(); // BACKGROUND_JOB always has a body
                if let Some(uuid) = result.strip_prefix("+OK ") {
                    call_uuid = Some(
                        uuid.trim()
                            .to_string(),
                    );
                    info!("call created: {}", uuid.trim());
                } else {
                    // -ERR <cause> -- originate failed before any channel was created
                    error!("originate failed: {}", result.trim());
                    break;
                }
            }
            Some(EslEventType::ChannelCreate) => {
                let uuid = event.header(EventHeader::UniqueId);
                info!("channel created: {}", uuid.unwrap_or("?"));
            }
            Some(EslEventType::ChannelAnswer) => {
                let uuid = event.header(EventHeader::UniqueId);
                info!("channel answered: {}", uuid.unwrap_or("?"));
            }
            Some(EslEventType::ChannelHangup) => {
                let uuid = event.header(EventHeader::UniqueId);
                let cause = event
                    .hangup_cause()
                    .unwrap_or("unknown");
                info!("channel hangup: {} cause={}", uuid.unwrap_or("?"), cause);
            }
            Some(EslEventType::ChannelDestroy) => {
                let uuid = event.header(EventHeader::UniqueId);
                info!("channel destroyed: {}", uuid.unwrap_or("?"));
                // Stop once our specific channel is gone
                if call_uuid.is_some() && uuid == call_uuid.as_deref() {
                    break;
                }
            }
            _ => {}
        }
    }

    client
        .disconnect()
        .await?;
    Ok(())
}
