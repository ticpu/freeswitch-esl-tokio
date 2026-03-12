//! FreeSWITCH protocol types: channel state, events, headers, commands, and variables.
//!
//! This crate provides the domain types for FreeSWITCH's Event Socket Library (ESL)
//! protocol without any async runtime dependency. Use it standalone for CDR parsing,
//! config generation, command building, or channel variable validation.
//!
//! For async ESL transport (connecting to FreeSWITCH, sending commands, receiving events),
//! see the [`freeswitch-esl-tokio`](https://docs.rs/freeswitch-esl-tokio) crate which
//! re-exports everything from this crate.

#[macro_use]
mod macros;

pub use sip_uri;

pub mod channel;
#[cfg(feature = "esl")]
pub mod commands;
pub mod conference_info;
#[cfg(feature = "esl")]
pub mod event;
pub mod headers;
pub mod lookup;
pub mod prelude;
pub mod sip_header;
pub mod sip_header_addr;
pub mod sip_message;
pub mod variables;

/// Default FreeSWITCH ESL port for inbound connections.
pub const DEFAULT_ESL_PORT: u16 = 8021;

/// Default FreeSWITCH ESL password (`ClueCon`).
pub const DEFAULT_ESL_PASSWORD: &str = "ClueCon";

pub use channel::{
    AnswerState, CallDirection, CallState, ChannelState, ChannelTimetable, HangupCause,
    ParseAnswerStateError, ParseCallDirectionError, ParseCallStateError, ParseChannelStateError,
    ParseHangupCauseError, ParseTimetableError, TimetablePrefix,
};
#[cfg(feature = "esl")]
pub use commands::{
    Application, BridgeDialString, DialString, DialplanType, Endpoint, GroupCallOrder, Originate,
    OriginateError, OriginateTarget, ParseDialplanTypeError, ParseGroupCallOrderError, UuidAnswer,
    UuidBridge, UuidDeflect, UuidGetVar, UuidHold, UuidKill, UuidSendDtmf, UuidSetVar,
    UuidTransfer, Variables, VariablesType,
};
#[cfg(feature = "esl")]
pub use event::{
    EslEvent, EslEventPriority, EslEventType, EventFormat, ParseEventFormatError,
    ParseEventTypeError, ParsePriorityError,
};
pub use headers::{normalize_header_key, EventHeader, ParseEventHeaderError};
pub use lookup::HeaderLookup;
pub use sip_header::{ParseSipHeaderError, SipHeader, SipHeaderLookup};
pub use sip_header_addr::{ParseSipHeaderAddrError, SipHeaderAddr};
pub use sip_message::extract_header;
pub use variables::{
    ChannelVariable, EslArray, MultipartBody, MultipartItem, ParseChannelVariableError,
    SipCallInfo, SipCallInfoEntry, SipCallInfoError, SipGeolocation, SipGeolocationRef,
    VariableName,
};
