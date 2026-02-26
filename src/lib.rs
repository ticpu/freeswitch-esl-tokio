//! FreeSWITCH Event Socket Library (ESL) client for Rust
//!
//! This crate provides an async Rust client for FreeSWITCH's Event Socket Library (ESL),
//! allowing applications to connect to FreeSWITCH, execute commands, and receive events.
//!
//! # Architecture
//!
//! The library uses a split reader/writer design:
//! - [`EslClient`] (Clone + Send) — send commands from any task
//! - [`EslEventStream`] — receive events from a background reader task
//!
//! # Examples
//!
//! ## Inbound Connection
//!
//! ```rust,no_run
//! use freeswitch_esl_tokio::{EslClient, EslError};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), EslError> {
//!     let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;
//!
//!     let response = client.api("status").await?;
//!     println!("Status: {}", response.body().unwrap_or("No body"));
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Outbound Mode
//!
//! In outbound mode, FreeSWITCH connects to *your* application via the
//! [`socket`](https://developer.signalwire.com/freeswitch/FreeSWITCH-Explained/Modules/mod_event_socket_1048924/)
//! dialplan application. You run a TCP listener and accept connections:
//!
//! ```rust,no_run
//! use freeswitch_esl_tokio::{EslClient, EslError, AppCommand, EventFormat, EventHeader};
//! use tokio::net::TcpListener;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), EslError> {
//!     let listener = TcpListener::bind("0.0.0.0:8040").await
//!         .map_err(EslError::from)?;
//!
//!     let (client, mut events) = EslClient::accept_outbound(&listener).await?;
//!
//!     // First command must be connect_session — establishes the session
//!     // and returns all channel variables as headers.
//!     let channel_data = client.connect_session().await?;
//!     // HeaderLookup trait provides typed header access via EventHeader enum
//!     println!("Channel: {}", channel_data.header(EventHeader::ChannelName).unwrap_or("?"));
//!
//!     client.myevents(EventFormat::Plain).await?;
//!     client.linger(None).await?; // keep socket open after hangup
//!     client.resume().await?;     // resume dialplan on disconnect
//!
//!     client.send_command(AppCommand::answer()).await?;
//!     client.send_command(AppCommand::playback("ivr/ivr-welcome.wav")).await?;
//!
//!     while let Some(Ok(event)) = events.recv().await {
//!         println!("{:?}", event.event_type());
//!     }
//!     Ok(())
//! }
//! ```
//!
//! Configure FreeSWITCH to connect to your app:
//! ```xml
//! <action application="socket" data="127.0.0.1:8040 async full"/>
//! ```
//!
//! See [`docs/outbound-esl-quirks.md`](https://github.com/ticpu/freeswitch-esl-tokio/blob/master/docs/outbound-esl-quirks.md)
//! for protocol details and command availability by mode.
//!
//! ## Command Builders
//!
//! Typed builders for common API commands — no raw string assembly needed:
//!
//! ```rust
//! use freeswitch_esl_tokio::{Originate, Endpoint, Application};
//! use freeswitch_esl_tokio::commands::SofiaGateway;
//!
//! let cmd = Originate::application(
//!     Endpoint::SofiaGateway(SofiaGateway::new("my_provider", "18005551234")),
//!     Application::simple("park"),
//! )
//! .cid_name("Outbound Call")
//! .cid_num("5551234")
//! .timeout(30);
//!
//! // Use with client.api(&cmd.to_string()) or client.bgapi(&cmd.to_string())
//! assert!(cmd.to_string().contains("sofia/gateway/my_provider/18005551234"));
//! ```
//!
//! See the [`commands`] module for `Originate`, `UuidBridge`, `UuidTransfer`,
//! and other builders.
//!
//! ## Event Subscription
//!
//! ```rust,no_run
//! use freeswitch_esl_tokio::{EslClient, EslEventType, EventFormat, HeaderLookup};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let (client, mut events) = EslClient::connect("localhost", 8021, "ClueCon").await?;
//!
//!     client.subscribe_events(EventFormat::Plain, &[
//!         EslEventType::ChannelAnswer,
//!         EslEventType::ChannelHangup
//!     ]).await?;
//!
//!     while let Some(Ok(event)) = events.recv().await {
//!         // Typed accessors parse headers into enums automatically
//!         if let Ok(Some(state)) = event.channel_state() {
//!             println!("{:?}: {}", event.event_type(), state);
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```

#[macro_use]
mod macros;

pub mod app;
pub mod channel;
pub mod commands;
pub mod connection;
pub mod error;
pub mod event;
pub mod headers;
pub mod lookup;
pub mod variables;

pub(crate) mod buffer;
pub(crate) mod command;
pub(crate) mod constants;
pub(crate) mod protocol;

pub use app::dptools::AppCommand;
pub use channel::{
    AnswerState, CallDirection, CallState, ChannelState, ChannelTimetable, ParseAnswerStateError,
    ParseCallDirectionError, ParseCallStateError, ParseChannelStateError, ParseTimetableError,
    TimetablePrefix,
};
pub use command::{CommandBuilder, EslCommand, EslResponse, ReplyStatus};
pub use commands::{
    Application, BridgeDialString, DialString, DialplanType, Endpoint, Originate, OriginateError,
    OriginateTarget, UuidAnswer, UuidBridge, UuidDeflect, UuidGetVar, UuidHold, UuidKill,
    UuidSendDtmf, UuidSetVar, UuidTransfer, Variables, VariablesType,
};
pub use connection::{
    ConnectionMode, ConnectionStatus, DisconnectReason, EslClient, EslConnectOptions,
    EslEventStream,
};
pub use constants::DEFAULT_ESL_PORT;
pub use error::{EslError, EslResult};
pub use event::{
    EslEvent, EslEventPriority, EslEventType, EventFormat, ParseEventFormatError,
    ParseEventTypeError, ParsePriorityError,
};
pub use headers::{EventHeader, ParseEventHeaderError};
pub use lookup::HeaderLookup;
pub use variables::{
    ChannelVariable, EslArray, MultipartBody, MultipartItem, ParseChannelVariableError,
    ParseSofiaVariableError, SofiaVariable, VariableName,
};
