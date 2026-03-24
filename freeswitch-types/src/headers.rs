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

sip_header::define_header_enum! {
    error_type: ParseEventHeaderError,
    /// Top-level header names that appear in FreeSWITCH ESL events.
    ///
    /// These are the headers on the parsed event itself (not protocol framing
    /// headers like `Content-Type`). Use with [`EslEvent::header()`](crate::EslEvent::header) for
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
        DtmfDigit => "DTMF-Digit",
        Priority => "priority",
        LogLevel => "Log-Level",
        /// SIP NOTIFY body content (JSON payload from `NOTIFY_IN` events).
        PlData => "pl_data",
        /// SIP event package name from `NOTIFY_IN` events (e.g. `emergency-AbandonedCall`).
        SipEvent => "event",
        /// SIP content type from `NOTIFY_IN` events.
        SipContentType => "sip_content_type",
        /// Gateway that received the SIP NOTIFY.
        GatewayName => "gateway_name",

        // --- Codec (from switch_channel_event_set_data / switch_core_codec.c) ---
        // Audio read
        ChannelReadCodecName => "Channel-Read-Codec-Name",
        ChannelReadCodecRate => "Channel-Read-Codec-Rate",
        ChannelReadCodecBitRate => "Channel-Read-Codec-Bit-Rate",
        /// Only present when actual_samples_per_second != samples_per_second.
        ChannelReportedReadCodecRate => "Channel-Reported-Read-Codec-Rate",
        // Audio write
        ChannelWriteCodecName => "Channel-Write-Codec-Name",
        ChannelWriteCodecRate => "Channel-Write-Codec-Rate",
        ChannelWriteCodecBitRate => "Channel-Write-Codec-Bit-Rate",
        /// Only present when actual_samples_per_second != samples_per_second.
        ChannelReportedWriteCodecRate => "Channel-Reported-Write-Codec-Rate",
        // Video read/write
        ChannelVideoReadCodecName => "Channel-Video-Read-Codec-Name",
        ChannelVideoReadCodecRate => "Channel-Video-Read-Codec-Rate",
        ChannelVideoWriteCodecName => "Channel-Video-Write-Codec-Name",
        ChannelVideoWriteCodecRate => "Channel-Video-Write-Codec-Rate",
        /// Active session count from `HEARTBEAT` events.
        SessionCount => "Session-Count",
        FreeswitchHostname => "FreeSWITCH-Hostname",
        FreeswitchSwitchname => "FreeSWITCH-Switchname",
        FreeswitchIpv4 => "FreeSWITCH-IPv4",
        FreeswitchIpv6 => "FreeSWITCH-IPv6",
        FreeswitchVersion => "FreeSWITCH-Version",
        FreeswitchDomain => "FreeSWITCH-Domain",
        FreeswitchUser => "FreeSWITCH-User",

        // --- Application (from switch_core_session.c) ---
        Application => "Application",
        ApplicationData => "Application-Data",
        ApplicationResponse => "Application-Response",
        ApplicationUuid => "Application-UUID",
    }
}

/// Normalize a header key to its canonical form for case-insensitive storage.
///
/// FreeSWITCH's C ESL uses case-insensitive header lookups (`strcasecmp`), but
/// stores header names verbatim. Multiple C code paths emit the same logical
/// header with different casing (e.g. `switch_channel.c` sends `Unique-ID`
/// while `switch_event.c` sends `unique-id`). This function normalizes keys
/// so that both resolve to the same `HashMap` entry.
///
/// **Strategy:**
/// 1. Known [`EventHeader`] variants are matched first (case-insensitive) and
///    returned in their canonical wire form (e.g. `unique-id` → `Unique-ID`).
/// 2. Unknown keys containing underscores are returned **unchanged** -- these
///    are channel variables (`variable_*`) or `sip_h_*` passthrough headers
///    where the suffix preserves the original SIP header casing.
/// 3. Unknown dash-separated keys are Title-Cased to match FreeSWITCH's
///    dominant convention for event and framing headers.
pub fn normalize_header_key(raw: &str) -> String {
    if let Ok(eh) = raw.parse::<EventHeader>() {
        return eh
            .as_str()
            .to_string();
    }
    if raw.contains('_') {
        raw.to_string()
    } else {
        title_case_dashes(raw)
    }
}

fn title_case_dashes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '-' {
            result.push('-');
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c.to_ascii_lowercase());
        }
    }
    result
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

    // --- normalize_header_key tests ---
    // FreeSWITCH C ESL uses strcasecmp for header lookups but stores names
    // verbatim. Multiple C code paths emit the same logical header with
    // different casing (switch_channel.c Title-Case vs switch_event.c lowercase
    // vs switch_core_codec.c mixed). normalize_header_key canonicalizes keys
    // so they collapse to a single HashMap entry.

    #[test]
    fn normalize_known_enum_variants_return_canonical_form() {
        // EventHeader::from_str is case-insensitive; canonical as_str() is returned
        assert_eq!(normalize_header_key("unique-id"), "Unique-ID");
        assert_eq!(normalize_header_key("UNIQUE-ID"), "Unique-ID");
        assert_eq!(normalize_header_key("Unique-ID"), "Unique-ID");
        assert_eq!(normalize_header_key("dtmf-digit"), "DTMF-Digit");
        assert_eq!(normalize_header_key("DTMF-DIGIT"), "DTMF-Digit");
        assert_eq!(
            normalize_header_key("channel-call-uuid"),
            "Channel-Call-UUID"
        );
        assert_eq!(normalize_header_key("event-name"), "Event-Name");
    }

    #[test]
    fn normalize_known_underscore_variants_return_canonical_form() {
        // Headers whose canonical form contains underscores
        assert_eq!(normalize_header_key("priority"), "priority");
        assert_eq!(normalize_header_key("PRIORITY"), "priority");
        assert_eq!(normalize_header_key("pl_data"), "pl_data");
        assert_eq!(normalize_header_key("PL_DATA"), "pl_data");
        assert_eq!(normalize_header_key("sip_content_type"), "sip_content_type");
        assert_eq!(normalize_header_key("gateway_name"), "gateway_name");
        assert_eq!(normalize_header_key("event"), "event");
        assert_eq!(normalize_header_key("EVENT"), "event");
    }

    #[test]
    fn normalize_codec_headers_from_switch_core_codec() {
        // switch_core_codec.c sends lowercase, switch_channel_event_set_data sends Title-Case
        // Both must normalize to the canonical EventHeader form
        assert_eq!(
            normalize_header_key("channel-read-codec-bit-rate"),
            "Channel-Read-Codec-Bit-Rate"
        );
        assert_eq!(
            normalize_header_key("Channel-Read-Codec-Bit-Rate"),
            "Channel-Read-Codec-Bit-Rate"
        );
        // switch_core_codec.c mixed case for write: "Channel-Write-codec-bit-rate"
        assert_eq!(
            normalize_header_key("Channel-Write-codec-bit-rate"),
            "Channel-Write-Codec-Bit-Rate"
        );
        assert_eq!(
            normalize_header_key("channel-video-read-codec-name"),
            "Channel-Video-Read-Codec-Name"
        );
    }

    #[test]
    fn normalize_unknown_underscore_keys_passthrough() {
        // Channel variables and sip_h_* passthrough preserve original casing
        assert_eq!(
            normalize_header_key("variable_sip_call_id"),
            "variable_sip_call_id"
        );
        assert_eq!(
            normalize_header_key("variable_sip_h_X-My-CUSTOM-Header"),
            "variable_sip_h_X-My-CUSTOM-Header"
        );
        assert_eq!(
            normalize_header_key("variable_sip_h_Diversion"),
            "variable_sip_h_Diversion"
        );
    }

    #[test]
    fn normalize_unknown_dash_keys_title_case() {
        // Framing and unknown event headers get Title-Cased
        assert_eq!(normalize_header_key("content-type"), "Content-Type");
        assert_eq!(normalize_header_key("Content-Type"), "Content-Type");
        assert_eq!(normalize_header_key("CONTENT-TYPE"), "Content-Type");
        assert_eq!(normalize_header_key("x-custom-header"), "X-Custom-Header");
        assert_eq!(
            normalize_header_key("Content-Disposition"),
            "Content-Disposition"
        );
        assert_eq!(normalize_header_key("reply-text"), "Reply-Text");
    }

    #[test]
    fn normalize_idempotent_for_all_enum_variants() {
        // Normalizing an already-canonical wire string must return it unchanged
        let variants = [
            EventHeader::EventName,
            EventHeader::UniqueId,
            EventHeader::ChannelCallUuid,
            EventHeader::DtmfDigit,
            EventHeader::Priority,
            EventHeader::PlData,
            EventHeader::SipEvent,
            EventHeader::GatewayName,
            EventHeader::SipContentType,
            EventHeader::ChannelReadCodecBitRate,
            EventHeader::ChannelVideoWriteCodecRate,
            EventHeader::LogLevel,
        ];
        for v in variants {
            let canonical = v.as_str();
            assert_eq!(
                normalize_header_key(canonical),
                canonical,
                "normalization not idempotent for {canonical}"
            );
        }
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
            EventHeader::DtmfDigit,
            EventHeader::Priority,
            EventHeader::LogLevel,
            EventHeader::PlData,
            EventHeader::SipEvent,
            EventHeader::SipContentType,
            EventHeader::GatewayName,
            EventHeader::ChannelReadCodecName,
            EventHeader::ChannelReadCodecRate,
            EventHeader::ChannelReadCodecBitRate,
            EventHeader::ChannelReportedReadCodecRate,
            EventHeader::ChannelWriteCodecName,
            EventHeader::ChannelWriteCodecRate,
            EventHeader::ChannelWriteCodecBitRate,
            EventHeader::ChannelReportedWriteCodecRate,
            EventHeader::ChannelVideoReadCodecName,
            EventHeader::ChannelVideoReadCodecRate,
            EventHeader::ChannelVideoWriteCodecName,
            EventHeader::ChannelVideoWriteCodecRate,
            EventHeader::Application,
            EventHeader::ApplicationData,
            EventHeader::ApplicationResponse,
            EventHeader::ApplicationUuid,
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
