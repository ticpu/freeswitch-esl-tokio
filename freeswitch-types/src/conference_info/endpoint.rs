use std::fmt;
use std::str::FromStr;

use super::common::{ExecutionInfo, State};
use super::media::Media;

/// A participant's device/session within a conference (RFC 4575 Section 5.5).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Endpoint {
    /// Unique endpoint identifier (required XML attribute), typically a SIP
    /// contact URI with a `participantid` parameter.
    #[cfg_attr(feature = "serde", serde(rename = "@entity"))]
    pub entity: String,
    /// Partial notification state.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@state", default, skip_serializing_if = "Option::is_none")
    )]
    pub state: Option<State>,
    /// Human-readable device name.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// If this endpoint was REFER'd into the conference.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub referred: Option<ExecutionInfo>,
    /// Current connection status.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub status: Option<EndpointStatus>,
    /// How this endpoint joined the conference.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "joining-method",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub joining_method: Option<JoiningMethod>,
    /// When and by whom this endpoint joined.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "joining-info",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub joining_info: Option<ExecutionInfo>,
    /// How this endpoint disconnected.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "disconnection-method",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub disconnection_method: Option<DisconnectionMethod>,
    /// When and by whom this endpoint was disconnected.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "disconnection-info",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub disconnection_info: Option<ExecutionInfo>,
    /// Active media streams.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "media", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub media: Vec<Media>,
    /// SIP dialog identifiers for this endpoint's leg.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "call-info", default, skip_serializing_if = "Option::is_none")
    )]
    pub call_info: Option<CallInfo>,
}

impl Endpoint {
    /// Create an endpoint with the given entity URI.
    pub fn new(entity: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            state: None,
            display_text: None,
            referred: None,
            status: None,
            joining_method: None,
            joining_info: None,
            disconnection_method: None,
            disconnection_info: None,
            media: Vec::new(),
            call_info: None,
        }
    }

    /// Set the connection status.
    pub fn with_status(mut self, status: EndpointStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Set the joining method.
    pub fn with_joining_method(mut self, method: JoiningMethod) -> Self {
        self.joining_method = Some(method);
        self
    }

    /// Set the joining info.
    pub fn with_joining_info(mut self, info: ExecutionInfo) -> Self {
        self.joining_info = Some(info);
        self
    }

    /// Add a media stream.
    pub fn with_media(mut self, media: Media) -> Self {
        self.media
            .push(media);
        self
    }

    /// Set the SIP call info.
    pub fn with_call_info(mut self, call_info: CallInfo) -> Self {
        self.call_info = Some(call_info);
        self
    }
}

/// Connection status of a conference endpoint (RFC 4575 Section 5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum EndpointStatus {
    /// Awaiting response from endpoint.
    #[cfg_attr(feature = "serde", serde(rename = "pending"))]
    Pending,
    /// Focus is dialing the endpoint.
    #[cfg_attr(feature = "serde", serde(rename = "dialing-out"))]
    DialingOut,
    /// Endpoint is dialing the focus.
    #[cfg_attr(feature = "serde", serde(rename = "dialing-in"))]
    DialingIn,
    /// Ringing, before answer.
    #[cfg_attr(feature = "serde", serde(rename = "alerting"))]
    Alerting,
    /// Participant is on hold.
    #[cfg_attr(feature = "serde", serde(rename = "on-hold"))]
    OnHold,
    /// Active in the conference.
    #[cfg_attr(feature = "serde", serde(rename = "connected"))]
    Connected,
    /// Audio suppressed by the focus.
    #[cfg_attr(feature = "serde", serde(rename = "muted-via-focus"))]
    MutedViaFocus,
    /// Teardown in progress.
    #[cfg_attr(feature = "serde", serde(rename = "disconnecting"))]
    Disconnecting,
    /// Departed or rejected.
    #[cfg_attr(feature = "serde", serde(rename = "disconnected"))]
    Disconnected,
}

impl fmt::Display for EndpointStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::DialingOut => "dialing-out",
            Self::DialingIn => "dialing-in",
            Self::Alerting => "alerting",
            Self::OnHold => "on-hold",
            Self::Connected => "connected",
            Self::MutedViaFocus => "muted-via-focus",
            Self::Disconnecting => "disconnecting",
            Self::Disconnected => "disconnected",
        })
    }
}

/// Error returned when parsing an invalid [`EndpointStatus`] string.
#[derive(Debug, Clone)]
pub struct ParseEndpointStatusError(pub String);

impl fmt::Display for ParseEndpointStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid endpoint status: {:?}", self.0)
    }
}

impl std::error::Error for ParseEndpointStatusError {}

impl FromStr for EndpointStatus {
    type Err = ParseEndpointStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "dialing-out" => Ok(Self::DialingOut),
            "dialing-in" => Ok(Self::DialingIn),
            "alerting" => Ok(Self::Alerting),
            "on-hold" => Ok(Self::OnHold),
            "connected" => Ok(Self::Connected),
            "muted-via-focus" => Ok(Self::MutedViaFocus),
            "disconnecting" => Ok(Self::Disconnecting),
            "disconnected" => Ok(Self::Disconnected),
            other => Err(ParseEndpointStatusError(other.to_owned())),
        }
    }
}

/// How an endpoint joined the conference (RFC 4575 Section 5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum JoiningMethod {
    /// Endpoint dialed the focus (incoming).
    #[cfg_attr(feature = "serde", serde(rename = "dialed-in"))]
    DialedIn,
    /// Focus dialed the endpoint (outgoing).
    #[cfg_attr(feature = "serde", serde(rename = "dialed-out"))]
    DialedOut,
    /// Endpoint is the focus itself.
    #[cfg_attr(feature = "serde", serde(rename = "focus-owner"))]
    FocusOwner,
}

impl fmt::Display for JoiningMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DialedIn => "dialed-in",
            Self::DialedOut => "dialed-out",
            Self::FocusOwner => "focus-owner",
        })
    }
}

/// Error returned when parsing an invalid [`JoiningMethod`] string.
#[derive(Debug, Clone)]
pub struct ParseJoiningMethodError(pub String);

impl fmt::Display for ParseJoiningMethodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid joining method: {:?}", self.0)
    }
}

impl std::error::Error for ParseJoiningMethodError {}

impl FromStr for JoiningMethod {
    type Err = ParseJoiningMethodError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dialed-in" => Ok(Self::DialedIn),
            "dialed-out" => Ok(Self::DialedOut),
            "focus-owner" => Ok(Self::FocusOwner),
            other => Err(ParseJoiningMethodError(other.to_owned())),
        }
    }
}

/// How an endpoint left the conference (RFC 4575 Section 5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum DisconnectionMethod {
    /// Endpoint-initiated BYE.
    #[cfg_attr(feature = "serde", serde(rename = "departed"))]
    Departed,
    /// Focus rejected or removed the endpoint.
    #[cfg_attr(feature = "serde", serde(rename = "booted"))]
    Booted,
    /// Connection attempt failed.
    #[cfg_attr(feature = "serde", serde(rename = "failed"))]
    Failed,
    /// Endpoint returned busy (486).
    #[cfg_attr(feature = "serde", serde(rename = "busy"))]
    Busy,
}

impl fmt::Display for DisconnectionMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Departed => "departed",
            Self::Booted => "booted",
            Self::Failed => "failed",
            Self::Busy => "busy",
        })
    }
}

/// Error returned when parsing an invalid [`DisconnectionMethod`] string.
#[derive(Debug, Clone)]
pub struct ParseDisconnectionMethodError(pub String);

impl fmt::Display for ParseDisconnectionMethodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid disconnection method: {:?}", self.0)
    }
}

impl std::error::Error for ParseDisconnectionMethodError {}

impl FromStr for DisconnectionMethod {
    type Err = ParseDisconnectionMethodError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "departed" => Ok(Self::Departed),
            "booted" => Ok(Self::Booted),
            "failed" => Ok(Self::Failed),
            "busy" => Ok(Self::Busy),
            other => Err(ParseDisconnectionMethodError(other.to_owned())),
        }
    }
}

/// SIP dialog identifiers for a conference endpoint's call leg.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct CallInfo {
    /// SIP dialog identifier triplet.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub sip: Option<SipDialogId>,
}

impl CallInfo {
    /// Create a call-info with SIP dialog identifiers.
    pub fn with_sip(sip: SipDialogId) -> Self {
        Self { sip: Some(sip) }
    }
}

/// SIP dialog identifier triplet (Call-ID, From-tag, To-tag) that uniquely
/// identifies a SIP dialog per RFC 3261.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct SipDialogId {
    /// Human-readable description.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// SIP Call-ID header value.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "call-id", default, skip_serializing_if = "Option::is_none")
    )]
    pub call_id: Option<String>,
    /// SIP From header tag parameter.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "from-tag", default, skip_serializing_if = "Option::is_none")
    )]
    pub from_tag: Option<String>,
    /// SIP To header tag parameter.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "to-tag", default, skip_serializing_if = "Option::is_none")
    )]
    pub to_tag: Option<String>,
}

impl SipDialogId {
    /// Create a SIP dialog ID from the three identifying components.
    pub fn new(
        call_id: impl Into<String>,
        from_tag: impl Into<String>,
        to_tag: impl Into<String>,
    ) -> Self {
        Self {
            display_text: None,
            call_id: Some(call_id.into()),
            from_tag: Some(from_tag.into()),
            to_tag: Some(to_tag.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_status_round_trip() {
        for (s, expected) in [
            ("pending", EndpointStatus::Pending),
            ("dialing-out", EndpointStatus::DialingOut),
            ("dialing-in", EndpointStatus::DialingIn),
            ("alerting", EndpointStatus::Alerting),
            ("on-hold", EndpointStatus::OnHold),
            ("connected", EndpointStatus::Connected),
            ("muted-via-focus", EndpointStatus::MutedViaFocus),
            ("disconnecting", EndpointStatus::Disconnecting),
            ("disconnected", EndpointStatus::Disconnected),
        ] {
            let parsed: EndpointStatus = s
                .parse()
                .unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn joining_method_round_trip() {
        for (s, expected) in [
            ("dialed-in", JoiningMethod::DialedIn),
            ("dialed-out", JoiningMethod::DialedOut),
            ("focus-owner", JoiningMethod::FocusOwner),
        ] {
            let parsed: JoiningMethod = s
                .parse()
                .unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn disconnection_method_round_trip() {
        for (s, expected) in [
            ("departed", DisconnectionMethod::Departed),
            ("booted", DisconnectionMethod::Booted),
            ("failed", DisconnectionMethod::Failed),
            ("busy", DisconnectionMethod::Busy),
        ] {
            let parsed: DisconnectionMethod = s
                .parse()
                .unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn sip_dialog_id_new() {
        let id = SipDialogId::new("call-123", "from-abc", "to-xyz");
        assert_eq!(
            id.call_id
                .as_deref(),
            Some("call-123")
        );
        assert_eq!(
            id.from_tag
                .as_deref(),
            Some("from-abc")
        );
        assert_eq!(
            id.to_tag
                .as_deref(),
            Some("to-xyz")
        );
        assert!(id
            .display_text
            .is_none());
    }

    #[test]
    fn endpoint_builder() {
        let ep = Endpoint::new("sip:alice@example.com")
            .with_status(EndpointStatus::Connected)
            .with_joining_method(JoiningMethod::DialedIn)
            .with_media(super::super::media::Media::new("1").with_type("audio"));
        assert_eq!(ep.entity, "sip:alice@example.com");
        assert_eq!(ep.status, Some(EndpointStatus::Connected));
        assert_eq!(
            ep.media
                .len(),
            1
        );
    }
}
