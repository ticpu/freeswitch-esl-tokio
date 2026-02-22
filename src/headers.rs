//! Typed event header names for FreeSWITCH ESL events.

/// Error returned when parsing an unrecognized event header name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEventHeaderError(pub String);

impl std::fmt::Display for ParseEventHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown event header: {}", self.0)
    }
}

impl std::error::Error for ParseEventHeaderError {}

define_header_enum! {
    error_type: ParseEventHeaderError,
    /// Top-level header names that appear in FreeSWITCH ESL events.
    ///
    /// These are the headers on the parsed event itself (not protocol framing
    /// headers like `Content-Type`). Use with [`EslEvent::header()`] for
    /// type-safe lookups.
    pub enum EventHeader {
        EventName => "Event-Name",
        EventSubclass => "Event-Subclass",
        UniqueId => "Unique-ID",
        CallerUniqueId => "Caller-Unique-ID",
        OtherLegUniqueId => "Other-Leg-Unique-ID",
        ChannelCallUuid => "Channel-Call-UUID",
        JobUuid => "Job-UUID",
        ChannelName => "Channel-Name",
        ChannelState => "Channel-State",
        ChannelStateNumber => "Channel-State-Number",
        ChannelCallState => "Channel-Call-State",
        AnswerState => "Answer-State",
        CallDirection => "Call-Direction",
        HangupCause => "Hangup-Cause",
        CallerCallerIdName => "Caller-Caller-ID-Name",
        CallerCallerIdNumber => "Caller-Caller-ID-Number",
        CallerOrigCallerIdName => "Caller-Orig-Caller-ID-Name",
        CallerOrigCallerIdNumber => "Caller-Orig-Caller-ID-Number",
        CallerCalleeIdName => "Caller-Callee-ID-Name",
        CallerCalleeIdNumber => "Caller-Callee-ID-Number",
        CallerDestinationNumber => "Caller-Destination-Number",
        CallerContext => "Caller-Context",
        CallerDirection => "Caller-Direction",
        CallerNetworkAddr => "Caller-Network-Addr",
        CoreUuid => "Core-UUID",
        Priority => "priority",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trip() {
        assert_eq!(EventHeader::UniqueId.to_string(), "Unique-ID");
        assert_eq!(
            EventHeader::ChannelCallState.to_string(),
            "Channel-Call-State"
        );
        assert_eq!(
            EventHeader::CallerCallerIdName.to_string(),
            "Caller-Caller-ID-Name"
        );
        assert_eq!(EventHeader::Priority.to_string(), "priority");
    }

    #[test]
    fn as_ref_str() {
        let h: &str = EventHeader::UniqueId.as_ref();
        assert_eq!(h, "Unique-ID");
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!(
            "unique-id".parse::<EventHeader>(),
            Ok(EventHeader::UniqueId)
        );
        assert_eq!(
            "UNIQUE-ID".parse::<EventHeader>(),
            Ok(EventHeader::UniqueId)
        );
        assert_eq!(
            "Unique-ID".parse::<EventHeader>(),
            Ok(EventHeader::UniqueId)
        );
        assert_eq!(
            "channel-call-state".parse::<EventHeader>(),
            Ok(EventHeader::ChannelCallState)
        );
    }

    #[test]
    fn from_str_unknown() {
        let err = "X-Custom-Not-In-Enum".parse::<EventHeader>();
        assert!(err.is_err());
        assert_eq!(
            err.unwrap_err()
                .to_string(),
            "unknown event header: X-Custom-Not-In-Enum"
        );
    }

    #[test]
    fn from_str_round_trip_all_variants() {
        let variants = [
            EventHeader::EventName,
            EventHeader::EventSubclass,
            EventHeader::UniqueId,
            EventHeader::CallerUniqueId,
            EventHeader::OtherLegUniqueId,
            EventHeader::ChannelCallUuid,
            EventHeader::JobUuid,
            EventHeader::ChannelName,
            EventHeader::ChannelState,
            EventHeader::ChannelStateNumber,
            EventHeader::ChannelCallState,
            EventHeader::AnswerState,
            EventHeader::CallDirection,
            EventHeader::HangupCause,
            EventHeader::CallerCallerIdName,
            EventHeader::CallerCallerIdNumber,
            EventHeader::CallerOrigCallerIdName,
            EventHeader::CallerOrigCallerIdNumber,
            EventHeader::CallerCalleeIdName,
            EventHeader::CallerCalleeIdNumber,
            EventHeader::CallerDestinationNumber,
            EventHeader::CallerContext,
            EventHeader::CallerDirection,
            EventHeader::CallerNetworkAddr,
            EventHeader::CoreUuid,
            EventHeader::Priority,
        ];
        for v in variants {
            let wire = v.to_string();
            let parsed: EventHeader = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, v, "round-trip failed for {wire}");
        }
    }
}
