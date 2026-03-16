//! FreeSWITCH protocol types: channel state, events, headers, commands, and variables.
//!
//! This crate provides the domain types for FreeSWITCH's Event Socket Library (ESL)
//! protocol without any async runtime dependency. Use it standalone for CDR parsing,
//! config generation, command building, or channel variable validation.
//!
//! For async ESL transport (connecting to FreeSWITCH, sending commands, receiving events),
//! see the [`freeswitch-esl-tokio`](https://docs.rs/freeswitch-esl-tokio) crate which
//! re-exports everything from this crate.
//!
//! # Upcoming changes in 0.20
//!
//! The general-purpose SIP header parsing functionality has been extracted to the
//! standalone [`sip-header`](https://crates.io/crates/sip-header) crate. In version 0.20,
//! module paths will change and some types will be re-exported rather than defined locally.
//!
//! To prepare for 0.20, use the crate-root re-exports rather than module paths:
//!
//! - `freeswitch_types::SipCallInfo` instead of `freeswitch_types::variables::SipCallInfo`
//! - `freeswitch_types::SipHeaderAddr` instead of `freeswitch_types::sip_header_addr::SipHeaderAddr`
//! - `freeswitch_types::HistoryInfo` instead of `freeswitch_types::variables::HistoryInfo`
//! - `freeswitch_types::SipGeolocation` instead of `freeswitch_types::variables::SipGeolocation`
//!
//! Code importing these types from crate-root re-exports will continue to work across the
//! 0.19 → 0.20 transition. Code importing from module paths may break when those modules
//! are removed or reorganized.

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
    EslEvent, EslEventPriority, EslEventType, EventFormat, EventSubscription,
    EventSubscriptionError, ParseEventFormatError, ParseEventTypeError, ParsePriorityError,
};
pub use headers::{normalize_header_key, EventHeader, ParseEventHeaderError};
pub use lookup::HeaderLookup;
pub use sip_header::{ParseSipHeaderError, SipHeader, SipHeaderLookup};
pub use sip_header_addr::{ParseSipHeaderAddrError, SipHeaderAddr};
pub use sip_message::extract_header;
pub use variables::{
    ChannelVariable, EslArray, HistoryInfo, HistoryInfoEntry, HistoryInfoError, HistoryInfoReason,
    MultipartBody, MultipartItem, ParseChannelVariableError, SipCallInfo, SipCallInfoEntry,
    SipCallInfoError, SipGeolocation, SipGeolocationRef, VariableName,
};
