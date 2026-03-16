//! Raw SIP INVITE header variables (`sip_i_*`).
//!
//! These are set by `sofia_parse_all_invite_headers()` when
//! `parse-all-invite-headers` is enabled on the sofia profile.
//! Each variable contains the verbatim serialized SIP header value.
//!
//! Some headers may repeat in a SIP message. Those are stored using
//! FreeSWITCH's ARRAY format (`ARRAY::value1|:value2`). Parse them
//! with [`EslArray`](super::EslArray).

/// Error returned when parsing an unrecognized SIP invite header variable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSipInviteHeaderError(pub String);

impl std::fmt::Display for ParseSipInviteHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown SIP invite header variable: {}", self.0)
    }
}

impl std::error::Error for ParseSipInviteHeaderError {}

sip_header::define_header_enum! {
    error_type: ParseSipInviteHeaderError,
    /// Raw SIP INVITE headers preserved verbatim as channel variables.
    ///
    /// Set by `sofia_parse_all_invite_headers()` when the sofia profile has
    /// `parse-all-invite-headers` enabled. Access via
    /// [`HeaderLookup::variable()`](crate::HeaderLookup::variable).
    ///
    /// Variants marked "ARRAY" may contain multiple values in
    /// `ARRAY::val1|:val2` format when the SIP message has repeated headers.
    /// Parse with [`EslArray`](super::EslArray). Variants marked "single"
    /// contain one serialized header value.
    ///
    /// For headers not covered by this enum (dynamic unknown headers stored
    /// as `sip_i_<lowercased_name>`), use
    /// [`variable_str()`](crate::HeaderLookup::variable_str).
    pub enum SipInviteHeader {
        // --- Single-value headers ---

        /// SIP From header.
        From => "sip_i_from",
        /// SIP To header.
        To => "sip_i_to",
        /// SIP Call-ID header.
        CallId => "sip_i_call_id",
        /// SIP CSeq header.
        Cseq => "sip_i_cseq",
        /// SIP Identity header (RFC 8224).
        Identity => "sip_i_identity",
        /// SIP Route header.
        Route => "sip_i_route",
        /// SIP Max-Forwards header.
        MaxForwards => "sip_i_max_forwards",
        /// SIP Proxy-Require header.
        ProxyRequire => "sip_i_proxy_require",
        /// SIP Contact header.
        Contact => "sip_i_contact",
        /// SIP User-Agent header.
        UserAgent => "sip_i_user_agent",
        /// SIP Subject header.
        Subject => "sip_i_subject",
        /// SIP Priority header.
        Priority => "sip_i_priority",
        /// SIP Organization header.
        Organization => "sip_i_organization",
        /// SIP In-Reply-To header.
        InReplyTo => "sip_i_in_reply_to",
        /// SIP Accept-Encoding header.
        AcceptEncoding => "sip_i_accept_encoding",
        /// SIP Accept-Language header.
        AcceptLanguage => "sip_i_accept_language",
        /// SIP Allow header.
        Allow => "sip_i_allow",
        /// SIP Require header.
        Require => "sip_i_require",
        /// SIP Supported header.
        Supported => "sip_i_supported",
        /// SIP Date header.
        Date => "sip_i_date",
        /// SIP Timestamp header.
        Timestamp => "sip_i_timestamp",
        /// SIP Expires header.
        Expires => "sip_i_expires",
        /// SIP Min-Expires header.
        MinExpires => "sip_i_min_expires",
        /// SIP Session-Expires header.
        SessionExpires => "sip_i_session_expires",
        /// SIP Min-SE header.
        MinSe => "sip_i_min_se",
        /// SIP Privacy header.
        Privacy => "sip_i_privacy",
        /// SIP MIME-Version header.
        MimeVersion => "sip_i_mime_version",
        /// SIP Content-Type header.
        ContentType => "sip_i_content_type",
        /// SIP Content-Encoding header.
        ContentEncoding => "sip_i_content_encoding",
        /// SIP Content-Language header.
        ContentLanguage => "sip_i_content_language",
        /// SIP Content-Disposition header.
        ContentDisposition => "sip_i_content_disposition",
        /// SIP Content-Length header.
        ContentLength => "sip_i_content_length",

        // --- ARRAY headers (may contain multiple values) ---

        /// SIP Via headers. ARRAY when multiple hops present.
        Via => "sip_i_via",
        /// SIP Record-Route headers. ARRAY when multiple proxies present.
        RecordRoute => "sip_i_record_route",
        /// SIP Proxy-Authorization headers. ARRAY when multiple credentials present.
        ProxyAuthorization => "sip_i_proxy_authorization",
        /// SIP Call-Info headers. ARRAY when multiple info URIs present.
        CallInfo => "sip_i_call_info",
        /// SIP Accept headers. ARRAY when multiple media types present.
        Accept => "sip_i_accept",
        /// SIP Authorization headers. ARRAY when multiple credentials present.
        Authorization => "sip_i_authorization",
        /// SIP Alert-Info headers. ARRAY when multiple alert URIs present.
        AlertInfo => "sip_i_alert_info",
        /// SIP P-Asserted-Identity headers. ARRAY when multiple identities present (RFC 3325).
        PAssertedIdentity => "sip_i_p_asserted_identity",
        /// SIP P-Preferred-Identity headers. ARRAY when multiple identities present.
        PPreferredIdentity => "sip_i_p_preferred_identity",
        /// SIP Remote-Party-ID headers. ARRAY when multiple identities present.
        RemotePartyId => "sip_i_remote_party_id",
        /// SIP Reply-To headers. ARRAY when multiple reply addresses present.
        ReplyTo => "sip_i_reply_to",
    }
}

impl SipInviteHeader {
    /// Headers that may contain multiple values in ARRAY format.
    pub const ARRAY_HEADERS: &[SipInviteHeader] = &[
        SipInviteHeader::Via,
        SipInviteHeader::RecordRoute,
        SipInviteHeader::ProxyAuthorization,
        SipInviteHeader::CallInfo,
        SipInviteHeader::Accept,
        SipInviteHeader::Authorization,
        SipInviteHeader::AlertInfo,
        SipInviteHeader::PAssertedIdentity,
        SipInviteHeader::PPreferredIdentity,
        SipInviteHeader::RemotePartyId,
        SipInviteHeader::ReplyTo,
    ];

    /// Whether this header may contain multiple values in ARRAY format.
    pub fn is_array_header(&self) -> bool {
        Self::ARRAY_HEADERS.contains(self)
    }

    /// Canonical SIP header name (e.g. `"From"`, `"Call-ID"`).
    pub fn header_name(&self) -> &'static str {
        match self {
            Self::From => "From",
            Self::To => "To",
            Self::CallId => "Call-ID",
            Self::Cseq => "CSeq",
            Self::Identity => "Identity",
            Self::Route => "Route",
            Self::MaxForwards => "Max-Forwards",
            Self::ProxyRequire => "Proxy-Require",
            Self::Contact => "Contact",
            Self::UserAgent => "User-Agent",
            Self::Subject => "Subject",
            Self::Priority => "Priority",
            Self::Organization => "Organization",
            Self::InReplyTo => "In-Reply-To",
            Self::AcceptEncoding => "Accept-Encoding",
            Self::AcceptLanguage => "Accept-Language",
            Self::Allow => "Allow",
            Self::Require => "Require",
            Self::Supported => "Supported",
            Self::Date => "Date",
            Self::Timestamp => "Timestamp",
            Self::Expires => "Expires",
            Self::MinExpires => "Min-Expires",
            Self::SessionExpires => "Session-Expires",
            Self::MinSe => "Min-SE",
            Self::Privacy => "Privacy",
            Self::MimeVersion => "MIME-Version",
            Self::ContentType => "Content-Type",
            Self::ContentEncoding => "Content-Encoding",
            Self::ContentLanguage => "Content-Language",
            Self::ContentDisposition => "Content-Disposition",
            Self::ContentLength => "Content-Length",
            Self::Via => "Via",
            Self::RecordRoute => "Record-Route",
            Self::ProxyAuthorization => "Proxy-Authorization",
            Self::CallInfo => "Call-Info",
            Self::Accept => "Accept",
            Self::Authorization => "Authorization",
            Self::AlertInfo => "Alert-Info",
            Self::PAssertedIdentity => "P-Asserted-Identity",
            Self::PPreferredIdentity => "P-Preferred-Identity",
            Self::RemotePartyId => "Remote-Party-ID",
            Self::ReplyTo => "Reply-To",
        }
    }

    /// Extract this header's value from a raw SIP message.
    ///
    /// Delegates to [`extract_header`](sip_header::extract_header)
    /// using the canonical name from [`header_name()`](Self::header_name).
    pub fn extract_from(&self, message: &str) -> Option<String> {
        sip_header::extract_header(message, self.header_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trip() {
        assert_eq!(
            SipInviteHeader::PAssertedIdentity.to_string(),
            "sip_i_p_asserted_identity"
        );
        assert_eq!(SipInviteHeader::From.to_string(), "sip_i_from");
        assert_eq!(SipInviteHeader::Via.to_string(), "sip_i_via");
    }

    #[test]
    fn as_ref_str() {
        let v: &str = SipInviteHeader::CallId.as_ref();
        assert_eq!(v, "sip_i_call_id");
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!(
            "sip_i_p_asserted_identity".parse::<SipInviteHeader>(),
            Ok(SipInviteHeader::PAssertedIdentity)
        );
        assert_eq!(
            "SIP_I_P_ASSERTED_IDENTITY".parse::<SipInviteHeader>(),
            Ok(SipInviteHeader::PAssertedIdentity)
        );
    }

    #[test]
    fn from_str_unknown() {
        assert!("sip_i_nonexistent"
            .parse::<SipInviteHeader>()
            .is_err());
    }

    #[test]
    fn from_str_round_trip_all() {
        let variants = [
            SipInviteHeader::From,
            SipInviteHeader::To,
            SipInviteHeader::CallId,
            SipInviteHeader::Via,
            SipInviteHeader::RecordRoute,
            SipInviteHeader::PAssertedIdentity,
            SipInviteHeader::PPreferredIdentity,
            SipInviteHeader::RemotePartyId,
            SipInviteHeader::AlertInfo,
            SipInviteHeader::Privacy,
            SipInviteHeader::ContentType,
        ];
        for v in variants {
            let wire = v.to_string();
            let parsed: SipInviteHeader = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, v, "round-trip failed for {wire}");
        }
    }

    #[test]
    fn header_name_mapping() {
        assert_eq!(SipInviteHeader::From.header_name(), "From");
        assert_eq!(SipInviteHeader::CallId.header_name(), "Call-ID");
        assert_eq!(SipInviteHeader::Cseq.header_name(), "CSeq");
        assert_eq!(
            SipInviteHeader::PAssertedIdentity.header_name(),
            "P-Asserted-Identity"
        );
        assert_eq!(SipInviteHeader::MinSe.header_name(), "Min-SE");
        assert_eq!(SipInviteHeader::MimeVersion.header_name(), "MIME-Version");
        assert_eq!(SipInviteHeader::UserAgent.header_name(), "User-Agent");
        assert_eq!(SipInviteHeader::RecordRoute.header_name(), "Record-Route");
    }

    #[test]
    fn extract_from_sip_message() {
        let msg = "INVITE sip:bob@host SIP/2.0\r\n\
                   From: Alice <sip:alice@host>;tag=abc\r\n\
                   To: Bob <sip:bob@host>\r\n\
                   Call-ID: 12345@host\r\n\
                   \r\n";
        assert_eq!(
            SipInviteHeader::From.extract_from(msg),
            Some("Alice <sip:alice@host>;tag=abc".into())
        );
        assert_eq!(
            SipInviteHeader::CallId.extract_from(msg),
            Some("12345@host".into())
        );
    }

    #[test]
    fn extract_from_array_header() {
        let msg = "INVITE sip:bob@host SIP/2.0\r\n\
                   Via: SIP/2.0/UDP first.example.com\r\n\
                   Via: SIP/2.0/UDP second.example.com\r\n\
                   \r\n";
        assert_eq!(
            SipInviteHeader::Via.extract_from(msg),
            Some("SIP/2.0/UDP first.example.com, SIP/2.0/UDP second.example.com".into())
        );
    }

    #[test]
    fn extract_from_missing() {
        let msg = "INVITE sip:bob@host SIP/2.0\r\n\
                   From: Alice <sip:alice@host>\r\n\
                   \r\n";
        assert_eq!(SipInviteHeader::Identity.extract_from(msg), None);
    }

    #[test]
    fn array_headers_classification() {
        assert!(SipInviteHeader::Via.is_array_header());
        assert!(SipInviteHeader::PAssertedIdentity.is_array_header());
        assert!(SipInviteHeader::RecordRoute.is_array_header());
        assert!(!SipInviteHeader::From.is_array_header());
        assert!(!SipInviteHeader::CallId.is_array_header());
        assert!(!SipInviteHeader::ContentType.is_array_header());
    }
}
