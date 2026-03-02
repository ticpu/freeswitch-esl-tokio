//! Channel-related data types extracted from ESL event headers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Channel state from `switch_channel_state_t` — carried in the `Channel-State` header
/// as a string (`CS_ROUTING`) and in `Channel-State-Number` as an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[repr(u8)]
#[allow(missing_docs)]
pub enum ChannelState {
    CsNew = 0,
    CsInit = 1,
    CsRouting = 2,
    CsSoftExecute = 3,
    CsExecute = 4,
    CsExchangeMedia = 5,
    CsPark = 6,
    CsConsumeMedia = 7,
    CsHibernate = 8,
    CsReset = 9,
    CsHangup = 10,
    CsReporting = 11,
    CsDestroy = 12,
    CsNone = 13,
}

impl ChannelState {
    /// Parse from the `Channel-State-Number` integer header value.
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::CsNew),
            1 => Some(Self::CsInit),
            2 => Some(Self::CsRouting),
            3 => Some(Self::CsSoftExecute),
            4 => Some(Self::CsExecute),
            5 => Some(Self::CsExchangeMedia),
            6 => Some(Self::CsPark),
            7 => Some(Self::CsConsumeMedia),
            8 => Some(Self::CsHibernate),
            9 => Some(Self::CsReset),
            10 => Some(Self::CsHangup),
            11 => Some(Self::CsReporting),
            12 => Some(Self::CsDestroy),
            13 => Some(Self::CsNone),
            _ => None,
        }
    }

    /// Integer discriminant matching `switch_channel_state_t`.
    pub fn as_number(&self) -> u8 {
        *self as u8
    }
}

impl fmt::Display for ChannelState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::CsNew => "CS_NEW",
            Self::CsInit => "CS_INIT",
            Self::CsRouting => "CS_ROUTING",
            Self::CsSoftExecute => "CS_SOFT_EXECUTE",
            Self::CsExecute => "CS_EXECUTE",
            Self::CsExchangeMedia => "CS_EXCHANGE_MEDIA",
            Self::CsPark => "CS_PARK",
            Self::CsConsumeMedia => "CS_CONSUME_MEDIA",
            Self::CsHibernate => "CS_HIBERNATE",
            Self::CsReset => "CS_RESET",
            Self::CsHangup => "CS_HANGUP",
            Self::CsReporting => "CS_REPORTING",
            Self::CsDestroy => "CS_DESTROY",
            Self::CsNone => "CS_NONE",
        };
        f.write_str(name)
    }
}

/// Error returned when parsing an invalid channel state string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseChannelStateError(pub String);

impl fmt::Display for ParseChannelStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown channel state: {}", self.0)
    }
}

impl std::error::Error for ParseChannelStateError {}

impl FromStr for ChannelState {
    type Err = ParseChannelStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CS_NEW" => Ok(Self::CsNew),
            "CS_INIT" => Ok(Self::CsInit),
            "CS_ROUTING" => Ok(Self::CsRouting),
            "CS_SOFT_EXECUTE" => Ok(Self::CsSoftExecute),
            "CS_EXECUTE" => Ok(Self::CsExecute),
            "CS_EXCHANGE_MEDIA" => Ok(Self::CsExchangeMedia),
            "CS_PARK" => Ok(Self::CsPark),
            "CS_CONSUME_MEDIA" => Ok(Self::CsConsumeMedia),
            "CS_HIBERNATE" => Ok(Self::CsHibernate),
            "CS_RESET" => Ok(Self::CsReset),
            "CS_HANGUP" => Ok(Self::CsHangup),
            "CS_REPORTING" => Ok(Self::CsReporting),
            "CS_DESTROY" => Ok(Self::CsDestroy),
            "CS_NONE" => Ok(Self::CsNone),
            _ => Err(ParseChannelStateError(s.to_string())),
        }
    }
}

/// Call state from `switch_channel_callstate_t` — carried in the `Channel-Call-State` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum CallState {
    Down,
    Dialing,
    Ringing,
    Early,
    Active,
    Held,
    RingWait,
    Hangup,
    Unheld,
}

impl fmt::Display for CallState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Down => "DOWN",
            Self::Dialing => "DIALING",
            Self::Ringing => "RINGING",
            Self::Early => "EARLY",
            Self::Active => "ACTIVE",
            Self::Held => "HELD",
            Self::RingWait => "RING_WAIT",
            Self::Hangup => "HANGUP",
            Self::Unheld => "UNHELD",
        };
        f.write_str(name)
    }
}

/// Error returned when parsing an invalid call state string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCallStateError(pub String);

impl fmt::Display for ParseCallStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown call state: {}", self.0)
    }
}

impl std::error::Error for ParseCallStateError {}

impl FromStr for CallState {
    type Err = ParseCallStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "DOWN" => Ok(Self::Down),
            "DIALING" => Ok(Self::Dialing),
            "RINGING" => Ok(Self::Ringing),
            "EARLY" => Ok(Self::Early),
            "ACTIVE" => Ok(Self::Active),
            "HELD" => Ok(Self::Held),
            "RING_WAIT" => Ok(Self::RingWait),
            "HANGUP" => Ok(Self::Hangup),
            "UNHELD" => Ok(Self::Unheld),
            _ => Err(ParseCallStateError(s.to_string())),
        }
    }
}

/// Answer state from the `Answer-State` header. Wire format is lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum AnswerState {
    Hangup,
    Answered,
    Early,
    Ringing,
}

impl fmt::Display for AnswerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Hangup => "hangup",
            Self::Answered => "answered",
            Self::Early => "early",
            Self::Ringing => "ringing",
        };
        f.write_str(name)
    }
}

/// Error returned when parsing an invalid answer state string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseAnswerStateError(pub String);

impl fmt::Display for ParseAnswerStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown answer state: {}", self.0)
    }
}

impl std::error::Error for ParseAnswerStateError {}

impl FromStr for AnswerState {
    type Err = ParseAnswerStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hangup" => Ok(Self::Hangup),
            "answered" => Ok(Self::Answered),
            "early" => Ok(Self::Early),
            "ringing" => Ok(Self::Ringing),
            _ => Err(ParseAnswerStateError(s.to_string())),
        }
    }
}

/// Call direction from the `Call-Direction` header. Wire format is lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum CallDirection {
    Inbound,
    Outbound,
}

impl fmt::Display for CallDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        };
        f.write_str(name)
    }
}

/// Error returned when parsing an invalid call direction string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCallDirectionError(pub String);

impl fmt::Display for ParseCallDirectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown call direction: {}", self.0)
    }
}

impl std::error::Error for ParseCallDirectionError {}

impl FromStr for CallDirection {
    type Err = ParseCallDirectionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "inbound" => Ok(Self::Inbound),
            "outbound" => Ok(Self::Outbound),
            _ => Err(ParseCallDirectionError(s.to_string())),
        }
    }
}

/// Error returned when parsing an unknown hangup cause string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseHangupCauseError(pub String);

impl fmt::Display for ParseHangupCauseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown hangup cause: {}", self.0)
    }
}

impl std::error::Error for ParseHangupCauseError {}

/// Hangup cause from `switch_cause_t` (Q.850 + FreeSWITCH extensions).
///
/// Carried in the `Hangup-Cause` header. Wire format is `SCREAMING_SNAKE_CASE`
/// (e.g. `NORMAL_CLEARING`). The numeric value matches the Q.850 cause code
/// for standard causes, or a FreeSWITCH-internal range for extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[repr(u16)]
#[allow(missing_docs)]
pub enum HangupCause {
    None = 0,
    UnallocatedNumber = 1,
    NoRouteTransitNet = 2,
    NoRouteDestination = 3,
    ChannelUnacceptable = 6,
    CallAwardedDelivered = 7,
    NormalClearing = 16,
    UserBusy = 17,
    NoUserResponse = 18,
    NoAnswer = 19,
    SubscriberAbsent = 20,
    CallRejected = 21,
    NumberChanged = 22,
    RedirectionToNewDestination = 23,
    ExchangeRoutingError = 25,
    DestinationOutOfOrder = 27,
    InvalidNumberFormat = 28,
    FacilityRejected = 29,
    ResponseToStatusEnquiry = 30,
    NormalUnspecified = 31,
    NormalCircuitCongestion = 34,
    NetworkOutOfOrder = 38,
    NormalTemporaryFailure = 41,
    SwitchCongestion = 42,
    AccessInfoDiscarded = 43,
    RequestedChanUnavail = 44,
    PreEmpted = 45,
    FacilityNotSubscribed = 50,
    OutgoingCallBarred = 52,
    IncomingCallBarred = 54,
    BearercapabilityNotauth = 57,
    BearercapabilityNotavail = 58,
    ServiceUnavailable = 63,
    BearercapabilityNotimpl = 65,
    ChanNotImplemented = 66,
    FacilityNotImplemented = 69,
    ServiceNotImplemented = 79,
    InvalidCallReference = 81,
    IncompatibleDestination = 88,
    InvalidMsgUnspecified = 95,
    MandatoryIeMissing = 96,
    MessageTypeNonexist = 97,
    WrongMessage = 98,
    IeNonexist = 99,
    InvalidIeContents = 100,
    WrongCallState = 101,
    RecoveryOnTimerExpire = 102,
    MandatoryIeLengthError = 103,
    ProtocolError = 111,
    Interworking = 127,
    Success = 142,
    OriginatorCancel = 487,
    Crash = 700,
    SystemShutdown = 701,
    LoseRace = 702,
    ManagerRequest = 703,
    BlindTransfer = 800,
    AttendedTransfer = 801,
    AllottedTimeout = 802,
    UserChallenge = 803,
    MediaTimeout = 804,
    PickedOff = 805,
    UserNotRegistered = 806,
    ProgressTimeout = 807,
    InvalidGateway = 808,
    GatewayDown = 809,
    InvalidUrl = 810,
    InvalidProfile = 811,
    NoPickup = 812,
    SrtpReadError = 813,
    Bowout = 814,
    BusyEverywhere = 815,
    Decline = 816,
    DoesNotExistAnywhere = 817,
    NotAcceptable = 818,
    Unwanted = 819,
    NoIdentity = 820,
    BadIdentityInfo = 821,
    UnsupportedCertificate = 822,
    InvalidIdentity = 823,
    /// Stale Date (STIR/SHAKEN).
    StaleDate = 824,
    /// Reject all calls.
    RejectAll = 825,
}

impl HangupCause {
    /// Q.850 / FreeSWITCH numeric cause code.
    pub fn as_number(&self) -> u16 {
        *self as u16
    }

    /// Look up by numeric cause code.
    pub fn from_number(n: u16) -> Option<Self> {
        match n {
            0 => Some(Self::None),
            1 => Some(Self::UnallocatedNumber),
            2 => Some(Self::NoRouteTransitNet),
            3 => Some(Self::NoRouteDestination),
            6 => Some(Self::ChannelUnacceptable),
            7 => Some(Self::CallAwardedDelivered),
            16 => Some(Self::NormalClearing),
            17 => Some(Self::UserBusy),
            18 => Some(Self::NoUserResponse),
            19 => Some(Self::NoAnswer),
            20 => Some(Self::SubscriberAbsent),
            21 => Some(Self::CallRejected),
            22 => Some(Self::NumberChanged),
            23 => Some(Self::RedirectionToNewDestination),
            25 => Some(Self::ExchangeRoutingError),
            27 => Some(Self::DestinationOutOfOrder),
            28 => Some(Self::InvalidNumberFormat),
            29 => Some(Self::FacilityRejected),
            30 => Some(Self::ResponseToStatusEnquiry),
            31 => Some(Self::NormalUnspecified),
            34 => Some(Self::NormalCircuitCongestion),
            38 => Some(Self::NetworkOutOfOrder),
            41 => Some(Self::NormalTemporaryFailure),
            42 => Some(Self::SwitchCongestion),
            43 => Some(Self::AccessInfoDiscarded),
            44 => Some(Self::RequestedChanUnavail),
            45 => Some(Self::PreEmpted),
            50 => Some(Self::FacilityNotSubscribed),
            52 => Some(Self::OutgoingCallBarred),
            54 => Some(Self::IncomingCallBarred),
            57 => Some(Self::BearercapabilityNotauth),
            58 => Some(Self::BearercapabilityNotavail),
            63 => Some(Self::ServiceUnavailable),
            65 => Some(Self::BearercapabilityNotimpl),
            66 => Some(Self::ChanNotImplemented),
            69 => Some(Self::FacilityNotImplemented),
            79 => Some(Self::ServiceNotImplemented),
            81 => Some(Self::InvalidCallReference),
            88 => Some(Self::IncompatibleDestination),
            95 => Some(Self::InvalidMsgUnspecified),
            96 => Some(Self::MandatoryIeMissing),
            97 => Some(Self::MessageTypeNonexist),
            98 => Some(Self::WrongMessage),
            99 => Some(Self::IeNonexist),
            100 => Some(Self::InvalidIeContents),
            101 => Some(Self::WrongCallState),
            102 => Some(Self::RecoveryOnTimerExpire),
            103 => Some(Self::MandatoryIeLengthError),
            111 => Some(Self::ProtocolError),
            127 => Some(Self::Interworking),
            142 => Some(Self::Success),
            487 => Some(Self::OriginatorCancel),
            700 => Some(Self::Crash),
            701 => Some(Self::SystemShutdown),
            702 => Some(Self::LoseRace),
            703 => Some(Self::ManagerRequest),
            800 => Some(Self::BlindTransfer),
            801 => Some(Self::AttendedTransfer),
            802 => Some(Self::AllottedTimeout),
            803 => Some(Self::UserChallenge),
            804 => Some(Self::MediaTimeout),
            805 => Some(Self::PickedOff),
            806 => Some(Self::UserNotRegistered),
            807 => Some(Self::ProgressTimeout),
            808 => Some(Self::InvalidGateway),
            809 => Some(Self::GatewayDown),
            810 => Some(Self::InvalidUrl),
            811 => Some(Self::InvalidProfile),
            812 => Some(Self::NoPickup),
            813 => Some(Self::SrtpReadError),
            814 => Some(Self::Bowout),
            815 => Some(Self::BusyEverywhere),
            816 => Some(Self::Decline),
            817 => Some(Self::DoesNotExistAnywhere),
            818 => Some(Self::NotAcceptable),
            819 => Some(Self::Unwanted),
            820 => Some(Self::NoIdentity),
            821 => Some(Self::BadIdentityInfo),
            822 => Some(Self::UnsupportedCertificate),
            823 => Some(Self::InvalidIdentity),
            824 => Some(Self::StaleDate),
            825 => Some(Self::RejectAll),
            _ => None,
        }
    }
}

impl fmt::Display for HangupCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::None => "NONE",
            Self::UnallocatedNumber => "UNALLOCATED_NUMBER",
            Self::NoRouteTransitNet => "NO_ROUTE_TRANSIT_NET",
            Self::NoRouteDestination => "NO_ROUTE_DESTINATION",
            Self::ChannelUnacceptable => "CHANNEL_UNACCEPTABLE",
            Self::CallAwardedDelivered => "CALL_AWARDED_DELIVERED",
            Self::NormalClearing => "NORMAL_CLEARING",
            Self::UserBusy => "USER_BUSY",
            Self::NoUserResponse => "NO_USER_RESPONSE",
            Self::NoAnswer => "NO_ANSWER",
            Self::SubscriberAbsent => "SUBSCRIBER_ABSENT",
            Self::CallRejected => "CALL_REJECTED",
            Self::NumberChanged => "NUMBER_CHANGED",
            Self::RedirectionToNewDestination => "REDIRECTION_TO_NEW_DESTINATION",
            Self::ExchangeRoutingError => "EXCHANGE_ROUTING_ERROR",
            Self::DestinationOutOfOrder => "DESTINATION_OUT_OF_ORDER",
            Self::InvalidNumberFormat => "INVALID_NUMBER_FORMAT",
            Self::FacilityRejected => "FACILITY_REJECTED",
            Self::ResponseToStatusEnquiry => "RESPONSE_TO_STATUS_ENQUIRY",
            Self::NormalUnspecified => "NORMAL_UNSPECIFIED",
            Self::NormalCircuitCongestion => "NORMAL_CIRCUIT_CONGESTION",
            Self::NetworkOutOfOrder => "NETWORK_OUT_OF_ORDER",
            Self::NormalTemporaryFailure => "NORMAL_TEMPORARY_FAILURE",
            Self::SwitchCongestion => "SWITCH_CONGESTION",
            Self::AccessInfoDiscarded => "ACCESS_INFO_DISCARDED",
            Self::RequestedChanUnavail => "REQUESTED_CHAN_UNAVAIL",
            Self::PreEmpted => "PRE_EMPTED",
            Self::FacilityNotSubscribed => "FACILITY_NOT_SUBSCRIBED",
            Self::OutgoingCallBarred => "OUTGOING_CALL_BARRED",
            Self::IncomingCallBarred => "INCOMING_CALL_BARRED",
            Self::BearercapabilityNotauth => "BEARERCAPABILITY_NOTAUTH",
            Self::BearercapabilityNotavail => "BEARERCAPABILITY_NOTAVAIL",
            Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            Self::BearercapabilityNotimpl => "BEARERCAPABILITY_NOTIMPL",
            Self::ChanNotImplemented => "CHAN_NOT_IMPLEMENTED",
            Self::FacilityNotImplemented => "FACILITY_NOT_IMPLEMENTED",
            Self::ServiceNotImplemented => "SERVICE_NOT_IMPLEMENTED",
            Self::InvalidCallReference => "INVALID_CALL_REFERENCE",
            Self::IncompatibleDestination => "INCOMPATIBLE_DESTINATION",
            Self::InvalidMsgUnspecified => "INVALID_MSG_UNSPECIFIED",
            Self::MandatoryIeMissing => "MANDATORY_IE_MISSING",
            Self::MessageTypeNonexist => "MESSAGE_TYPE_NONEXIST",
            Self::WrongMessage => "WRONG_MESSAGE",
            Self::IeNonexist => "IE_NONEXIST",
            Self::InvalidIeContents => "INVALID_IE_CONTENTS",
            Self::WrongCallState => "WRONG_CALL_STATE",
            Self::RecoveryOnTimerExpire => "RECOVERY_ON_TIMER_EXPIRE",
            Self::MandatoryIeLengthError => "MANDATORY_IE_LENGTH_ERROR",
            Self::ProtocolError => "PROTOCOL_ERROR",
            Self::Interworking => "INTERWORKING",
            Self::Success => "SUCCESS",
            Self::OriginatorCancel => "ORIGINATOR_CANCEL",
            Self::Crash => "CRASH",
            Self::SystemShutdown => "SYSTEM_SHUTDOWN",
            Self::LoseRace => "LOSE_RACE",
            Self::ManagerRequest => "MANAGER_REQUEST",
            Self::BlindTransfer => "BLIND_TRANSFER",
            Self::AttendedTransfer => "ATTENDED_TRANSFER",
            Self::AllottedTimeout => "ALLOTTED_TIMEOUT",
            Self::UserChallenge => "USER_CHALLENGE",
            Self::MediaTimeout => "MEDIA_TIMEOUT",
            Self::PickedOff => "PICKED_OFF",
            Self::UserNotRegistered => "USER_NOT_REGISTERED",
            Self::ProgressTimeout => "PROGRESS_TIMEOUT",
            Self::InvalidGateway => "INVALID_GATEWAY",
            Self::GatewayDown => "GATEWAY_DOWN",
            Self::InvalidUrl => "INVALID_URL",
            Self::InvalidProfile => "INVALID_PROFILE",
            Self::NoPickup => "NO_PICKUP",
            Self::SrtpReadError => "SRTP_READ_ERROR",
            Self::Bowout => "BOWOUT",
            Self::BusyEverywhere => "BUSY_EVERYWHERE",
            Self::Decline => "DECLINE",
            Self::DoesNotExistAnywhere => "DOES_NOT_EXIST_ANYWHERE",
            Self::NotAcceptable => "NOT_ACCEPTABLE",
            Self::Unwanted => "UNWANTED",
            Self::NoIdentity => "NO_IDENTITY",
            Self::BadIdentityInfo => "BAD_IDENTITY_INFO",
            Self::UnsupportedCertificate => "UNSUPPORTED_CERTIFICATE",
            Self::InvalidIdentity => "INVALID_IDENTITY",
            Self::StaleDate => "STALE_DATE",
            Self::RejectAll => "REJECT_ALL",
        };
        f.write_str(name)
    }
}

impl FromStr for HangupCause {
    type Err = ParseHangupCauseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "NONE" => Self::None,
            "UNALLOCATED_NUMBER" => Self::UnallocatedNumber,
            "NO_ROUTE_TRANSIT_NET" => Self::NoRouteTransitNet,
            "NO_ROUTE_DESTINATION" => Self::NoRouteDestination,
            "CHANNEL_UNACCEPTABLE" => Self::ChannelUnacceptable,
            "CALL_AWARDED_DELIVERED" => Self::CallAwardedDelivered,
            "NORMAL_CLEARING" => Self::NormalClearing,
            "USER_BUSY" => Self::UserBusy,
            "NO_USER_RESPONSE" => Self::NoUserResponse,
            "NO_ANSWER" => Self::NoAnswer,
            "SUBSCRIBER_ABSENT" => Self::SubscriberAbsent,
            "CALL_REJECTED" => Self::CallRejected,
            "NUMBER_CHANGED" => Self::NumberChanged,
            "REDIRECTION_TO_NEW_DESTINATION" => Self::RedirectionToNewDestination,
            "EXCHANGE_ROUTING_ERROR" => Self::ExchangeRoutingError,
            "DESTINATION_OUT_OF_ORDER" => Self::DestinationOutOfOrder,
            "INVALID_NUMBER_FORMAT" => Self::InvalidNumberFormat,
            "FACILITY_REJECTED" => Self::FacilityRejected,
            "RESPONSE_TO_STATUS_ENQUIRY" => Self::ResponseToStatusEnquiry,
            "NORMAL_UNSPECIFIED" => Self::NormalUnspecified,
            "NORMAL_CIRCUIT_CONGESTION" => Self::NormalCircuitCongestion,
            "NETWORK_OUT_OF_ORDER" => Self::NetworkOutOfOrder,
            "NORMAL_TEMPORARY_FAILURE" => Self::NormalTemporaryFailure,
            "SWITCH_CONGESTION" => Self::SwitchCongestion,
            "ACCESS_INFO_DISCARDED" => Self::AccessInfoDiscarded,
            "REQUESTED_CHAN_UNAVAIL" => Self::RequestedChanUnavail,
            "PRE_EMPTED" => Self::PreEmpted,
            "FACILITY_NOT_SUBSCRIBED" => Self::FacilityNotSubscribed,
            "OUTGOING_CALL_BARRED" => Self::OutgoingCallBarred,
            "INCOMING_CALL_BARRED" => Self::IncomingCallBarred,
            "BEARERCAPABILITY_NOTAUTH" => Self::BearercapabilityNotauth,
            "BEARERCAPABILITY_NOTAVAIL" => Self::BearercapabilityNotavail,
            "SERVICE_UNAVAILABLE" => Self::ServiceUnavailable,
            "BEARERCAPABILITY_NOTIMPL" => Self::BearercapabilityNotimpl,
            "CHAN_NOT_IMPLEMENTED" => Self::ChanNotImplemented,
            "FACILITY_NOT_IMPLEMENTED" => Self::FacilityNotImplemented,
            "SERVICE_NOT_IMPLEMENTED" => Self::ServiceNotImplemented,
            "INVALID_CALL_REFERENCE" => Self::InvalidCallReference,
            "INCOMPATIBLE_DESTINATION" => Self::IncompatibleDestination,
            "INVALID_MSG_UNSPECIFIED" => Self::InvalidMsgUnspecified,
            "MANDATORY_IE_MISSING" => Self::MandatoryIeMissing,
            "MESSAGE_TYPE_NONEXIST" => Self::MessageTypeNonexist,
            "WRONG_MESSAGE" => Self::WrongMessage,
            "IE_NONEXIST" => Self::IeNonexist,
            "INVALID_IE_CONTENTS" => Self::InvalidIeContents,
            "WRONG_CALL_STATE" => Self::WrongCallState,
            "RECOVERY_ON_TIMER_EXPIRE" => Self::RecoveryOnTimerExpire,
            "MANDATORY_IE_LENGTH_ERROR" => Self::MandatoryIeLengthError,
            "PROTOCOL_ERROR" => Self::ProtocolError,
            "INTERWORKING" => Self::Interworking,
            "SUCCESS" => Self::Success,
            "ORIGINATOR_CANCEL" => Self::OriginatorCancel,
            "CRASH" => Self::Crash,
            "SYSTEM_SHUTDOWN" => Self::SystemShutdown,
            "LOSE_RACE" => Self::LoseRace,
            "MANAGER_REQUEST" => Self::ManagerRequest,
            "BLIND_TRANSFER" => Self::BlindTransfer,
            "ATTENDED_TRANSFER" => Self::AttendedTransfer,
            "ALLOTTED_TIMEOUT" => Self::AllottedTimeout,
            "USER_CHALLENGE" => Self::UserChallenge,
            "MEDIA_TIMEOUT" => Self::MediaTimeout,
            "PICKED_OFF" => Self::PickedOff,
            "USER_NOT_REGISTERED" => Self::UserNotRegistered,
            "PROGRESS_TIMEOUT" => Self::ProgressTimeout,
            "INVALID_GATEWAY" => Self::InvalidGateway,
            "GATEWAY_DOWN" => Self::GatewayDown,
            "INVALID_URL" => Self::InvalidUrl,
            "INVALID_PROFILE" => Self::InvalidProfile,
            "NO_PICKUP" => Self::NoPickup,
            "SRTP_READ_ERROR" => Self::SrtpReadError,
            "BOWOUT" => Self::Bowout,
            "BUSY_EVERYWHERE" => Self::BusyEverywhere,
            "DECLINE" => Self::Decline,
            "DOES_NOT_EXIST_ANYWHERE" => Self::DoesNotExistAnywhere,
            "NOT_ACCEPTABLE" => Self::NotAcceptable,
            "UNWANTED" => Self::Unwanted,
            "NO_IDENTITY" => Self::NoIdentity,
            "BAD_IDENTITY_INFO" => Self::BadIdentityInfo,
            "UNSUPPORTED_CERTIFICATE" => Self::UnsupportedCertificate,
            "INVALID_IDENTITY" => Self::InvalidIdentity,
            "STALE_DATE" => Self::StaleDate,
            "REJECT_ALL" => Self::RejectAll,
            _ => return Err(ParseHangupCauseError(s.to_string())),
        })
    }
}

/// Channel timing data from FreeSWITCH's `switch_channel_timetable_t`.
///
/// Timestamps are epoch microseconds (`i64`). A value of `0` means the
/// corresponding event never occurred (e.g., `hungup == Some(0)` means
/// the channel has not hung up yet). `None` means the header was absent
/// or unparseable.
///
/// Extracted from ESL event headers using a prefix (typically `"Caller"`
/// or `"Other-Leg"`). The wire header format is `{prefix}-{suffix}`,
/// e.g. `Caller-Channel-Created-Time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChannelTimetable {
    /// When the caller profile was created.
    pub profile_created: Option<i64>,
    /// When the channel was created.
    pub created: Option<i64>,
    /// When the channel was answered.
    pub answered: Option<i64>,
    /// When early media (183) was received.
    pub progress: Option<i64>,
    /// When media-bearing early media arrived.
    pub progress_media: Option<i64>,
    /// When the channel hung up.
    pub hungup: Option<i64>,
    /// When the channel was transferred.
    pub transferred: Option<i64>,
    /// When the channel was resurrected.
    pub resurrected: Option<i64>,
    /// When the channel was bridged.
    pub bridged: Option<i64>,
    /// Timestamp of the last hold event.
    pub last_hold: Option<i64>,
    /// Accumulated hold time in microseconds.
    pub hold_accum: Option<i64>,
}

/// Header prefix identifying which call leg's timetable to extract.
///
/// FreeSWITCH emits timetable headers as `{prefix}-Channel-Created-Time`, etc.
/// The prefix varies by context — `Caller` for the primary leg, `Other-Leg`
/// for the bridged party, `Channel` in outbound ESL mode, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TimetablePrefix {
    /// Primary call leg (`Caller-*`).
    Caller,
    /// Bridged party (`Other-Leg-*`).
    OtherLeg,
    /// Outbound ESL channel profile (`Channel-*`).
    Channel,
    /// XML dialplan hunt (`Hunt-*`).
    Hunt,
    /// Bridge debug originator (`ORIGINATOR-*`).
    Originator,
    /// Bridge debug originatee (`ORIGINATEE-*`).
    Originatee,
    /// Post-bridge debug originator (`POST-ORIGINATOR-*`).
    PostOriginator,
    /// Post-bridge debug originatee (`POST-ORIGINATEE-*`).
    PostOriginatee,
}

impl TimetablePrefix {
    /// Wire-format prefix string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Caller => "Caller",
            Self::OtherLeg => "Other-Leg",
            Self::Channel => "Channel",
            Self::Hunt => "Hunt",
            Self::Originator => "ORIGINATOR",
            Self::Originatee => "ORIGINATEE",
            Self::PostOriginator => "POST-ORIGINATOR",
            Self::PostOriginatee => "POST-ORIGINATEE",
        }
    }
}

impl fmt::Display for TimetablePrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for TimetablePrefix {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Error returned when a timetable header is present but not a valid `i64`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseTimetableError {
    /// Full header name (e.g. `Caller-Channel-Created-Time`).
    pub header: String,
    /// The unparseable value found in the header.
    pub value: String,
}

impl fmt::Display for ParseTimetableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid timetable value for {}: {:?}",
            self.header, self.value
        )
    }
}

impl std::error::Error for ParseTimetableError {}

impl ParseTimetableError {
    /// Create a new timetable parse error.
    pub fn new(header: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            value: value.into(),
        }
    }
}

impl ChannelTimetable {
    /// Extract a timetable by looking up prefixed header names via a closure.
    ///
    /// The closure receives full header names (e.g. `"Caller-Channel-Created-Time"`)
    /// and should return the raw value if present. Works with any key-value store:
    /// `HashMap<String, String>`, `EslEvent`, `BTreeMap`, etc.
    ///
    /// Returns `Ok(None)` if no timestamp headers with this prefix are present.
    /// Returns `Err` if a header is present but contains an invalid (non-`i64`) value.
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use freeswitch_types::{ChannelTimetable, TimetablePrefix};
    ///
    /// let mut headers: HashMap<String, String> = HashMap::new();
    /// headers.insert("Caller-Channel-Created-Time".into(), "1700000001000000".into());
    ///
    /// // With enum:
    /// let tt = ChannelTimetable::from_lookup(TimetablePrefix::Caller, |k| headers.get(k).map(|v: &String| v.as_str()));
    /// assert!(tt.unwrap().unwrap().created.is_some());
    ///
    /// // With raw string (e.g. for dynamic "Call-1" prefix):
    /// let tt = ChannelTimetable::from_lookup("Caller", |k| headers.get(k).map(|v: &String| v.as_str()));
    /// assert!(tt.unwrap().unwrap().created.is_some());
    /// ```
    pub fn from_lookup<'a>(
        prefix: impl AsRef<str>,
        lookup: impl Fn(&str) -> Option<&'a str>,
    ) -> Result<Option<Self>, ParseTimetableError> {
        let prefix = prefix.as_ref();
        let mut tt = Self::default();
        let mut found = false;

        macro_rules! field {
            ($field:ident, $suffix:literal) => {
                let header = format!("{}-{}", prefix, $suffix);
                if let Some(raw) = lookup(&header) {
                    let v: i64 = raw
                        .parse()
                        .map_err(|_| ParseTimetableError {
                            header: header.clone(),
                            value: raw.to_string(),
                        })?;
                    tt.$field = Some(v);
                    found = true;
                }
            };
        }

        field!(profile_created, "Profile-Created-Time");
        field!(created, "Channel-Created-Time");
        field!(answered, "Channel-Answered-Time");
        field!(progress, "Channel-Progress-Time");
        field!(progress_media, "Channel-Progress-Media-Time");
        field!(hungup, "Channel-Hangup-Time");
        field!(transferred, "Channel-Transfer-Time");
        field!(resurrected, "Channel-Resurrect-Time");
        field!(bridged, "Channel-Bridged-Time");
        field!(last_hold, "Channel-Last-Hold");
        field!(hold_accum, "Channel-Hold-Accum");

        if found {
            Ok(Some(tt))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EslEvent;
    use crate::lookup::HeaderLookup;

    // --- ChannelState tests ---

    #[test]
    fn test_channel_state_display() {
        assert_eq!(ChannelState::CsNew.to_string(), "CS_NEW");
        assert_eq!(ChannelState::CsInit.to_string(), "CS_INIT");
        assert_eq!(ChannelState::CsRouting.to_string(), "CS_ROUTING");
        assert_eq!(ChannelState::CsSoftExecute.to_string(), "CS_SOFT_EXECUTE");
        assert_eq!(ChannelState::CsExecute.to_string(), "CS_EXECUTE");
        assert_eq!(
            ChannelState::CsExchangeMedia.to_string(),
            "CS_EXCHANGE_MEDIA"
        );
        assert_eq!(ChannelState::CsPark.to_string(), "CS_PARK");
        assert_eq!(ChannelState::CsConsumeMedia.to_string(), "CS_CONSUME_MEDIA");
        assert_eq!(ChannelState::CsHibernate.to_string(), "CS_HIBERNATE");
        assert_eq!(ChannelState::CsReset.to_string(), "CS_RESET");
        assert_eq!(ChannelState::CsHangup.to_string(), "CS_HANGUP");
        assert_eq!(ChannelState::CsReporting.to_string(), "CS_REPORTING");
        assert_eq!(ChannelState::CsDestroy.to_string(), "CS_DESTROY");
        assert_eq!(ChannelState::CsNone.to_string(), "CS_NONE");
    }

    #[test]
    fn test_channel_state_from_str() {
        assert_eq!("CS_NEW".parse::<ChannelState>(), Ok(ChannelState::CsNew));
        assert_eq!(
            "CS_EXECUTE".parse::<ChannelState>(),
            Ok(ChannelState::CsExecute)
        );
        assert_eq!(
            "CS_HANGUP".parse::<ChannelState>(),
            Ok(ChannelState::CsHangup)
        );
        assert_eq!(
            "CS_DESTROY".parse::<ChannelState>(),
            Ok(ChannelState::CsDestroy)
        );
    }

    #[test]
    fn test_channel_state_from_str_rejects_wrong_case() {
        assert!("cs_new"
            .parse::<ChannelState>()
            .is_err());
        assert!("Cs_Routing"
            .parse::<ChannelState>()
            .is_err());
    }

    #[test]
    fn test_channel_state_from_str_unknown() {
        assert!("CS_BOGUS"
            .parse::<ChannelState>()
            .is_err());
        assert!(""
            .parse::<ChannelState>()
            .is_err());
    }

    #[test]
    fn test_channel_state_from_number() {
        assert_eq!(ChannelState::from_number(0), Some(ChannelState::CsNew));
        assert_eq!(ChannelState::from_number(4), Some(ChannelState::CsExecute));
        assert_eq!(ChannelState::from_number(10), Some(ChannelState::CsHangup));
        assert_eq!(ChannelState::from_number(13), Some(ChannelState::CsNone));
        assert_eq!(ChannelState::from_number(14), None);
        assert_eq!(ChannelState::from_number(255), None);
    }

    #[test]
    fn test_channel_state_as_number() {
        assert_eq!(ChannelState::CsNew.as_number(), 0);
        assert_eq!(ChannelState::CsExecute.as_number(), 4);
        assert_eq!(ChannelState::CsHangup.as_number(), 10);
        assert_eq!(ChannelState::CsNone.as_number(), 13);
    }

    // --- CallState tests ---

    #[test]
    fn test_call_state_display() {
        assert_eq!(CallState::Down.to_string(), "DOWN");
        assert_eq!(CallState::Dialing.to_string(), "DIALING");
        assert_eq!(CallState::Ringing.to_string(), "RINGING");
        assert_eq!(CallState::Early.to_string(), "EARLY");
        assert_eq!(CallState::Active.to_string(), "ACTIVE");
        assert_eq!(CallState::Held.to_string(), "HELD");
        assert_eq!(CallState::RingWait.to_string(), "RING_WAIT");
        assert_eq!(CallState::Hangup.to_string(), "HANGUP");
        assert_eq!(CallState::Unheld.to_string(), "UNHELD");
    }

    #[test]
    fn test_call_state_from_str() {
        assert_eq!("DOWN".parse::<CallState>(), Ok(CallState::Down));
        assert_eq!("ACTIVE".parse::<CallState>(), Ok(CallState::Active));
        assert_eq!("RING_WAIT".parse::<CallState>(), Ok(CallState::RingWait));
        assert_eq!("UNHELD".parse::<CallState>(), Ok(CallState::Unheld));
    }

    #[test]
    fn test_call_state_from_str_rejects_wrong_case() {
        assert!("down"
            .parse::<CallState>()
            .is_err());
        assert!("Active"
            .parse::<CallState>()
            .is_err());
    }

    #[test]
    fn test_call_state_from_str_unknown() {
        assert!("BOGUS"
            .parse::<CallState>()
            .is_err());
    }

    // --- AnswerState tests ---

    #[test]
    fn test_answer_state_display() {
        assert_eq!(AnswerState::Hangup.to_string(), "hangup");
        assert_eq!(AnswerState::Answered.to_string(), "answered");
        assert_eq!(AnswerState::Early.to_string(), "early");
        assert_eq!(AnswerState::Ringing.to_string(), "ringing");
    }

    #[test]
    fn test_answer_state_from_str() {
        assert_eq!("hangup".parse::<AnswerState>(), Ok(AnswerState::Hangup));
        assert_eq!("answered".parse::<AnswerState>(), Ok(AnswerState::Answered));
        assert_eq!("early".parse::<AnswerState>(), Ok(AnswerState::Early));
        assert_eq!("ringing".parse::<AnswerState>(), Ok(AnswerState::Ringing));
    }

    #[test]
    fn test_answer_state_from_str_rejects_wrong_case() {
        assert!("HANGUP"
            .parse::<AnswerState>()
            .is_err());
        assert!("Answered"
            .parse::<AnswerState>()
            .is_err());
    }

    #[test]
    fn test_answer_state_from_str_unknown() {
        assert!("bogus"
            .parse::<AnswerState>()
            .is_err());
    }

    // --- CallDirection tests ---

    #[test]
    fn test_call_direction_display() {
        assert_eq!(CallDirection::Inbound.to_string(), "inbound");
        assert_eq!(CallDirection::Outbound.to_string(), "outbound");
    }

    #[test]
    fn test_call_direction_from_str() {
        assert_eq!(
            "inbound".parse::<CallDirection>(),
            Ok(CallDirection::Inbound)
        );
        assert_eq!(
            "outbound".parse::<CallDirection>(),
            Ok(CallDirection::Outbound)
        );
    }

    #[test]
    fn test_call_direction_from_str_rejects_wrong_case() {
        assert!("INBOUND"
            .parse::<CallDirection>()
            .is_err());
        assert!("Outbound"
            .parse::<CallDirection>()
            .is_err());
    }

    #[test]
    fn test_call_direction_from_str_unknown() {
        assert!("bogus"
            .parse::<CallDirection>()
            .is_err());
    }

    // --- HangupCause tests ---

    #[test]
    fn hangup_cause_display() {
        assert_eq!(HangupCause::NormalClearing.to_string(), "NORMAL_CLEARING");
        assert_eq!(HangupCause::UserBusy.to_string(), "USER_BUSY");
        assert_eq!(
            HangupCause::OriginatorCancel.to_string(),
            "ORIGINATOR_CANCEL"
        );
        assert_eq!(HangupCause::None.to_string(), "NONE");
    }

    #[test]
    fn hangup_cause_from_str() {
        assert_eq!(
            "NORMAL_CLEARING"
                .parse::<HangupCause>()
                .unwrap(),
            HangupCause::NormalClearing
        );
        assert_eq!(
            "USER_BUSY"
                .parse::<HangupCause>()
                .unwrap(),
            HangupCause::UserBusy
        );
    }

    #[test]
    fn hangup_cause_from_str_rejects_wrong_case() {
        assert!("normal_clearing"
            .parse::<HangupCause>()
            .is_err());
        assert!("User_Busy"
            .parse::<HangupCause>()
            .is_err());
    }

    #[test]
    fn hangup_cause_from_str_unknown() {
        assert!("BOGUS_CAUSE"
            .parse::<HangupCause>()
            .is_err());
    }

    #[test]
    fn hangup_cause_display_round_trip() {
        let causes = [
            HangupCause::None,
            HangupCause::NormalClearing,
            HangupCause::UserBusy,
            HangupCause::NoAnswer,
            HangupCause::OriginatorCancel,
            HangupCause::BlindTransfer,
            HangupCause::InvalidIdentity,
        ];
        for cause in causes {
            let s = cause.to_string();
            let parsed: HangupCause = s
                .parse()
                .unwrap();
            assert_eq!(parsed, cause);
        }
    }

    #[test]
    fn hangup_cause_as_number_q850() {
        assert_eq!(HangupCause::None.as_number(), 0);
        assert_eq!(HangupCause::UnallocatedNumber.as_number(), 1);
        assert_eq!(HangupCause::NormalClearing.as_number(), 16);
        assert_eq!(HangupCause::UserBusy.as_number(), 17);
        assert_eq!(HangupCause::NoAnswer.as_number(), 19);
        assert_eq!(HangupCause::CallRejected.as_number(), 21);
        assert_eq!(HangupCause::NormalUnspecified.as_number(), 31);
        assert_eq!(HangupCause::Interworking.as_number(), 127);
    }

    #[test]
    fn hangup_cause_as_number_freeswitch_extensions() {
        assert_eq!(HangupCause::Success.as_number(), 142);
        assert_eq!(HangupCause::OriginatorCancel.as_number(), 487);
        assert_eq!(HangupCause::Crash.as_number(), 700);
        assert_eq!(HangupCause::BlindTransfer.as_number(), 800);
        assert_eq!(HangupCause::InvalidIdentity.as_number(), 823);
    }

    #[test]
    fn hangup_cause_from_number_round_trip() {
        let codes: &[u16] = &[0, 1, 16, 17, 19, 21, 31, 127, 142, 487, 700, 800, 823];
        for &code in codes {
            let cause = HangupCause::from_number(code).unwrap();
            assert_eq!(cause.as_number(), code);
        }
    }

    #[test]
    fn hangup_cause_from_number_unknown() {
        assert!(HangupCause::from_number(999).is_none());
        assert!(HangupCause::from_number(4).is_none());
    }

    // --- ChannelTimetable tests ---

    #[test]
    fn caller_timetable_all_fields() {
        let mut event = EslEvent::new();
        event.set_header("Caller-Profile-Created-Time", "1700000000000000");
        event.set_header("Caller-Channel-Created-Time", "1700000001000000");
        event.set_header("Caller-Channel-Answered-Time", "1700000005000000");
        event.set_header("Caller-Channel-Progress-Time", "1700000002000000");
        event.set_header("Caller-Channel-Progress-Media-Time", "1700000003000000");
        event.set_header("Caller-Channel-Hangup-Time", "0");
        event.set_header("Caller-Channel-Transfer-Time", "0");
        event.set_header("Caller-Channel-Resurrect-Time", "0");
        event.set_header("Caller-Channel-Bridged-Time", "1700000006000000");
        event.set_header("Caller-Channel-Last-Hold", "0");
        event.set_header("Caller-Channel-Hold-Accum", "0");

        let tt = event
            .caller_timetable()
            .unwrap()
            .expect("should have timetable");
        assert_eq!(tt.profile_created, Some(1700000000000000));
        assert_eq!(tt.created, Some(1700000001000000));
        assert_eq!(tt.answered, Some(1700000005000000));
        assert_eq!(tt.progress, Some(1700000002000000));
        assert_eq!(tt.progress_media, Some(1700000003000000));
        assert_eq!(tt.hungup, Some(0));
        assert_eq!(tt.transferred, Some(0));
        assert_eq!(tt.resurrected, Some(0));
        assert_eq!(tt.bridged, Some(1700000006000000));
        assert_eq!(tt.last_hold, Some(0));
        assert_eq!(tt.hold_accum, Some(0));
    }

    #[test]
    fn other_leg_timetable() {
        let mut event = EslEvent::new();
        event.set_header("Other-Leg-Profile-Created-Time", "1700000000000000");
        event.set_header("Other-Leg-Channel-Created-Time", "1700000001000000");
        event.set_header("Other-Leg-Channel-Answered-Time", "1700000005000000");
        event.set_header("Other-Leg-Channel-Progress-Time", "0");
        event.set_header("Other-Leg-Channel-Progress-Media-Time", "0");
        event.set_header("Other-Leg-Channel-Hangup-Time", "0");
        event.set_header("Other-Leg-Channel-Transfer-Time", "0");
        event.set_header("Other-Leg-Channel-Resurrect-Time", "0");
        event.set_header("Other-Leg-Channel-Bridged-Time", "1700000006000000");
        event.set_header("Other-Leg-Channel-Last-Hold", "0");
        event.set_header("Other-Leg-Channel-Hold-Accum", "0");

        let tt = event
            .other_leg_timetable()
            .unwrap()
            .expect("should have timetable");
        assert_eq!(tt.created, Some(1700000001000000));
        assert_eq!(tt.bridged, Some(1700000006000000));
    }

    #[test]
    fn timetable_no_headers() {
        let event = EslEvent::new();
        assert_eq!(
            event
                .caller_timetable()
                .unwrap(),
            None
        );
        assert_eq!(
            event
                .other_leg_timetable()
                .unwrap(),
            None
        );
    }

    #[test]
    fn timetable_partial_headers() {
        let mut event = EslEvent::new();
        event.set_header("Caller-Channel-Created-Time", "1700000001000000");

        let tt = event
            .caller_timetable()
            .unwrap()
            .expect("at least one field parsed");
        assert_eq!(tt.created, Some(1700000001000000));
        assert_eq!(tt.answered, None);
        assert_eq!(tt.profile_created, None);
    }

    #[test]
    fn timetable_invalid_value_is_error() {
        let mut event = EslEvent::new();
        event.set_header("Caller-Channel-Created-Time", "not_a_number");

        let err = event
            .caller_timetable()
            .unwrap_err();
        assert_eq!(err.header, "Caller-Channel-Created-Time");
        assert_eq!(err.value, "not_a_number");
    }

    #[test]
    fn timetable_valid_then_invalid_is_error() {
        let mut event = EslEvent::new();
        event.set_header("Caller-Profile-Created-Time", "1700000000000000");
        event.set_header("Caller-Channel-Created-Time", "garbage");

        let err = event
            .caller_timetable()
            .unwrap_err();
        assert_eq!(err.header, "Caller-Channel-Created-Time");
        assert_eq!(err.value, "garbage");
    }

    #[test]
    fn timetable_zero_preserved() {
        let mut event = EslEvent::new();
        event.set_header("Caller-Channel-Hangup-Time", "0");

        let tt = event
            .caller_timetable()
            .unwrap()
            .expect("should have timetable");
        assert_eq!(tt.hungup, Some(0));
    }

    #[test]
    fn timetable_custom_prefix() {
        let mut event = EslEvent::new();
        event.set_header("Channel-Channel-Created-Time", "1700000001000000");

        let tt = event
            .timetable("Channel")
            .unwrap()
            .expect("custom prefix should work");
        assert_eq!(tt.created, Some(1700000001000000));
    }
}
