//! ESL event types and structures

use crate::headers::{normalize_header_key, EventHeader};
use crate::lookup::HeaderLookup;
use crate::sofia::SofiaEventSubclass;
use crate::variables::EslArray;
use indexmap::IndexMap;
use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use std::fmt;
use std::str::FromStr;

/// Event format types supported by FreeSWITCH ESL
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum EventFormat {
    /// Plain text format (default)
    Plain,
    /// JSON format
    Json,
    /// XML format
    Xml,
}

impl EventFormat {
    /// Determine event format from a Content-Type header value.
    ///
    /// Returns `Err` for unrecognized content types to avoid silently
    /// misparsing events if FreeSWITCH adds a new format.
    pub fn from_content_type(ct: &str) -> Result<Self, ParseEventFormatError> {
        match ct {
            "text/event-json" => Ok(Self::Json),
            "text/event-xml" => Ok(Self::Xml),
            "text/event-plain" => Ok(Self::Plain),
            _ => Err(ParseEventFormatError(ct.to_string())),
        }
    }
}

impl fmt::Display for EventFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventFormat::Plain => write!(f, "plain"),
            EventFormat::Json => write!(f, "json"),
            EventFormat::Xml => write!(f, "xml"),
        }
    }
}

impl FromStr for EventFormat {
    type Err = ParseEventFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("plain") {
            Ok(Self::Plain)
        } else if s.eq_ignore_ascii_case("json") {
            Ok(Self::Json)
        } else if s.eq_ignore_ascii_case("xml") {
            Ok(Self::Xml)
        } else {
            Err(ParseEventFormatError(s.to_string()))
        }
    }
}

/// Error returned when parsing an invalid event format string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEventFormatError(pub String);

impl fmt::Display for ParseEventFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown event format: {}", self.0)
    }
}

impl std::error::Error for ParseEventFormatError {}

/// Generates `EslEventType` enum with `Display`, `FromStr`, `as_str`, and `parse_event_type`.
macro_rules! esl_event_types {
    (
        $(
            $(#[$attr:meta])*
            $variant:ident => $wire:literal
        ),+ $(,)?
        ;
        // Extra variants not in the main match (after All)
        $(
            $(#[$extra_attr:meta])*
            $extra_variant:ident => $extra_wire:literal
        ),* $(,)?
    ) => {
        /// FreeSWITCH event types matching the canonical order from `esl_event.h`
        /// and `switch_event.c` EVENT_NAMES[].
        ///
        /// Variant names are the canonical wire names (e.g. `ChannelCreate` = `CHANNEL_CREATE`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[non_exhaustive]
        #[allow(missing_docs)]
        pub enum EslEventType {
            $(
                $(#[$attr])*
                $variant,
            )+
            $(
                $(#[$extra_attr])*
                $extra_variant,
            )*
        }

        impl fmt::Display for EslEventType {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl EslEventType {
            /// Returns the canonical wire name as a static string slice.
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $( EslEventType::$variant => $wire, )+
                    $( EslEventType::$extra_variant => $extra_wire, )*
                }
            }

            /// Parse event type from wire name (canonical case).
            pub fn parse_event_type(s: &str) -> Option<Self> {
                match s {
                    $( $wire => Some(EslEventType::$variant), )+
                    $( $extra_wire => Some(EslEventType::$extra_variant), )*
                    _ => None,
                }
            }
        }

        impl FromStr for EslEventType {
            type Err = ParseEventTypeError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse_event_type(s).ok_or_else(|| ParseEventTypeError(s.to_string()))
            }
        }
    };
}

esl_event_types! {
    Custom => "CUSTOM",
    Clone => "CLONE",
    ChannelCreate => "CHANNEL_CREATE",
    ChannelDestroy => "CHANNEL_DESTROY",
    ChannelState => "CHANNEL_STATE",
    ChannelCallstate => "CHANNEL_CALLSTATE",
    ChannelAnswer => "CHANNEL_ANSWER",
    ChannelHangup => "CHANNEL_HANGUP",
    ChannelHangupComplete => "CHANNEL_HANGUP_COMPLETE",
    ChannelExecute => "CHANNEL_EXECUTE",
    ChannelExecuteComplete => "CHANNEL_EXECUTE_COMPLETE",
    ChannelHold => "CHANNEL_HOLD",
    ChannelUnhold => "CHANNEL_UNHOLD",
    ChannelBridge => "CHANNEL_BRIDGE",
    ChannelUnbridge => "CHANNEL_UNBRIDGE",
    ChannelProgress => "CHANNEL_PROGRESS",
    ChannelProgressMedia => "CHANNEL_PROGRESS_MEDIA",
    ChannelOutgoing => "CHANNEL_OUTGOING",
    ChannelPark => "CHANNEL_PARK",
    ChannelUnpark => "CHANNEL_UNPARK",
    ChannelApplication => "CHANNEL_APPLICATION",
    ChannelOriginate => "CHANNEL_ORIGINATE",
    ChannelUuid => "CHANNEL_UUID",
    Api => "API",
    Log => "LOG",
    InboundChan => "INBOUND_CHAN",
    OutboundChan => "OUTBOUND_CHAN",
    Startup => "STARTUP",
    Shutdown => "SHUTDOWN",
    Publish => "PUBLISH",
    Unpublish => "UNPUBLISH",
    Talk => "TALK",
    Notalk => "NOTALK",
    SessionCrash => "SESSION_CRASH",
    ModuleLoad => "MODULE_LOAD",
    ModuleUnload => "MODULE_UNLOAD",
    Dtmf => "DTMF",
    Message => "MESSAGE",
    PresenceIn => "PRESENCE_IN",
    NotifyIn => "NOTIFY_IN",
    PresenceOut => "PRESENCE_OUT",
    PresenceProbe => "PRESENCE_PROBE",
    MessageWaiting => "MESSAGE_WAITING",
    MessageQuery => "MESSAGE_QUERY",
    Roster => "ROSTER",
    Codec => "CODEC",
    BackgroundJob => "BACKGROUND_JOB",
    DetectedSpeech => "DETECTED_SPEECH",
    DetectedTone => "DETECTED_TONE",
    PrivateCommand => "PRIVATE_COMMAND",
    Heartbeat => "HEARTBEAT",
    Trap => "TRAP",
    AddSchedule => "ADD_SCHEDULE",
    DelSchedule => "DEL_SCHEDULE",
    ExeSchedule => "EXE_SCHEDULE",
    ReSchedule => "RE_SCHEDULE",
    ReloadXml => "RELOADXML",
    Notify => "NOTIFY",
    PhoneFeature => "PHONE_FEATURE",
    PhoneFeatureSubscribe => "PHONE_FEATURE_SUBSCRIBE",
    SendMessage => "SEND_MESSAGE",
    RecvMessage => "RECV_MESSAGE",
    RequestParams => "REQUEST_PARAMS",
    ChannelData => "CHANNEL_DATA",
    General => "GENERAL",
    Command => "COMMAND",
    SessionHeartbeat => "SESSION_HEARTBEAT",
    ClientDisconnected => "CLIENT_DISCONNECTED",
    ServerDisconnected => "SERVER_DISCONNECTED",
    SendInfo => "SEND_INFO",
    RecvInfo => "RECV_INFO",
    RecvRtcpMessage => "RECV_RTCP_MESSAGE",
    SendRtcpMessage => "SEND_RTCP_MESSAGE",
    CallSecure => "CALL_SECURE",
    Nat => "NAT",
    RecordStart => "RECORD_START",
    RecordStop => "RECORD_STOP",
    PlaybackStart => "PLAYBACK_START",
    PlaybackStop => "PLAYBACK_STOP",
    CallUpdate => "CALL_UPDATE",
    Failure => "FAILURE",
    SocketData => "SOCKET_DATA",
    MediaBugStart => "MEDIA_BUG_START",
    MediaBugStop => "MEDIA_BUG_STOP",
    ConferenceDataQuery => "CONFERENCE_DATA_QUERY",
    ConferenceData => "CONFERENCE_DATA",
    CallSetupReq => "CALL_SETUP_REQ",
    CallSetupResult => "CALL_SETUP_RESULT",
    CallDetail => "CALL_DETAIL",
    DeviceState => "DEVICE_STATE",
    Text => "TEXT",
    ShutdownRequested => "SHUTDOWN_REQUESTED",
    /// Subscribe to all events
    All => "ALL";
    // --- Not in libs/esl/ EVENT_NAMES[], only in switch_event.c ---
    // check-event-types.sh stops scanning at the All variant above.
    /// Present in `switch_event.c` but not in `libs/esl/` EVENT_NAMES[].
    StartRecording => "START_RECORDING",
}

// -- Event group constants --------------------------------------------------
//
// Predefined slices for common subscription patterns. Pass directly to
// `EslClient::subscribe_events()`.
//
// MAINTENANCE: when adding new `EslEventType` variants, check whether they
// belong in any of these groups and update accordingly.

impl EslEventType {
    /// Every `CHANNEL_*` event type.
    ///
    /// Covers the full channel lifecycle: creation, state changes, execution,
    /// bridging, hold, park, progress, originate, and destruction.
    ///
    /// ```rust
    /// use freeswitch_types::EslEventType;
    /// assert!(EslEventType::CHANNEL_EVENTS.contains(&EslEventType::ChannelCreate));
    /// assert!(EslEventType::CHANNEL_EVENTS.contains(&EslEventType::ChannelHangupComplete));
    /// ```
    pub const CHANNEL_EVENTS: &[EslEventType] = &[
        EslEventType::ChannelCreate,
        EslEventType::ChannelDestroy,
        EslEventType::ChannelState,
        EslEventType::ChannelCallstate,
        EslEventType::ChannelAnswer,
        EslEventType::ChannelHangup,
        EslEventType::ChannelHangupComplete,
        EslEventType::ChannelExecute,
        EslEventType::ChannelExecuteComplete,
        EslEventType::ChannelHold,
        EslEventType::ChannelUnhold,
        EslEventType::ChannelBridge,
        EslEventType::ChannelUnbridge,
        EslEventType::ChannelProgress,
        EslEventType::ChannelProgressMedia,
        EslEventType::ChannelOutgoing,
        EslEventType::ChannelPark,
        EslEventType::ChannelUnpark,
        EslEventType::ChannelApplication,
        EslEventType::ChannelOriginate,
        EslEventType::ChannelUuid,
        EslEventType::ChannelData,
    ];

    /// In-call events: DTMF, VAD speech detection, media security, and call updates.
    ///
    /// Events that fire during an established call, tied to RTP/media activity
    /// rather than signaling state transitions.
    ///
    /// ```rust
    /// use freeswitch_types::EslEventType;
    /// assert!(EslEventType::IN_CALL_EVENTS.contains(&EslEventType::Dtmf));
    /// assert!(EslEventType::IN_CALL_EVENTS.contains(&EslEventType::Talk));
    /// ```
    pub const IN_CALL_EVENTS: &[EslEventType] = &[
        EslEventType::Dtmf,
        EslEventType::Talk,
        EslEventType::Notalk,
        EslEventType::CallSecure,
        EslEventType::CallUpdate,
        EslEventType::RecvRtcpMessage,
        EslEventType::SendRtcpMessage,
    ];

    /// Media-related events: playback, recording, media bugs, and detection.
    ///
    /// Useful for IVR applications that need to track media operations without
    /// subscribing to the full channel lifecycle.
    ///
    /// ```rust
    /// use freeswitch_types::EslEventType;
    /// assert!(EslEventType::MEDIA_EVENTS.contains(&EslEventType::PlaybackStart));
    /// assert!(EslEventType::MEDIA_EVENTS.contains(&EslEventType::DetectedSpeech));
    /// ```
    pub const MEDIA_EVENTS: &[EslEventType] = &[
        EslEventType::PlaybackStart,
        EslEventType::PlaybackStop,
        EslEventType::RecordStart,
        EslEventType::RecordStop,
        EslEventType::StartRecording,
        EslEventType::MediaBugStart,
        EslEventType::MediaBugStop,
        EslEventType::DetectedSpeech,
        EslEventType::DetectedTone,
    ];

    /// Presence and messaging events.
    ///
    /// For applications that track user presence (BLF, buddy lists) or
    /// message-waiting indicators (voicemail MWI).
    ///
    /// ```rust
    /// use freeswitch_types::EslEventType;
    /// assert!(EslEventType::PRESENCE_EVENTS.contains(&EslEventType::PresenceIn));
    /// assert!(EslEventType::PRESENCE_EVENTS.contains(&EslEventType::MessageWaiting));
    /// ```
    pub const PRESENCE_EVENTS: &[EslEventType] = &[
        EslEventType::PresenceIn,
        EslEventType::PresenceOut,
        EslEventType::PresenceProbe,
        EslEventType::MessageWaiting,
        EslEventType::MessageQuery,
        EslEventType::Roster,
    ];

    /// System lifecycle events.
    ///
    /// Server startup/shutdown, heartbeats, module loading, and XML reloads.
    /// Useful for monitoring dashboards and operational tooling.
    ///
    /// ```rust
    /// use freeswitch_types::EslEventType;
    /// assert!(EslEventType::SYSTEM_EVENTS.contains(&EslEventType::Heartbeat));
    /// assert!(EslEventType::SYSTEM_EVENTS.contains(&EslEventType::Shutdown));
    /// ```
    pub const SYSTEM_EVENTS: &[EslEventType] = &[
        EslEventType::Startup,
        EslEventType::Shutdown,
        EslEventType::ShutdownRequested,
        EslEventType::Heartbeat,
        EslEventType::SessionHeartbeat,
        EslEventType::SessionCrash,
        EslEventType::ModuleLoad,
        EslEventType::ModuleUnload,
        EslEventType::ReloadXml,
    ];

    /// Conference-related events.
    ///
    /// ```rust
    /// use freeswitch_types::EslEventType;
    /// assert!(EslEventType::CONFERENCE_EVENTS.contains(&EslEventType::ConferenceData));
    /// ```
    pub const CONFERENCE_EVENTS: &[EslEventType] = &[
        EslEventType::ConferenceDataQuery,
        EslEventType::ConferenceData,
    ];
}

/// Error returned when parsing an unknown event type string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEventTypeError(pub String);

impl fmt::Display for ParseEventTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown event type: {}", self.0)
    }
}

impl std::error::Error for ParseEventTypeError {}

/// Error returned when an [`EventSubscription`] builder method receives invalid input.
///
/// Custom subclasses and filter values are validated against ESL wire-safety
/// constraints: no newlines, carriage returns, or (for subclasses) spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSubscriptionError(pub String);

impl fmt::Display for EventSubscriptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid event subscription: {}", self.0)
    }
}

impl std::error::Error for EventSubscriptionError {}

/// Declarative description of an ESL event subscription.
///
/// Captures the event format, event types, custom subclasses, and filters
/// as a single unit. Useful for config-driven subscriptions and reconnection
/// patterns where the caller needs to rebuild subscriptions from a saved
/// description.
///
/// # Wire safety
///
/// Builder methods validate inputs against ESL wire injection risks.
/// Custom subclasses reject `\n`, `\r`, spaces, and empty strings.
/// Filter headers and values reject `\n` and `\r`.
///
/// # Example
///
/// ```rust
/// use freeswitch_types::{EventSubscription, EventFormat, EslEventType, EventHeader};
///
/// let sub = EventSubscription::new(EventFormat::Plain)
///     .events(EslEventType::CHANNEL_EVENTS)
///     .event(EslEventType::Heartbeat)
///     .custom_subclass("sofia::register").unwrap()
///     .filter(EventHeader::CallDirection, "inbound").unwrap();
///
/// assert!(!sub.is_empty());
/// assert!(!sub.is_all());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventSubscription {
    format: EventFormat,
    events: Vec<EslEventType>,
    custom_subclasses: Vec<String>,
    filters: Vec<(String, String)>,
}

/// Validates that a custom subclass token is safe for ESL wire use.
fn validate_custom_subclass(s: &str) -> Result<(), EventSubscriptionError> {
    if s.is_empty() {
        return Err(EventSubscriptionError(
            "custom subclass cannot be empty".into(),
        ));
    }
    if s.contains('\n') || s.contains('\r') {
        return Err(EventSubscriptionError(format!(
            "custom subclass contains newline: {:?}",
            s
        )));
    }
    if s.contains(' ') {
        return Err(EventSubscriptionError(format!(
            "custom subclass contains space: {:?}",
            s
        )));
    }
    Ok(())
}

/// Validates that a filter header or value has no newline characters.
fn validate_filter_field(field: &str, label: &str) -> Result<(), EventSubscriptionError> {
    if field.contains('\n') || field.contains('\r') {
        return Err(EventSubscriptionError(format!(
            "filter {} contains newline: {:?}",
            label, field
        )));
    }
    Ok(())
}

impl EventSubscription {
    /// Create an empty subscription with the given format.
    pub fn new(format: EventFormat) -> Self {
        Self {
            format,
            events: Vec::new(),
            custom_subclasses: Vec::new(),
            filters: Vec::new(),
        }
    }

    /// Create a subscription for all events.
    pub fn all(format: EventFormat) -> Self {
        Self {
            format,
            events: vec![EslEventType::All],
            custom_subclasses: Vec::new(),
            filters: Vec::new(),
        }
    }

    /// Add a single event type.
    pub fn event(mut self, event: EslEventType) -> Self {
        self.events
            .push(event);
        self
    }

    /// Add multiple event types (e.g. from group constants like `EslEventType::CHANNEL_EVENTS`).
    pub fn events<T: IntoIterator<Item = impl std::borrow::Borrow<EslEventType>>>(
        mut self,
        events: T,
    ) -> Self {
        self.events
            .extend(
                events
                    .into_iter()
                    .map(|e| *e.borrow()),
            );
        self
    }

    /// Add a custom subclass (e.g. `"sofia::register"`).
    ///
    /// Returns `Err` if the subclass contains spaces, newlines, or is empty.
    pub fn custom_subclass(
        mut self,
        subclass: impl Into<String>,
    ) -> Result<Self, EventSubscriptionError> {
        let s = subclass.into();
        validate_custom_subclass(&s)?;
        self.custom_subclasses
            .push(s);
        Ok(self)
    }

    /// Add multiple custom subclasses.
    ///
    /// Returns `Err` on the first invalid subclass.
    pub fn custom_subclasses(
        mut self,
        subclasses: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, EventSubscriptionError> {
        for s in subclasses {
            let s = s.into();
            validate_custom_subclass(&s)?;
            self.custom_subclasses
                .push(s);
        }
        Ok(self)
    }

    /// Subscribe to a single Sofia event subclass.
    ///
    /// Convenience wrapper around [`custom_subclass()`](Self::custom_subclass) that
    /// accepts a typed [`SofiaEventSubclass`] instead of a raw string.
    pub fn sofia_event(mut self, subclass: SofiaEventSubclass) -> Self {
        self.custom_subclasses
            .push(
                subclass
                    .as_str()
                    .to_string(),
            );
        self
    }

    /// Subscribe to multiple Sofia event subclasses.
    pub fn sofia_events(
        mut self,
        subclasses: impl IntoIterator<Item = impl std::borrow::Borrow<SofiaEventSubclass>>,
    ) -> Self {
        self.custom_subclasses
            .extend(
                subclasses
                    .into_iter()
                    .map(|s| {
                        s.borrow()
                            .as_str()
                            .to_string()
                    }),
            );
        self
    }

    /// Add a filter with a typed header.
    ///
    /// The header enum is always valid; only the value is validated.
    pub fn filter(
        self,
        header: crate::headers::EventHeader,
        value: impl Into<String>,
    ) -> Result<Self, EventSubscriptionError> {
        let v = value.into();
        validate_filter_field(&v, "value")?;
        let mut s = self;
        s.filters
            .push((
                header
                    .as_str()
                    .to_string(),
                v,
            ));
        Ok(s)
    }

    /// Add a filter with raw header and value strings.
    ///
    /// Both header and value are validated against newline injection.
    pub fn filter_raw(
        self,
        header: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, EventSubscriptionError> {
        let h = header.into();
        let v = value.into();
        validate_filter_field(&h, "header")?;
        validate_filter_field(&v, "value")?;
        let mut s = self;
        s.filters
            .push((h, v));
        Ok(s)
    }

    /// Change the event format.
    pub fn with_format(mut self, format: EventFormat) -> Self {
        self.format = format;
        self
    }

    /// The event format.
    pub fn format(&self) -> EventFormat {
        self.format
    }

    /// Mutable reference to the event format.
    pub fn format_mut(&mut self) -> &mut EventFormat {
        &mut self.format
    }

    /// The subscribed event types.
    pub fn event_types(&self) -> &[EslEventType] {
        &self.events
    }

    /// Mutable access to the event types list.
    pub fn event_types_mut(&mut self) -> &mut Vec<EslEventType> {
        &mut self.events
    }

    /// The subscribed custom subclasses.
    pub fn custom_subclass_list(&self) -> &[String] {
        &self.custom_subclasses
    }

    /// Mutable access to the custom subclasses list.
    pub fn custom_subclasses_mut(&mut self) -> &mut Vec<String> {
        &mut self.custom_subclasses
    }

    /// The event filters as (header, value) pairs.
    pub fn filters(&self) -> &[(String, String)] {
        &self.filters
    }

    /// Mutable access to the filters list.
    pub fn filters_mut(&mut self) -> &mut Vec<(String, String)> {
        &mut self.filters
    }

    /// Whether the subscription includes all events.
    pub fn is_all(&self) -> bool {
        self.events
            .contains(&EslEventType::All)
    }

    /// Whether the subscription has no events and no custom subclasses.
    pub fn is_empty(&self) -> bool {
        self.events
            .is_empty()
            && self
                .custom_subclasses
                .is_empty()
    }

    /// Build the event string for the ESL `event` command.
    ///
    /// Returns `None` if no events or custom subclasses are configured.
    /// Returns `Some("ALL")` if `EslEventType::All` is present.
    /// Otherwise returns space-separated event names with custom subclasses
    /// appended after a `CUSTOM` token.
    pub fn to_event_string(&self) -> Option<String> {
        if self
            .events
            .contains(&EslEventType::All)
        {
            return Some("ALL".to_string());
        }

        let mut parts: Vec<&str> = self
            .events
            .iter()
            .map(|e| e.as_str())
            .collect();

        if !self
            .custom_subclasses
            .is_empty()
        {
            if !self
                .events
                .contains(&EslEventType::Custom)
            {
                parts.push("CUSTOM");
            }
            for sc in &self.custom_subclasses {
                parts.push(sc.as_str());
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

#[cfg(feature = "serde")]
mod event_subscription_serde {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct EventSubscriptionRaw {
        format: EventFormat,
        #[serde(default)]
        events: Vec<EslEventType>,
        #[serde(default)]
        custom_subclasses: Vec<String>,
        #[serde(default)]
        filters: Vec<(String, String)>,
    }

    impl TryFrom<EventSubscriptionRaw> for EventSubscription {
        type Error = EventSubscriptionError;

        fn try_from(raw: EventSubscriptionRaw) -> Result<Self, Self::Error> {
            for sc in &raw.custom_subclasses {
                validate_custom_subclass(sc)?;
            }
            for (h, v) in &raw.filters {
                validate_filter_field(h, "header")?;
                validate_filter_field(v, "value")?;
            }
            Ok(EventSubscription {
                format: raw.format,
                events: raw.events,
                custom_subclasses: raw.custom_subclasses,
                filters: raw.filters,
            })
        }
    }

    impl Serialize for EventSubscription {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let raw = EventSubscriptionRaw {
                format: self.format,
                events: self
                    .events
                    .clone(),
                custom_subclasses: self
                    .custom_subclasses
                    .clone(),
                filters: self
                    .filters
                    .clone(),
            };
            raw.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for EventSubscription {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let raw = EventSubscriptionRaw::deserialize(deserializer)?;
            EventSubscription::try_from(raw).map_err(serde::de::Error::custom)
        }
    }
}

/// Event priority levels matching FreeSWITCH `esl_priority_t`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum EslEventPriority {
    /// Default priority.
    Normal,
    /// Lower than normal.
    Low,
    /// Higher than normal.
    High,
}

impl fmt::Display for EslEventPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EslEventPriority::Normal => write!(f, "NORMAL"),
            EslEventPriority::Low => write!(f, "LOW"),
            EslEventPriority::High => write!(f, "HIGH"),
        }
    }
}

/// Error returned when parsing an invalid priority string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePriorityError(pub String);

impl fmt::Display for ParsePriorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown priority: {}", self.0)
    }
}

impl std::error::Error for ParsePriorityError {}

impl FromStr for EslEventPriority {
    type Err = ParsePriorityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "NORMAL" => Ok(EslEventPriority::Normal),
            "LOW" => Ok(EslEventPriority::Low),
            "HIGH" => Ok(EslEventPriority::High),
            _ => Err(ParsePriorityError(s.to_string())),
        }
    }
}

/// ESL Event structure containing headers and optional body
#[derive(Debug, Clone, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EslEvent {
    event_type: Option<EslEventType>,
    headers: IndexMap<String, String>,
    #[cfg_attr(feature = "serde", serde(skip))]
    original_keys: IndexMap<String, String>,
    body: Option<String>,
}

impl EslEvent {
    /// Create a new empty event
    pub fn new() -> Self {
        Self {
            event_type: None,
            headers: IndexMap::new(),
            original_keys: IndexMap::new(),
            body: None,
        }
    }

    /// Create event with specified type
    pub fn with_type(event_type: EslEventType) -> Self {
        Self {
            event_type: Some(event_type),
            headers: IndexMap::new(),
            original_keys: IndexMap::new(),
            body: None,
        }
    }

    /// Parsed event type, if recognized.
    pub fn event_type(&self) -> Option<EslEventType> {
        self.event_type
    }

    /// Override the event type.
    pub fn set_event_type(&mut self, event_type: Option<EslEventType>) {
        self.event_type = event_type;
    }

    /// Look up a header by its [`EventHeader`] enum variant (case-sensitive).
    ///
    /// For headers not covered by `EventHeader`, use [`header_str()`](Self::header_str).
    pub fn header(&self, name: EventHeader) -> Option<&str> {
        self.headers
            .get(name.as_str())
            .map(|s| s.as_str())
    }

    /// Look up a header by name, trying the canonical key first then falling
    /// back through the alias map for non-canonical lookups.
    ///
    /// Use [`header()`](Self::header) with an [`EventHeader`] variant for known
    /// headers. This method is for headers not (yet) covered by the enum,
    /// such as custom `X-` headers or FreeSWITCH headers added after this
    /// library was published.
    pub fn header_str(&self, name: &str) -> Option<&str> {
        self.headers
            .get(name)
            .or_else(|| {
                self.original_keys
                    .get(name)
                    .and_then(|normalized| {
                        self.headers
                            .get(normalized)
                    })
            })
            .map(|s| s.as_str())
    }

    /// Look up a channel variable by its bare name.
    ///
    /// Equivalent to [`variable()`](Self::variable) but matches the
    /// [`HeaderLookup`] trait signature.
    pub fn variable_str(&self, name: &str) -> Option<&str> {
        let key = format!("variable_{}", name);
        self.header_str(&key)
    }

    /// All headers as a map.
    pub fn headers(&self) -> &IndexMap<String, String> {
        &self.headers
    }

    /// Set or overwrite a header, normalizing the key.
    pub fn set_header(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let original = name.into();
        let normalized = normalize_header_key(&original);
        if original != normalized {
            self.original_keys
                .insert(original, normalized.clone());
        }
        self.headers
            .insert(normalized, value.into());
    }

    /// Remove a header, returning its value if it existed.
    ///
    /// Accepts both canonical and original (non-normalized) key names.
    pub fn remove_header(&mut self, name: impl AsRef<str>) -> Option<String> {
        let name = name.as_ref();
        if let Some(value) = self
            .headers
            .shift_remove(name)
        {
            return Some(value);
        }
        if let Some(normalized) = self
            .original_keys
            .shift_remove(name)
        {
            return self
                .headers
                .shift_remove(&normalized);
        }
        None
    }

    /// Event body (the content after the blank line in plain-text events).
    pub fn body(&self) -> Option<&str> {
        self.body
            .as_deref()
    }

    /// Set the event body.
    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = Some(body.into());
    }

    /// Sets the `priority` header carried on the event.
    ///
    /// FreeSWITCH stores this as metadata but does **not** use it for dispatch
    /// ordering -- all events are delivered FIFO regardless of priority.
    pub fn set_priority(&mut self, priority: EslEventPriority) {
        self.set_header(EventHeader::Priority.as_str(), priority.to_string());
    }

    /// Append a value to a multi-value header (PUSH semantics).
    ///
    /// If the header doesn't exist, sets it as a plain value.
    /// If it exists as a plain value, converts to `ARRAY::old|:new`.
    /// If it already has an `ARRAY::` prefix, appends the new value.
    ///
    /// ```
    /// # use freeswitch_types::EslEvent;
    /// let mut event = EslEvent::new();
    /// event.push_header("X-Test", "first");
    /// event.push_header("X-Test", "second");
    /// assert_eq!(event.header_str("X-Test"), Some("ARRAY::first|:second"));
    /// ```
    pub fn push_header(&mut self, name: &str, value: &str) {
        self.stack_header(name, value, EslArray::push);
    }

    /// Prepend a value to a multi-value header (UNSHIFT semantics).
    ///
    /// Same conversion rules as `push_header()`, but inserts at the front.
    ///
    /// ```
    /// # use freeswitch_types::EslEvent;
    /// let mut event = EslEvent::new();
    /// event.set_header("X-Test", "ARRAY::b|:c");
    /// event.unshift_header("X-Test", "a");
    /// assert_eq!(event.header_str("X-Test"), Some("ARRAY::a|:b|:c"));
    /// ```
    pub fn unshift_header(&mut self, name: &str, value: &str) {
        self.stack_header(name, value, EslArray::unshift);
    }

    fn stack_header(&mut self, name: &str, value: &str, op: fn(&mut EslArray, String)) {
        match self
            .headers
            .get(name)
        {
            None => {
                self.set_header(name, value);
            }
            Some(existing) => {
                let mut arr = match EslArray::parse(existing) {
                    Some(arr) => arr,
                    None => EslArray::new(vec![existing.clone()]),
                };
                op(&mut arr, value.into());
                self.set_header(name, arr.to_string());
            }
        }
    }

    /// Check whether this event matches the given type.
    pub fn is_event_type(&self, event_type: EslEventType) -> bool {
        self.event_type == Some(event_type)
    }

    /// Serialize to ESL plain text wire format with percent-encoded header values.
    ///
    /// This is the inverse of `EslParser::parse_plain_event()`. The output can
    /// be fed back through the parser to reconstruct an equivalent `EslEvent`
    /// (round-trip).
    ///
    /// Headers are emitted in insertion order (which matches wire order when the
    /// event was parsed from the network). `Content-Length` from stored headers
    /// is skipped and recomputed from the body if present.
    pub fn to_plain_format(&self) -> String {
        use std::fmt::Write;
        let mut result = String::new();

        for (key, value) in &self.headers {
            if key == "Content-Length" {
                continue;
            }
            let _ = writeln!(
                result,
                "{}: {}",
                key,
                percent_encode(value.as_bytes(), NON_ALPHANUMERIC)
            );
        }

        if let Some(body) = &self.body {
            let _ = writeln!(result, "Content-Length: {}", body.len());
            result.push('\n');
            result.push_str(body);
        } else {
            result.push('\n');
        }

        result
    }
}

impl Default for EslEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl HeaderLookup for EslEvent {
    fn header_str(&self, name: &str) -> Option<&str> {
        EslEvent::header_str(self, name)
    }

    fn variable_str(&self, name: &str) -> Option<&str> {
        let key = format!("variable_{}", name);
        self.header_str(&key)
    }
}

impl PartialEq for EslEvent {
    fn eq(&self, other: &Self) -> bool {
        self.event_type == other.event_type
            && self.headers == other.headers
            && self.body == other.body
    }
}

impl std::hash::Hash for EslEvent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.event_type
            .hash(state);
        for (k, v) in &self.headers {
            k.hash(state);
            v.hash(state);
        }
        self.body
            .hash(state);
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for EslEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            event_type: Option<EslEventType>,
            headers: IndexMap<String, String>,
            body: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut event = EslEvent::new();
        event.event_type = raw.event_type;
        event.body = raw.body;
        for (k, v) in raw.headers {
            event.set_header(k, v);
        }
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_preserve_insertion_order() {
        let mut event = EslEvent::new();
        event.set_header("Zebra", "last");
        event.set_header("Alpha", "first");
        event.set_header("Middle", "mid");
        let keys: Vec<&str> = event
            .headers()
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(keys, vec!["Zebra", "Alpha", "Middle"]);
    }

    #[test]
    fn test_notify_in_parse() {
        assert_eq!(
            EslEventType::parse_event_type("NOTIFY_IN"),
            Some(EslEventType::NotifyIn)
        );
        assert_eq!(EslEventType::parse_event_type("notify_in"), None);
    }

    #[test]
    fn test_notify_in_display() {
        assert_eq!(EslEventType::NotifyIn.to_string(), "NOTIFY_IN");
    }

    #[test]
    fn test_notify_in_distinct_from_notify() {
        assert_ne!(EslEventType::Notify, EslEventType::NotifyIn);
        assert_ne!(
            EslEventType::Notify.to_string(),
            EslEventType::NotifyIn.to_string()
        );
    }

    #[test]
    fn test_wire_names_match_c_esl() {
        assert_eq!(
            EslEventType::ChannelOutgoing.to_string(),
            "CHANNEL_OUTGOING"
        );
        assert_eq!(EslEventType::Api.to_string(), "API");
        assert_eq!(EslEventType::ReloadXml.to_string(), "RELOADXML");
        assert_eq!(EslEventType::PresenceIn.to_string(), "PRESENCE_IN");
        assert_eq!(EslEventType::Roster.to_string(), "ROSTER");
        assert_eq!(EslEventType::Text.to_string(), "TEXT");
        assert_eq!(EslEventType::ReSchedule.to_string(), "RE_SCHEDULE");

        assert_eq!(
            EslEventType::parse_event_type("CHANNEL_OUTGOING"),
            Some(EslEventType::ChannelOutgoing)
        );
        assert_eq!(
            EslEventType::parse_event_type("API"),
            Some(EslEventType::Api)
        );
        assert_eq!(
            EslEventType::parse_event_type("RELOADXML"),
            Some(EslEventType::ReloadXml)
        );
        assert_eq!(
            EslEventType::parse_event_type("PRESENCE_IN"),
            Some(EslEventType::PresenceIn)
        );
    }

    #[test]
    fn test_event_type_from_str() {
        assert_eq!(
            "CHANNEL_ANSWER".parse::<EslEventType>(),
            Ok(EslEventType::ChannelAnswer)
        );
        assert!("channel_answer"
            .parse::<EslEventType>()
            .is_err());
        assert!("UNKNOWN_EVENT"
            .parse::<EslEventType>()
            .is_err());
    }

    #[test]
    fn test_remove_header() {
        let mut event = EslEvent::new();
        event.set_header("Foo", "bar");
        event.set_header("Baz", "qux");

        let removed = event.remove_header("Foo");
        assert_eq!(removed, Some("bar".to_string()));
        assert!(event
            .header_str("Foo")
            .is_none());
        assert_eq!(event.header_str("Baz"), Some("qux"));

        let removed_again = event.remove_header("Foo");
        assert_eq!(removed_again, None);
    }

    #[test]
    fn test_to_plain_format_basic() {
        let mut event = EslEvent::with_type(EslEventType::Heartbeat);
        event.set_header("Event-Name", "HEARTBEAT");
        event.set_header("Core-UUID", "abc-123");

        let plain = event.to_plain_format();

        assert!(plain.starts_with("Event-Name: "));
        assert!(plain.contains("Core-UUID: "));
        assert!(plain.ends_with("\n\n"));
    }

    #[test]
    fn test_to_plain_format_percent_encoding() {
        let mut event = EslEvent::with_type(EslEventType::Heartbeat);
        event.set_header("Event-Name", "HEARTBEAT");
        event.set_header("Up-Time", "0 years, 0 days");

        let plain = event.to_plain_format();

        assert!(!plain.contains("0 years, 0 days"));
        assert!(plain.contains("Up-Time: "));
        assert!(plain.contains("%20"));
    }

    #[test]
    fn test_to_plain_format_with_body() {
        let mut event = EslEvent::with_type(EslEventType::BackgroundJob);
        event.set_header("Event-Name", "BACKGROUND_JOB");
        event.set_header("Job-UUID", "def-456");
        event.set_body("+OK result\n".to_string());

        let plain = event.to_plain_format();

        assert!(plain.contains("Content-Length: 11\n"));
        assert!(plain.ends_with("\n\n+OK result\n"));
    }

    #[test]
    fn test_to_plain_format_preserves_insertion_order() {
        let mut event = EslEvent::with_type(EslEventType::Heartbeat);
        event.set_header("Event-Name", "HEARTBEAT");
        event.set_header("Core-UUID", "abc-123");
        event.set_header("FreeSWITCH-Hostname", "fs01");
        event.set_header("Up-Time", "0 years, 1 day");

        let plain = event.to_plain_format();
        let lines: Vec<&str> = plain
            .lines()
            .collect();
        assert!(lines[0].starts_with("Event-Name: "));
        assert!(lines[1].starts_with("Core-UUID: "));
        assert!(lines[2].starts_with("FreeSWITCH-Hostname: "));
        assert!(lines[3].starts_with("Up-Time: "));
    }

    #[test]
    fn test_to_plain_format_round_trip() {
        let mut original = EslEvent::with_type(EslEventType::ChannelCreate);
        original.set_header("Event-Name", "CHANNEL_CREATE");
        original.set_header("Core-UUID", "abc-123");
        original.set_header("Channel-Name", "sofia/internal/1000@example.com");
        original.set_header("Caller-Caller-ID-Name", "Jérôme Poulin");
        original.set_body("some body content");

        let plain = original.to_plain_format();

        // Simulate what EslParser::parse_plain_event does
        let (header_section, inner_body) = if let Some(pos) = plain.find("\n\n") {
            (&plain[..pos], Some(&plain[pos + 2..]))
        } else {
            (plain.as_str(), None)
        };

        let mut parsed = EslEvent::new();
        for line in header_section.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim();
                if key == "Content-Length" {
                    continue;
                }
                let raw_value = line[colon_pos + 1..].trim();
                let value = percent_encoding::percent_decode_str(raw_value)
                    .decode_utf8()
                    .unwrap()
                    .into_owned();
                parsed.set_header(key, value);
            }
        }
        if let Some(ib) = inner_body {
            if !ib.is_empty() {
                parsed.set_body(ib);
            }
        }

        assert_eq!(original.headers(), parsed.headers());
        assert_eq!(original.body(), parsed.body());
    }

    #[test]
    fn test_set_priority_normal() {
        let mut event = EslEvent::new();
        event.set_priority(EslEventPriority::Normal);
        assert_eq!(
            event
                .priority()
                .unwrap(),
            Some(EslEventPriority::Normal)
        );
        assert_eq!(event.header(EventHeader::Priority), Some("NORMAL"));
    }

    #[test]
    fn test_set_priority_high() {
        let mut event = EslEvent::new();
        event.set_priority(EslEventPriority::High);
        assert_eq!(
            event
                .priority()
                .unwrap(),
            Some(EslEventPriority::High)
        );
        assert_eq!(event.header(EventHeader::Priority), Some("HIGH"));
    }

    #[test]
    fn test_priority_display() {
        assert_eq!(EslEventPriority::Normal.to_string(), "NORMAL");
        assert_eq!(EslEventPriority::Low.to_string(), "LOW");
        assert_eq!(EslEventPriority::High.to_string(), "HIGH");
    }

    #[test]
    fn test_priority_from_str() {
        assert_eq!(
            "NORMAL".parse::<EslEventPriority>(),
            Ok(EslEventPriority::Normal)
        );
        assert_eq!("LOW".parse::<EslEventPriority>(), Ok(EslEventPriority::Low));
        assert_eq!(
            "HIGH".parse::<EslEventPriority>(),
            Ok(EslEventPriority::High)
        );
        assert!("INVALID"
            .parse::<EslEventPriority>()
            .is_err());
    }

    #[test]
    fn test_priority_from_str_rejects_wrong_case() {
        assert!("normal"
            .parse::<EslEventPriority>()
            .is_err());
        assert!("Low"
            .parse::<EslEventPriority>()
            .is_err());
        assert!("hIgH"
            .parse::<EslEventPriority>()
            .is_err());
    }

    #[test]
    fn test_push_header_new() {
        let mut event = EslEvent::new();
        event.push_header("X-Test", "first");
        assert_eq!(event.header_str("X-Test"), Some("first"));
    }

    #[test]
    fn test_push_header_existing_plain() {
        let mut event = EslEvent::new();
        event.set_header("X-Test", "first");
        event.push_header("X-Test", "second");
        assert_eq!(event.header_str("X-Test"), Some("ARRAY::first|:second"));
    }

    #[test]
    fn test_push_header_existing_array() {
        let mut event = EslEvent::new();
        event.set_header("X-Test", "ARRAY::a|:b");
        event.push_header("X-Test", "c");
        assert_eq!(event.header_str("X-Test"), Some("ARRAY::a|:b|:c"));
    }

    #[test]
    fn test_unshift_header_new() {
        let mut event = EslEvent::new();
        event.unshift_header("X-Test", "only");
        assert_eq!(event.header_str("X-Test"), Some("only"));
    }

    #[test]
    fn test_unshift_header_existing_array() {
        let mut event = EslEvent::new();
        event.set_header("X-Test", "ARRAY::b|:c");
        event.unshift_header("X-Test", "a");
        assert_eq!(event.header_str("X-Test"), Some("ARRAY::a|:b|:c"));
    }

    #[test]
    fn test_sendevent_with_priority_wire_format() {
        let mut event = EslEvent::with_type(EslEventType::Custom);
        event.set_header("Event-Name", "CUSTOM");
        event.set_header("Event-Subclass", "test::priority");
        event.set_priority(EslEventPriority::High);

        let plain = event.to_plain_format();
        assert!(plain.contains("priority: HIGH\n"));
    }

    #[test]
    fn test_convenience_accessors() {
        let mut event = EslEvent::new();
        event.set_header("Channel-Name", "sofia/internal/1000@example.com");
        event.set_header("Caller-Caller-ID-Number", "1000");
        event.set_header("Caller-Caller-ID-Name", "Alice");
        event.set_header("Hangup-Cause", "NORMAL_CLEARING");
        event.set_header("Event-Subclass", "sofia::register");
        event.set_header("variable_sip_from_display", "Bob");

        assert_eq!(
            event.channel_name(),
            Some("sofia/internal/1000@example.com")
        );
        assert_eq!(event.caller_id_number(), Some("1000"));
        assert_eq!(event.caller_id_name(), Some("Alice"));
        assert_eq!(
            event
                .hangup_cause()
                .unwrap(),
            Some(crate::channel::HangupCause::NormalClearing)
        );
        assert_eq!(event.event_subclass(), Some("sofia::register"));
        assert_eq!(event.variable_str("sip_from_display"), Some("Bob"));
        assert_eq!(event.variable_str("nonexistent"), None);
    }

    #[test]
    fn test_event_format_from_str() {
        assert_eq!("plain".parse::<EventFormat>(), Ok(EventFormat::Plain));
        assert_eq!("json".parse::<EventFormat>(), Ok(EventFormat::Json));
        assert_eq!("xml".parse::<EventFormat>(), Ok(EventFormat::Xml));
        assert!("foo"
            .parse::<EventFormat>()
            .is_err());
    }

    #[test]
    fn test_event_format_from_str_case_insensitive() {
        assert_eq!("PLAIN".parse::<EventFormat>(), Ok(EventFormat::Plain));
        assert_eq!("Json".parse::<EventFormat>(), Ok(EventFormat::Json));
        assert_eq!("XML".parse::<EventFormat>(), Ok(EventFormat::Xml));
        assert_eq!("Xml".parse::<EventFormat>(), Ok(EventFormat::Xml));
    }

    #[test]
    fn test_event_format_from_content_type() {
        assert_eq!(
            EventFormat::from_content_type("text/event-json"),
            Ok(EventFormat::Json)
        );
        assert_eq!(
            EventFormat::from_content_type("text/event-xml"),
            Ok(EventFormat::Xml)
        );
        assert_eq!(
            EventFormat::from_content_type("text/event-plain"),
            Ok(EventFormat::Plain)
        );
        assert!(EventFormat::from_content_type("unknown").is_err());
    }

    // --- EslEvent accessor tests (via HeaderLookup trait) ---

    #[test]
    fn test_event_channel_state_accessor() {
        use crate::channel::ChannelState;
        let mut event = EslEvent::new();
        event.set_header("Channel-State", "CS_EXECUTE");
        assert_eq!(
            event
                .channel_state()
                .unwrap(),
            Some(ChannelState::CsExecute)
        );
    }

    #[test]
    fn test_event_channel_state_number_accessor() {
        use crate::channel::ChannelState;
        let mut event = EslEvent::new();
        event.set_header("Channel-State-Number", "4");
        assert_eq!(
            event
                .channel_state_number()
                .unwrap(),
            Some(ChannelState::CsExecute)
        );
    }

    #[test]
    fn test_event_call_state_accessor() {
        use crate::channel::CallState;
        let mut event = EslEvent::new();
        event.set_header("Channel-Call-State", "ACTIVE");
        assert_eq!(
            event
                .call_state()
                .unwrap(),
            Some(CallState::Active)
        );
    }

    #[test]
    fn test_event_answer_state_accessor() {
        use crate::channel::AnswerState;
        let mut event = EslEvent::new();
        event.set_header("Answer-State", "answered");
        assert_eq!(
            event
                .answer_state()
                .unwrap(),
            Some(AnswerState::Answered)
        );
    }

    #[test]
    fn test_event_call_direction_accessor() {
        use crate::channel::CallDirection;
        let mut event = EslEvent::new();
        event.set_header("Call-Direction", "inbound");
        assert_eq!(
            event
                .call_direction()
                .unwrap(),
            Some(CallDirection::Inbound)
        );
    }

    #[test]
    fn test_event_typed_accessors_missing_headers() {
        let event = EslEvent::new();
        assert_eq!(
            event
                .channel_state()
                .unwrap(),
            None
        );
        assert_eq!(
            event
                .channel_state_number()
                .unwrap(),
            None
        );
        assert_eq!(
            event
                .call_state()
                .unwrap(),
            None
        );
        assert_eq!(
            event
                .answer_state()
                .unwrap(),
            None
        );
        assert_eq!(
            event
                .call_direction()
                .unwrap(),
            None
        );
    }

    // --- Repeating SIP header tests ---

    #[test]
    fn test_sip_p_asserted_identity_comma_separated() {
        let mut event = EslEvent::new();
        // RFC 3325: P-Asserted-Identity can carry two identities (one sip:, one tel:)
        // FreeSWITCH stores the comma-separated value as a single channel variable
        event.set_header(
            "variable_sip_P-Asserted-Identity",
            "<sip:alice@atlanta.example.com>, <tel:+15551234567>",
        );

        assert_eq!(
            event.variable_str("sip_P-Asserted-Identity"),
            Some("<sip:alice@atlanta.example.com>, <tel:+15551234567>")
        );
    }

    #[test]
    fn test_sip_p_asserted_identity_array_format() {
        let mut event = EslEvent::new();
        // When FreeSWITCH stores repeated SIP headers via ARRAY format
        event.push_header(
            "variable_sip_P-Asserted-Identity",
            "<sip:alice@atlanta.example.com>",
        );
        event.push_header("variable_sip_P-Asserted-Identity", "<tel:+15551234567>");

        let raw = event
            .header_str("variable_sip_P-Asserted-Identity")
            .unwrap();
        assert_eq!(
            raw,
            "ARRAY::<sip:alice@atlanta.example.com>|:<tel:+15551234567>"
        );

        let arr = crate::variables::EslArray::parse(raw).unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr.items()[0], "<sip:alice@atlanta.example.com>");
        assert_eq!(arr.items()[1], "<tel:+15551234567>");
    }

    #[test]
    fn test_sip_header_with_colons_in_uri() {
        let mut event = EslEvent::new();
        // SIP URIs contain colons (sip:, sips:) which must not confuse ARRAY parsing
        event.push_header(
            "variable_sip_h_Diversion",
            "<sip:+15551234567@gw.example.com;reason=unconditional>",
        );
        event.push_header(
            "variable_sip_h_Diversion",
            "<sips:+15559876543@secure.example.com;reason=no-answer;counter=3>",
        );

        let raw = event
            .header_str("variable_sip_h_Diversion")
            .unwrap();
        let arr = crate::variables::EslArray::parse(raw).unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(
            arr.items()[0],
            "<sip:+15551234567@gw.example.com;reason=unconditional>"
        );
        assert_eq!(
            arr.items()[1],
            "<sips:+15559876543@secure.example.com;reason=no-answer;counter=3>"
        );
    }

    #[test]
    fn test_sip_p_asserted_identity_plain_format_round_trip() {
        let mut event = EslEvent::with_type(EslEventType::ChannelCreate);
        event.set_header("Event-Name", "CHANNEL_CREATE");
        event.set_header(
            "variable_sip_P-Asserted-Identity",
            "<sip:alice@atlanta.example.com>, <tel:+15551234567>",
        );

        let plain = event.to_plain_format();
        // The comma-separated value should be percent-encoded on the wire
        assert!(plain.contains("variable_sip_P-Asserted-Identity:"));
        // Angle brackets and comma should be encoded
        assert!(!plain.contains("<sip:alice"));
    }

    // --- Header key normalization on EslEvent ---
    // set_header() normalizes keys so lookups via header(EventHeader::X)
    // and header_str() work regardless of the casing used at insertion.

    #[test]
    fn set_header_normalizes_known_enum_variant() {
        let mut event = EslEvent::new();
        event.set_header("unique-id", "abc-123");
        assert_eq!(event.header(EventHeader::UniqueId), Some("abc-123"));
    }

    #[test]
    fn set_header_normalizes_codec_header() {
        let mut event = EslEvent::new();
        event.set_header("channel-read-codec-bit-rate", "128000");
        assert_eq!(
            event.header(EventHeader::ChannelReadCodecBitRate),
            Some("128000")
        );
    }

    #[test]
    fn header_str_finds_by_original_key() {
        let mut event = EslEvent::new();
        event.set_header("unique-id", "abc-123");
        // Lookup by original non-canonical key should still work
        assert_eq!(event.header_str("unique-id"), Some("abc-123"));
        // Lookup by canonical key also works
        assert_eq!(event.header_str("Unique-ID"), Some("abc-123"));
    }

    #[test]
    fn header_str_finds_unknown_dash_header_by_original() {
        let mut event = EslEvent::new();
        event.set_header("x-custom-header", "val");
        // Stored as Title-Case
        assert_eq!(event.header_str("X-Custom-Header"), Some("val"));
        // Original key also works via alias
        assert_eq!(event.header_str("x-custom-header"), Some("val"));
    }

    #[test]
    fn set_header_underscore_passthrough_preserves_sip_h() {
        let mut event = EslEvent::new();
        event.set_header("variable_sip_h_X-My-CUSTOM-Header", "val");
        assert_eq!(
            event.header_str("variable_sip_h_X-My-CUSTOM-Header"),
            Some("val")
        );
    }

    #[test]
    fn set_header_different_casing_overwrites() {
        let mut event = EslEvent::new();
        event.set_header("Unique-ID", "first");
        event.set_header("unique-id", "second");
        // Both normalize to "Unique-ID", second overwrites first
        assert_eq!(event.header(EventHeader::UniqueId), Some("second"));
    }

    #[test]
    fn remove_header_by_original_key() {
        let mut event = EslEvent::new();
        event.set_header("unique-id", "abc-123");
        let removed = event.remove_header("unique-id");
        assert_eq!(removed, Some("abc-123".to_string()));
        assert_eq!(event.header(EventHeader::UniqueId), None);
    }

    #[test]
    fn remove_header_by_canonical_key() {
        let mut event = EslEvent::new();
        event.set_header("unique-id", "abc-123");
        let removed = event.remove_header("Unique-ID");
        assert_eq!(removed, Some("abc-123".to_string()));
        assert_eq!(event.header_str("unique-id"), None);
    }

    #[test]
    fn serde_round_trip_preserves_canonical_lookups() {
        let mut event = EslEvent::new();
        event.set_header("unique-id", "abc-123");
        event.set_header("channel-read-codec-bit-rate", "128000");
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: EslEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.header(EventHeader::UniqueId), Some("abc-123"));
        assert_eq!(
            deserialized.header(EventHeader::ChannelReadCodecBitRate),
            Some("128000")
        );
    }

    #[test]
    fn serde_deserialize_normalizes_external_json() {
        let json = r#"{"event_type":null,"headers":{"unique-id":"abc-123","channel-read-codec-bit-rate":"128000"},"body":null}"#;
        let event: EslEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.header(EventHeader::UniqueId), Some("abc-123"));
        assert_eq!(
            event.header(EventHeader::ChannelReadCodecBitRate),
            Some("128000")
        );
        assert_eq!(event.header_str("unique-id"), Some("abc-123"));
    }

    #[test]
    fn test_event_typed_accessors_invalid_values() {
        let mut event = EslEvent::new();
        event.set_header("Channel-State", "BOGUS");
        event.set_header("Channel-State-Number", "999");
        event.set_header("Channel-Call-State", "BOGUS");
        event.set_header("Answer-State", "bogus");
        event.set_header("Call-Direction", "bogus");
        assert!(event
            .channel_state()
            .is_err());
        assert!(event
            .channel_state_number()
            .is_err());
        assert!(event
            .call_state()
            .is_err());
        assert!(event
            .answer_state()
            .is_err());
        assert!(event
            .call_direction()
            .is_err());
    }

    // --- EventSubscription tests ---

    #[test]
    fn new_creates_empty() {
        let sub = EventSubscription::new(EventFormat::Plain);
        assert!(sub.is_empty());
        assert!(!sub.is_all());
        assert_eq!(sub.format(), EventFormat::Plain);
        assert!(sub
            .event_types()
            .is_empty());
        assert!(sub
            .custom_subclass_list()
            .is_empty());
        assert!(sub
            .filters()
            .is_empty());
    }

    #[test]
    fn all_creates_all() {
        let sub = EventSubscription::all(EventFormat::Json);
        assert!(sub.is_all());
        assert!(!sub.is_empty());
        assert_eq!(sub.to_event_string(), Some("ALL".to_string()));
    }

    #[test]
    fn event_string_typed_only() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .event(EslEventType::ChannelCreate)
            .event(EslEventType::ChannelAnswer);
        assert_eq!(
            sub.to_event_string(),
            Some("CHANNEL_CREATE CHANNEL_ANSWER".to_string())
        );
    }

    #[test]
    fn event_string_custom_only() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .custom_subclass("sofia::register")
            .unwrap()
            .custom_subclass("sofia::unregister")
            .unwrap();
        assert_eq!(
            sub.to_event_string(),
            Some("CUSTOM sofia::register sofia::unregister".to_string())
        );
    }

    #[test]
    fn event_string_mixed() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .event(EslEventType::Heartbeat)
            .custom_subclass("sofia::register")
            .unwrap();
        assert_eq!(
            sub.to_event_string(),
            Some("HEARTBEAT CUSTOM sofia::register".to_string())
        );
    }

    #[test]
    fn event_string_custom_not_duplicated() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .event(EslEventType::Custom)
            .custom_subclass("sofia::register")
            .unwrap();
        // Should not have "CUSTOM" twice
        assert_eq!(
            sub.to_event_string(),
            Some("CUSTOM sofia::register".to_string())
        );
    }

    #[test]
    fn event_string_empty_is_none() {
        let sub = EventSubscription::new(EventFormat::Plain);
        assert_eq!(sub.to_event_string(), None);
    }

    #[test]
    fn filters_preserve_order() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .filter(EventHeader::CallDirection, "inbound")
            .unwrap()
            .filter_raw("X-Custom", "value1")
            .unwrap()
            .filter(EventHeader::ChannelState, "CS_EXECUTE")
            .unwrap();
        assert_eq!(
            sub.filters(),
            &[
                ("Call-Direction".to_string(), "inbound".to_string()),
                ("X-Custom".to_string(), "value1".to_string()),
                ("Channel-State".to_string(), "CS_EXECUTE".to_string()),
            ]
        );
    }

    #[test]
    fn builder_chain() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .events(EslEventType::CHANNEL_EVENTS)
            .event(EslEventType::Heartbeat)
            .custom_subclass("sofia::register")
            .unwrap()
            .filter(EventHeader::CallDirection, "inbound")
            .unwrap()
            .with_format(EventFormat::Json);

        assert_eq!(sub.format(), EventFormat::Json);
        assert!(!sub.is_empty());
        assert!(!sub.is_all());
        assert!(sub
            .event_types()
            .contains(&EslEventType::ChannelCreate));
        assert!(sub
            .event_types()
            .contains(&EslEventType::Heartbeat));
        assert_eq!(sub.custom_subclass_list(), &["sofia::register"]);
        assert_eq!(
            sub.filters()
                .len(),
            1
        );
    }

    #[test]
    fn serde_round_trip_subscription() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .event(EslEventType::ChannelCreate)
            .event(EslEventType::Heartbeat)
            .custom_subclass("sofia::register")
            .unwrap()
            .filter(EventHeader::CallDirection, "inbound")
            .unwrap();

        let json = serde_json::to_string(&sub).unwrap();
        let deserialized: EventSubscription = serde_json::from_str(&json).unwrap();
        assert_eq!(sub, deserialized);
    }

    #[test]
    fn serde_rejects_invalid_subclass() {
        let json =
            r#"{"format":"Plain","events":[],"custom_subclasses":["bad subclass"],"filters":[]}"#;
        let result: Result<EventSubscription, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result
            .unwrap_err()
            .to_string();
        assert!(err.contains("space"), "error should mention space: {err}");
    }

    #[test]
    fn serde_rejects_newline_in_filter() {
        let json = r#"{"format":"Plain","events":[],"custom_subclasses":[],"filters":[["Header","val\n"]]}"#;
        let result: Result<EventSubscription, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("newline"),
            "error should mention newline: {err}"
        );
    }

    #[test]
    fn custom_subclass_rejects_space() {
        let result = EventSubscription::new(EventFormat::Plain).custom_subclass("bad subclass");
        assert!(result.is_err());
    }

    #[test]
    fn custom_subclass_rejects_newline() {
        let result = EventSubscription::new(EventFormat::Plain).custom_subclass("bad\nsubclass");
        assert!(result.is_err());
    }

    #[test]
    fn custom_subclass_rejects_empty() {
        let result = EventSubscription::new(EventFormat::Plain).custom_subclass("");
        assert!(result.is_err());
    }

    #[test]
    fn filter_raw_rejects_newline_in_header() {
        let result = EventSubscription::new(EventFormat::Plain).filter_raw("Bad\nHeader", "value");
        assert!(result.is_err());
    }

    #[test]
    fn filter_raw_rejects_newline_in_value() {
        let result = EventSubscription::new(EventFormat::Plain).filter_raw("Header", "bad\nvalue");
        assert!(result.is_err());
    }

    #[test]
    fn filter_typed_rejects_newline_in_value() {
        let result = EventSubscription::new(EventFormat::Plain)
            .filter(EventHeader::CallDirection, "bad\nvalue");
        assert!(result.is_err());
    }

    #[test]
    fn sofia_event_single() {
        let sub =
            EventSubscription::new(EventFormat::Plain).sofia_event(SofiaEventSubclass::Register);
        assert_eq!(
            sub.to_event_string(),
            Some("CUSTOM sofia::register".to_string())
        );
    }

    #[test]
    fn sofia_events_group() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .sofia_events(SofiaEventSubclass::GATEWAY_EVENTS);
        let event_str = sub
            .to_event_string()
            .unwrap();
        assert!(event_str.starts_with("CUSTOM"));
        assert!(event_str.contains("sofia::gateway_state"));
        assert!(event_str.contains("sofia::gateway_add"));
        assert!(event_str.contains("sofia::gateway_delete"));
        assert!(event_str.contains("sofia::gateway_invalid_digest_req"));
    }

    #[test]
    fn sofia_event_mixed_with_typed_events() {
        let sub = EventSubscription::new(EventFormat::Plain)
            .event(EslEventType::Heartbeat)
            .sofia_event(SofiaEventSubclass::GatewayState);
        assert_eq!(
            sub.to_event_string(),
            Some("HEARTBEAT CUSTOM sofia::gateway_state".to_string())
        );
    }
}
