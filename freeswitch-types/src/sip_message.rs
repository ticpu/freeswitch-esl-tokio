//! RFC 3261 SIP message header extraction.
//!
//! Provides [`extract_header`] for pulling header values from raw SIP message
//! text, handling case-insensitive name matching, header folding (continuation
//! lines per RFC 3261 §7.3.1), and multi-occurrence concatenation.
//!
//! Compact header forms (RFC 3261 §7.3.3, e.g. `f` for `From`, `v` for `Via`)
//! are not recognized — use the full header name.

/// Extract a header value from a raw SIP message.
///
/// Scans all lines up to the blank line separating headers from the message
/// body. Header name matching is case-insensitive (RFC 3261 §7.3.5).
///
/// Header folding (continuation lines beginning with SP or HTAB) is unfolded
/// into a single logical value. When a header appears multiple times, values
/// are concatenated with `, ` (RFC 3261 §7.3.1).
///
/// Returns `None` if no header with the given name is found.
///
/// Compact header forms (RFC 3261 §7.3.3) are not supported.
pub fn extract_header(message: &str, name: &str) -> Option<String> {
    let mut values: Vec<String> = Vec::new();
    let mut current_match = false;

    for line in message.split('\n') {
        let line = line
            .strip_suffix('\r')
            .unwrap_or(line);

        if line.is_empty() {
            break;
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            if current_match {
                if let Some(last) = values.last_mut() {
                    last.push(' ');
                    last.push_str(line.trim_start());
                }
            }
            continue;
        }

        current_match = false;

        if let Some((hdr_name, hdr_value)) = line.split_once(':') {
            let hdr_name = hdr_name.trim_end();
            // RFC 3261: header names are tokens — no whitespace allowed.
            // This rejects request/status lines like "INVITE sip:..." where
            // the text before the first colon contains spaces.
            if !hdr_name.contains(' ') && hdr_name.eq_ignore_ascii_case(name) {
                current_match = true;
                values.push(
                    hdr_value
                        .trim_start()
                        .to_string(),
                );
            }
        }
    }

    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_INVITE: &str = "\
INVITE sip:bob@biloxi.example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bK776asdhds\r\n\
Via: SIP/2.0/UDP bigbox3.site3.atlanta.example.com;branch=z9hG4bKnashds8\r\n\
Max-Forwards: 70\r\n\
To: Bob <sip:bob@biloxi.example.com>\r\n\
From: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
Call-ID: a84b4c76e66710@pc33.atlanta.example.com\r\n\
CSeq: 314159 INVITE\r\n\
Contact: <sip:alice@pc33.atlanta.example.com>\r\n\
Content-Type: application/sdp\r\n\
Content-Length: 142\r\n\
\r\n\
v=0\r\n\
o=alice 2890844526 2890844526 IN IP4 pc33.atlanta.example.com\r\n";

    #[test]
    fn basic_extraction() {
        assert_eq!(
            extract_header(SAMPLE_INVITE, "From"),
            Some("Alice <sip:alice@atlanta.example.com>;tag=1928301774".into())
        );
        assert_eq!(
            extract_header(SAMPLE_INVITE, "Call-ID"),
            Some("a84b4c76e66710@pc33.atlanta.example.com".into())
        );
        assert_eq!(
            extract_header(SAMPLE_INVITE, "CSeq"),
            Some("314159 INVITE".into())
        );
    }

    #[test]
    fn case_insensitive_name() {
        let expected = Some("Alice <sip:alice@atlanta.example.com>;tag=1928301774".into());
        assert_eq!(extract_header(SAMPLE_INVITE, "from"), expected);
        assert_eq!(extract_header(SAMPLE_INVITE, "FROM"), expected);
        assert_eq!(extract_header(SAMPLE_INVITE, "From"), expected);
    }

    #[test]
    fn header_folding() {
        let msg = concat!(
            "SIP/2.0 200 OK\r\n",
            "Subject: I know you're there,\r\n",
            " pick up the phone\r\n",
            " and talk to me!\r\n",
            "\r\n",
        );
        assert_eq!(
            extract_header(msg, "Subject"),
            Some("I know you're there, pick up the phone and talk to me!".into())
        );
    }

    #[test]
    fn multiple_occurrences_concatenated() {
        assert_eq!(
            extract_header(SAMPLE_INVITE, "Via"),
            Some(
                "SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bK776asdhds, \
                 SIP/2.0/UDP bigbox3.site3.atlanta.example.com;branch=z9hG4bKnashds8"
                    .into()
            )
        );
    }

    #[test]
    fn stops_at_blank_line() {
        // Body contains "o=" which looks like it could be a header line
        assert_eq!(extract_header(SAMPLE_INVITE, "o"), None);
    }

    #[test]
    fn bare_lf_line_endings() {
        let msg = "SIP/2.0 200 OK\n\
                   From: Alice <sip:alice@host>\n\
                   To: Bob <sip:bob@host>\n\
                   \n\
                   body\n";
        assert_eq!(
            extract_header(msg, "From"),
            Some("Alice <sip:alice@host>".into())
        );
    }

    #[test]
    fn missing_header_returns_none() {
        assert_eq!(extract_header(SAMPLE_INVITE, "X-Custom"), None);
    }

    #[test]
    fn empty_message() {
        assert_eq!(extract_header("", "From"), None);
    }

    #[test]
    fn request_line_not_matched() {
        // The request line has a colon in the URI but should not match
        assert_eq!(extract_header(SAMPLE_INVITE, "INVITE sip"), None);
    }

    #[test]
    fn value_leading_whitespace_trimmed() {
        let msg = "SIP/2.0 200 OK\r\n\
                   From:   Alice <sip:alice@host>\r\n\
                   \r\n";
        assert_eq!(
            extract_header(msg, "From"),
            Some("Alice <sip:alice@host>".into())
        );
    }

    #[test]
    fn folding_on_multiple_occurrence() {
        let msg = concat!(
            "SIP/2.0 200 OK\r\n",
            "Via: SIP/2.0/UDP first.example.com\r\n",
            " ;branch=z9hG4bKaaa\r\n",
            "Via: SIP/2.0/UDP second.example.com;branch=z9hG4bKbbb\r\n",
            "\r\n",
        );
        assert_eq!(
            extract_header(msg, "Via"),
            Some(
                "SIP/2.0/UDP first.example.com ;branch=z9hG4bKaaa, \
                 SIP/2.0/UDP second.example.com;branch=z9hG4bKbbb"
                    .into()
            )
        );
    }

    #[test]
    fn empty_header_value() {
        let msg = "SIP/2.0 200 OK\r\n\
                   Subject:\r\n\
                   From: Alice <sip:alice@host>\r\n\
                   \r\n";
        assert_eq!(extract_header(msg, "Subject"), Some(String::new()));
    }

    #[test]
    fn tab_folding() {
        let msg = concat!(
            "SIP/2.0 200 OK\r\n",
            "Subject: hello\r\n",
            "\tworld\r\n",
            "\r\n",
        );
        assert_eq!(extract_header(msg, "Subject"), Some("hello world".into()));
    }

    // -- Integration pipeline tests: extract_header → existing parsers --

    const NG911_INVITE: &str = concat!(
        "INVITE sip:urn:service:sos@bcf.example.com SIP/2.0\r\n",
        "Via: SIP/2.0/TLS proxy.example.com;branch=z9hG4bK776\r\n",
        "From: \"Caller Name\" <sip:+15551234567@orig.example.com>;tag=abc123\r\n",
        "To: <sip:urn:service:sos@bcf.example.com>\r\n",
        "Call-ID: ng911-call-42@orig.example.com\r\n",
        "P-Asserted-Identity: \"EXAMPLE CO\" <sip:+15551234567@198.51.100.1>\r\n",
        "Call-Info: <urn:emergency:uid:callid:abc:bcf.example.com>;purpose=emergency-CallId,",
        "<https://adr.example.com/serviceInfo?t=x>;purpose=EmergencyCallData.ServiceInfo\r\n",
        "Geolocation: <cid:loc-id-1234>, <https://lis.example.com/held/test>\r\n",
        "Content-Type: application/sdp\r\n",
        "\r\n",
        "v=0\r\n",
    );

    #[test]
    fn extract_and_parse_call_info() {
        use crate::variables::SipCallInfo;

        let raw = extract_header(NG911_INVITE, "Call-Info").unwrap();
        let ci = SipCallInfo::parse(&raw).unwrap();
        assert_eq!(ci.len(), 2);
        assert_eq!(ci.entries()[0].purpose(), Some("emergency-CallId"));
        assert!(ci
            .find_by_purpose_suffix("ServiceInfo")
            .is_some());
    }

    #[test]
    fn extract_and_parse_p_asserted_identity() {
        use crate::sip_header_addr::SipHeaderAddr;

        let raw = extract_header(NG911_INVITE, "P-Asserted-Identity").unwrap();
        let pai: SipHeaderAddr = raw
            .parse()
            .unwrap();
        assert_eq!(pai.display_name(), Some("EXAMPLE CO"));
        assert!(pai
            .uri()
            .to_string()
            .contains("+15551234567"));
    }

    #[test]
    fn extract_and_parse_geolocation() {
        use crate::variables::SipGeolocation;

        let raw = extract_header(NG911_INVITE, "Geolocation").unwrap();
        let geo = SipGeolocation::parse(&raw);
        assert_eq!(geo.len(), 2);
        assert_eq!(geo.cid(), Some("loc-id-1234"));
        assert!(geo
            .url()
            .unwrap()
            .contains("lis.example.com"));
    }

    #[test]
    fn extract_and_parse_from_to() {
        use crate::sip_header_addr::SipHeaderAddr;

        let from_raw = extract_header(NG911_INVITE, "From").unwrap();
        let from: SipHeaderAddr = from_raw
            .parse()
            .unwrap();
        assert_eq!(from.display_name(), Some("Caller Name"));
        assert_eq!(from.tag(), Some("abc123"));

        let to_raw = extract_header(NG911_INVITE, "To").unwrap();
        let to: SipHeaderAddr = to_raw
            .parse()
            .unwrap();
        assert!(to
            .uri()
            .to_string()
            .contains("urn:service:sos"));
    }
}
