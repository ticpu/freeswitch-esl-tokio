//! Standard SIP header names and typed lookup trait (RFC 3261 and extensions).
//!
//! Protocol-agnostic catalog of SIP header names with canonical wire casing,
//! plus a [`SipHeaderLookup`] trait providing typed convenience accessors for
//! any key-value store that can look up headers by name.

use crate::sip_header_addr::{ParseSipHeaderAddrError, SipHeaderAddr};
use crate::variables::{SipCallInfo, SipCallInfoError};

/// Error returned when parsing an unrecognized SIP header name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSipHeaderError(pub String);

impl std::fmt::Display for ParseSipHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown SIP header: {}", self.0)
    }
}

impl std::error::Error for ParseSipHeaderError {}

define_header_enum! {
    error_type: ParseSipHeaderError,
    /// Standard SIP header names with canonical wire casing.
    ///
    /// Each variant maps to the header's canonical form as defined in the
    /// relevant RFC. `FromStr` is case-insensitive; `Display` always emits
    /// the canonical form.
    pub enum SipHeader {
        /// `Call-Info` (RFC 3261 section 20.9).
        CallInfo => "Call-Info",
        /// `P-Asserted-Identity` (RFC 3325).
        PAssertedIdentity => "P-Asserted-Identity",
    }
}

/// Trait for looking up standard SIP headers from any key-value store.
///
/// Implementors provide `sip_header_str()` and get all typed accessors as
/// default implementations. A blanket implementation bridges
/// [`HeaderLookup`](crate::HeaderLookup) implementors automatically.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use freeswitch_types::{SipHeaderLookup, SipHeader};
///
/// let mut headers = HashMap::new();
/// headers.insert(
///     "Call-Info".to_string(),
///     "<urn:emergency:uid:callid:abc>;purpose=emergency-CallId".to_string(),
/// );
///
/// assert_eq!(
///     headers.sip_header(SipHeader::CallInfo),
///     Some("<urn:emergency:uid:callid:abc>;purpose=emergency-CallId"),
/// );
/// let ci = headers.call_info().unwrap().unwrap();
/// assert_eq!(ci.entries()[0].purpose(), Some("emergency-CallId"));
/// ```
pub trait SipHeaderLookup {
    /// Look up a SIP header by its raw wire name (e.g. `"Call-Info"`).
    fn sip_header_str(&self, name: &str) -> Option<&str>;

    /// Look up a SIP header by its [`SipHeader`] enum variant.
    fn sip_header(&self, name: SipHeader) -> Option<&str> {
        self.sip_header_str(name.as_str())
    }

    /// Raw `Call-Info` header value (RFC 3261 section 20.9).
    fn call_info_raw(&self) -> Option<&str> {
        self.sip_header(SipHeader::CallInfo)
    }

    /// Parse the `Call-Info` header into a [`SipCallInfo`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn call_info(&self) -> Result<Option<SipCallInfo>, SipCallInfoError> {
        match self.call_info_raw() {
            Some(s) => SipCallInfo::parse(s).map(Some),
            None => Ok(None),
        }
    }

    /// Raw `P-Asserted-Identity` header value (RFC 3325).
    fn p_asserted_identity_raw(&self) -> Option<&str> {
        self.sip_header(SipHeader::PAssertedIdentity)
    }

    /// Parse the `P-Asserted-Identity` header into a [`SipHeaderAddr`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn p_asserted_identity(&self) -> Result<Option<SipHeaderAddr>, ParseSipHeaderAddrError> {
        match self.p_asserted_identity_raw() {
            Some(s) => s
                .parse::<SipHeaderAddr>()
                .map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn display_round_trip() {
        assert_eq!(SipHeader::CallInfo.to_string(), "Call-Info");
        assert_eq!(
            SipHeader::PAssertedIdentity.to_string(),
            "P-Asserted-Identity"
        );
    }

    #[test]
    fn as_ref_str() {
        let h: &str = SipHeader::CallInfo.as_ref();
        assert_eq!(h, "Call-Info");
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!("call-info".parse::<SipHeader>(), Ok(SipHeader::CallInfo));
        assert_eq!("CALL-INFO".parse::<SipHeader>(), Ok(SipHeader::CallInfo));
        assert_eq!(
            "p-asserted-identity".parse::<SipHeader>(),
            Ok(SipHeader::PAssertedIdentity)
        );
        assert_eq!(
            "P-ASSERTED-IDENTITY".parse::<SipHeader>(),
            Ok(SipHeader::PAssertedIdentity)
        );
    }

    #[test]
    fn from_str_unknown() {
        assert!("X-Custom"
            .parse::<SipHeader>()
            .is_err());
    }

    #[test]
    fn from_str_round_trip_all() {
        let variants = [SipHeader::CallInfo, SipHeader::PAssertedIdentity];
        for v in variants {
            let wire = v.to_string();
            let parsed: SipHeader = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, v, "round-trip failed for {wire}");
        }
    }

    fn headers_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn sip_header_by_enum() {
        let h = headers_with(&[("Call-Info", "<urn:x>;purpose=icon")]);
        assert_eq!(
            h.sip_header(SipHeader::CallInfo),
            Some("<urn:x>;purpose=icon")
        );
    }

    #[test]
    fn call_info_raw_lookup() {
        let h = headers_with(&[(
            "Call-Info",
            "<urn:emergency:uid:callid:test:bcf.example.com>;purpose=emergency-CallId",
        )]);
        assert_eq!(
            h.call_info_raw(),
            Some("<urn:emergency:uid:callid:test:bcf.example.com>;purpose=emergency-CallId")
        );
    }

    #[test]
    fn call_info_typed() {
        let h = headers_with(&[(
            "Call-Info",
            "<urn:emergency:uid:callid:test:bcf.example.com>;purpose=emergency-CallId",
        )]);
        let ci = h
            .call_info()
            .unwrap()
            .unwrap();
        assert_eq!(ci.len(), 1);
        assert_eq!(ci.entries()[0].purpose(), Some("emergency-CallId"));
    }

    #[test]
    fn call_info_absent() {
        let h = headers_with(&[]);
        assert_eq!(
            h.call_info()
                .unwrap(),
            None
        );
    }

    #[test]
    fn p_asserted_identity_raw_lookup() {
        let h = headers_with(&[(
            "P-Asserted-Identity",
            r#""EXAMPLE CO" <sip:+15551234567@198.51.100.1>"#,
        )]);
        assert_eq!(
            h.p_asserted_identity_raw(),
            Some(r#""EXAMPLE CO" <sip:+15551234567@198.51.100.1>"#)
        );
    }

    #[test]
    fn p_asserted_identity_typed() {
        let h = headers_with(&[(
            "P-Asserted-Identity",
            r#""EXAMPLE CO" <sip:+15551234567@198.51.100.1>"#,
        )]);
        let pai = h
            .p_asserted_identity()
            .unwrap()
            .unwrap();
        assert_eq!(pai.display_name(), Some("EXAMPLE CO"));
    }

    #[test]
    fn p_asserted_identity_absent() {
        let h = headers_with(&[]);
        assert_eq!(
            h.p_asserted_identity()
                .unwrap(),
            None
        );
    }

    #[test]
    fn extract_from_sip_message() {
        let msg = concat!(
            "INVITE sip:bob@host SIP/2.0\r\n",
            "Call-Info: <urn:emergency:uid:callid:abc>;purpose=emergency-CallId\r\n",
            "P-Asserted-Identity: \"Corp\" <sip:+15551234567@198.51.100.1>\r\n",
            "\r\n",
        );
        assert_eq!(
            SipHeader::CallInfo.extract_from(msg),
            Some("<urn:emergency:uid:callid:abc>;purpose=emergency-CallId".into())
        );
        assert_eq!(
            SipHeader::PAssertedIdentity.extract_from(msg),
            Some("\"Corp\" <sip:+15551234567@198.51.100.1>".into())
        );
    }

    #[test]
    fn extract_from_missing() {
        let msg = concat!(
            "INVITE sip:bob@host SIP/2.0\r\n",
            "From: Alice <sip:alice@host>\r\n",
            "\r\n",
        );
        assert_eq!(SipHeader::CallInfo.extract_from(msg), None);
        assert_eq!(SipHeader::PAssertedIdentity.extract_from(msg), None);
    }

    #[test]
    fn missing_headers_return_none() {
        let h = headers_with(&[]);
        assert_eq!(h.call_info_raw(), None);
        assert_eq!(
            h.call_info()
                .unwrap(),
            None
        );
        assert_eq!(h.p_asserted_identity_raw(), None);
        assert_eq!(
            h.p_asserted_identity()
                .unwrap(),
            None
        );
    }
}
