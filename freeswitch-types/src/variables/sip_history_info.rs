use std::fmt;
use std::str::Utf8Error;

use percent_encoding::percent_decode_str;

use crate::sip_header_addr::{ParseSipHeaderAddrError, SipHeaderAddr};

use super::EslArray;

/// Errors from parsing a History-Info header value (RFC 7044).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryInfoError {
    /// The input string was empty or whitespace-only.
    Empty,
    /// An entry could not be parsed as a SIP name-addr.
    InvalidEntry(ParseSipHeaderAddrError),
}

impl fmt::Display for HistoryInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty History-Info header"),
            Self::InvalidEntry(e) => write!(f, "invalid History-Info entry: {e}"),
        }
    }
}

impl std::error::Error for HistoryInfoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEntry(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ParseSipHeaderAddrError> for HistoryInfoError {
    fn from(e: ParseSipHeaderAddrError) -> Self {
        Self::InvalidEntry(e)
    }
}

/// Parsed RFC 3326 Reason header value extracted from a History-Info URI.
///
/// The Reason header embedded in History-Info URIs as `?Reason=...` follows
/// the format: `protocol ;cause=code ;text="description"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryInfoReason {
    protocol: String,
    cause: Option<u16>,
    text: Option<String>,
}

impl HistoryInfoReason {
    /// The protocol token (e.g. `"SIP"`, `"Q.850"`, `"RouteAction"`).
    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    /// The cause code, if present (e.g. `200`, `302`).
    pub fn cause(&self) -> Option<u16> {
        self.cause
    }

    /// The human-readable reason text, if present.
    pub fn text(&self) -> Option<&str> {
        self.text
            .as_deref()
    }
}

/// Parse a percent-decoded RFC 3326 Reason value.
///
/// Input format: `protocol;cause=N;text="description"`
fn parse_reason(decoded: &str) -> HistoryInfoReason {
    let (protocol, rest) = decoded
        .split_once(';')
        .unwrap_or((decoded, ""));

    let mut cause = None;
    let mut text = None;

    // Extract cause (always a simple integer, safe to find by prefix)
    if let Some(idx) = rest.find("cause=") {
        let val_start = idx + 6;
        let val_end = rest[val_start..]
            .find(';')
            .map(|i| val_start + i)
            .unwrap_or(rest.len());
        cause = rest[val_start..val_end]
            .trim()
            .parse::<u16>()
            .ok();
    }

    // Extract text (may be quoted, always appears after cause in practice)
    if let Some(idx) = rest.find("text=") {
        let val_start = idx + 5;
        let val = rest[val_start..].trim_start();
        if let Some(inner) = val.strip_prefix('"') {
            if let Some(end) = inner.find('"') {
                text = Some(inner[..end].to_string());
            } else {
                text = Some(inner.to_string());
            }
        } else {
            let end = val
                .find(';')
                .unwrap_or(val.len());
            text = Some(val[..end].to_string());
        }
    }

    HistoryInfoReason {
        protocol: protocol
            .trim()
            .to_string(),
        cause,
        text,
    }
}

/// A single entry from a History-Info header (RFC 7044).
///
/// Each entry is a SIP name-addr (`<URI>;params`) where the URI may contain
/// an embedded `?Reason=...` header and the params typically include `index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryInfoEntry {
    addr: SipHeaderAddr,
}

impl HistoryInfoEntry {
    /// The underlying parsed name-addr with header-level parameters.
    pub fn addr(&self) -> &SipHeaderAddr {
        &self.addr
    }

    /// The URI from this entry.
    pub fn uri(&self) -> &sip_uri::Uri {
        self.addr
            .uri()
    }

    /// The SIP URI, if this entry uses a `sip:` or `sips:` scheme.
    pub fn sip_uri(&self) -> Option<&sip_uri::SipUri> {
        self.addr
            .sip_uri()
    }

    /// The `index` parameter value (e.g. `"1"`, `"1.1"`, `"1.2"`).
    pub fn index(&self) -> Option<&str> {
        self.addr
            .param_raw("index")
            .flatten()
    }

    /// Raw percent-encoded Reason value from the URI `?Reason=...` header.
    ///
    /// Returns `None` if the URI is not a SIP URI or has no Reason header.
    pub fn reason_raw(&self) -> Option<&str> {
        self.addr
            .sip_uri()?
            .header("Reason")
    }

    /// Parse the Reason header embedded in the URI.
    ///
    /// The Reason value is percent-decoded (with `+` treated as space,
    /// matching common SIP URI encoding conventions) and parsed into
    /// protocol, cause code, and text components per RFC 3326.
    ///
    /// Returns `None` if no Reason is present, `Err` if percent-decoding
    /// produces invalid UTF-8.
    pub fn reason(&self) -> Option<Result<HistoryInfoReason, Utf8Error>> {
        let raw = self.reason_raw()?;
        // SIP stacks commonly use + for space in URI header values
        // (form-encoding convention). Replace before percent-decoding
        // so %2B (literal +) is preserved correctly.
        let raw = raw.replace('+', " ");
        Some(
            percent_decode_str(&raw)
                .decode_utf8()
                .map(|decoded| parse_reason(&decoded)),
        )
    }
}

impl fmt::Display for HistoryInfoEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.addr)
    }
}

/// Parsed History-Info header value (RFC 7044).
///
/// Contains one or more routing-chain entries, each with a SIP URI,
/// optional index, and optional embedded Reason header.
///
/// Accepts standard comma-separated SIP format, FreeSWITCH `ARRAY::|:`
/// encoding, and bracket-wrapped format from FreeSWITCH logs.
///
/// ```
/// use freeswitch_types::variables::HistoryInfo;
///
/// let raw = "<sip:alice@esrp.example.com>;index=1,<sip:sos@psap.example.com>;index=1.1";
/// let hi = HistoryInfo::parse(raw).unwrap();
/// assert_eq!(hi.len(), 2);
/// assert_eq!(hi.entries()[0].index(), Some("1"));
/// assert_eq!(hi.entries()[1].index(), Some("1.1"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryInfo(Vec<HistoryInfoEntry>);

impl HistoryInfo {
    /// Parse a raw History-Info header value.
    ///
    /// Handles comma-separated SIP format, FreeSWITCH `ARRAY::|:` encoding,
    /// and bracket-wrapped values from FreeSWITCH logs.
    pub fn parse(raw: &str) -> Result<Self, HistoryInfoError> {
        let raw = raw.trim();
        // Strip bracket wrapping from FreeSWITCH log format
        let raw = raw
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(raw);
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(HistoryInfoError::Empty);
        }

        let entries = if let Some(arr) = EslArray::parse(raw) {
            arr.into_iter()
                .map(|s| parse_entry(&s))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            super::split_comma_entries(raw)
                .into_iter()
                .map(parse_entry)
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(Self(entries))
    }

    /// The parsed entries as a slice.
    pub fn entries(&self) -> &[HistoryInfoEntry] {
        &self.0
    }

    /// Consume self and return the entries as a `Vec`.
    pub fn into_entries(self) -> Vec<HistoryInfoEntry> {
        self.0
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0
            .len()
    }

    /// Returns `true` if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.0
            .is_empty()
    }
}

fn parse_entry(raw: &str) -> Result<HistoryInfoEntry, HistoryInfoError> {
    let addr: SipHeaderAddr = raw
        .trim()
        .parse()?;
    Ok(HistoryInfoEntry { addr })
}

impl fmt::Display for HistoryInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, entry) in self
            .0
            .iter()
            .enumerate()
        {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{entry}")?;
        }
        Ok(())
    }
}

impl<'a> IntoIterator for &'a HistoryInfo {
    type Item = &'a HistoryInfoEntry;
    type IntoIter = std::slice::Iter<'a, HistoryInfoEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .iter()
    }
}

impl IntoIterator for HistoryInfo {
    type Item = HistoryInfoEntry;
    type IntoIter = std::vec::IntoIter<HistoryInfoEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Real-world examples from FreeSWITCH NG9-1-1 logs --

    const EXAMPLE_1: &str = "\
<sip:cauca1@esrp.qc.core.ng.911bell.ca?Reason=RouteAction%3Bcause%3D200%3Btext%3D%22Normal+Next+Hop%22>;index=1,\
<sip:sos@cauca1.qc.psap.ng.911bell.ca>;index=2";

    const EXAMPLE_2: &str = "\
<sip:lsrg.core.ng.911bell.ca?Reason=SIP%3Bcause%3D200%3Btext%3D%22Legacy+routing%22>;index=1,\
<sip:cauca1@esrp.mb.core.ng.911bell.ca;lr;transport=udp?Reason=RouteAction%3Bcause%3D200%3Btext%3D%22Normal+Next+Hop%22>;index=1.1,\
<sip:sos@cauca1.qc.psap.ng.911bell.ca>;index=1.2";

    // Bracket-wrapped format from FreeSWITCH logs
    const EXAMPLE_BRACKETED: &str = "\
[<sip:lsrg.core.ng.911bell.ca?Reason=SIP%3Bcause%3D200%3Btext%3D%22Legacy+routing%22>;index=1,\
<sip:sos@cauca1.qc.psap.ng.911bell.ca>;index=1.2]";

    // -- Entry count tests --

    #[test]
    fn parse_two_entries() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        assert_eq!(hi.len(), 2);
    }

    #[test]
    fn parse_three_entries() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        assert_eq!(hi.len(), 3);
    }

    #[test]
    fn parse_bracket_wrapped() {
        let hi = HistoryInfo::parse(EXAMPLE_BRACKETED).unwrap();
        assert_eq!(hi.len(), 2);
        assert_eq!(hi.entries()[0].index(), Some("1"));
    }

    #[test]
    fn parse_single_entry() {
        let hi = HistoryInfo::parse("<sip:alice@example.com>;index=1").unwrap();
        assert_eq!(hi.len(), 1);
    }

    #[test]
    fn empty_input() {
        assert!(matches!(
            HistoryInfo::parse(""),
            Err(HistoryInfoError::Empty)
        ));
    }

    #[test]
    fn empty_brackets() {
        assert!(matches!(
            HistoryInfo::parse("[]"),
            Err(HistoryInfoError::Empty)
        ));
    }

    // -- Index accessor tests --

    #[test]
    fn index_simple() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        assert_eq!(hi.entries()[0].index(), Some("1"));
        assert_eq!(hi.entries()[1].index(), Some("2"));
    }

    #[test]
    fn index_hierarchical() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        assert_eq!(hi.entries()[0].index(), Some("1"));
        assert_eq!(hi.entries()[1].index(), Some("1.1"));
        assert_eq!(hi.entries()[2].index(), Some("1.2"));
    }

    #[test]
    fn index_absent() {
        let hi = HistoryInfo::parse("<sip:alice@example.com>").unwrap();
        assert_eq!(hi.entries()[0].index(), None);
    }

    // -- URI accessor tests --

    #[test]
    fn uri_with_user() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        let sip = hi.entries()[0]
            .sip_uri()
            .unwrap();
        assert_eq!(sip.user(), Some("cauca1"));
        assert_eq!(
            sip.host()
                .to_string(),
            "esrp.qc.core.ng.911bell.ca"
        );
    }

    #[test]
    fn uri_without_user() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        let sip = hi.entries()[0]
            .sip_uri()
            .unwrap();
        assert_eq!(sip.user(), None);
        assert_eq!(
            sip.host()
                .to_string(),
            "lsrg.core.ng.911bell.ca"
        );
    }

    #[test]
    fn uri_with_params() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        let sip = hi.entries()[1]
            .sip_uri()
            .unwrap();
        assert_eq!(sip.user(), Some("cauca1"));
        assert_eq!(
            sip.host()
                .to_string(),
            "esrp.mb.core.ng.911bell.ca"
        );
        assert!(sip
            .param("lr")
            .is_some());
        assert_eq!(sip.param("transport"), Some(&Some("udp".to_string())));
    }

    // -- Reason accessor tests --

    #[test]
    fn reason_raw_present() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        assert_eq!(
            hi.entries()[0].reason_raw(),
            Some("RouteAction%3Bcause%3D200%3Btext%3D%22Normal+Next+Hop%22")
        );
    }

    #[test]
    fn reason_raw_absent() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        assert_eq!(hi.entries()[1].reason_raw(), None);
    }

    #[test]
    fn reason_parsed_route_action() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        let reason = hi.entries()[0]
            .reason()
            .unwrap()
            .unwrap();
        assert_eq!(reason.protocol(), "RouteAction");
        assert_eq!(reason.cause(), Some(200));
        assert_eq!(reason.text(), Some("Normal Next Hop"));
    }

    #[test]
    fn reason_parsed_sip() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        let reason = hi.entries()[0]
            .reason()
            .unwrap()
            .unwrap();
        assert_eq!(reason.protocol(), "SIP");
        assert_eq!(reason.cause(), Some(200));
        assert_eq!(reason.text(), Some("Legacy routing"));
    }

    #[test]
    fn reason_absent_returns_none() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        assert!(hi.entries()[2]
            .reason()
            .is_none());
    }

    #[test]
    fn reason_multiple_entries() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        let r0 = hi.entries()[0]
            .reason()
            .unwrap()
            .unwrap();
        let r1 = hi.entries()[1]
            .reason()
            .unwrap()
            .unwrap();
        assert_eq!(r0.protocol(), "SIP");
        assert_eq!(r1.protocol(), "RouteAction");
        assert!(hi.entries()[2]
            .reason()
            .is_none());
    }

    // -- Display round-trip tests --

    #[test]
    fn display_roundtrip_simple() {
        let raw = "<sip:alice@example.com>;index=1";
        let hi = HistoryInfo::parse(raw).unwrap();
        assert_eq!(hi.to_string(), raw);
    }

    #[test]
    fn display_entry_count_matches_commas() {
        let hi = HistoryInfo::parse(EXAMPLE_2).unwrap();
        let s = hi.to_string();
        assert_eq!(
            s.matches(',')
                .count()
                + 1,
            hi.len()
        );
    }

    #[test]
    fn display_roundtrip_real_world() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        let reparsed = HistoryInfo::parse(&hi.to_string()).unwrap();
        assert_eq!(hi.len(), reparsed.len());
        for (a, b) in hi
            .entries()
            .iter()
            .zip(reparsed.entries())
        {
            assert_eq!(a.index(), b.index());
            assert_eq!(a.reason_raw(), b.reason_raw());
        }
    }

    // -- ARRAY format tests --

    #[test]
    fn parse_array_format() {
        let raw = "ARRAY::\
<sip:esrp@example.com>;index=1|:\
<sip:sos@psap.example.com>;index=1.1";
        let hi = HistoryInfo::parse(raw).unwrap();
        assert_eq!(hi.len(), 2);
        assert_eq!(hi.entries()[0].index(), Some("1"));
        assert_eq!(hi.entries()[1].index(), Some("1.1"));
    }

    // -- Iterator tests --

    #[test]
    fn iter_by_ref() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        let indices: Vec<_> = hi
            .entries()
            .iter()
            .map(|e| e.index())
            .collect();
        assert_eq!(indices, vec![Some("1"), Some("2")]);
    }

    #[test]
    fn into_entries() {
        let hi = HistoryInfo::parse(EXAMPLE_1).unwrap();
        let entries = hi.into_entries();
        assert_eq!(entries.len(), 2);
    }

    // -- parse_reason unit tests --

    #[test]
    fn parse_reason_full() {
        let r = parse_reason("SIP;cause=302;text=\"Moved\"");
        assert_eq!(r.protocol(), "SIP");
        assert_eq!(r.cause(), Some(302));
        assert_eq!(r.text(), Some("Moved"));
    }

    #[test]
    fn parse_reason_no_text() {
        let r = parse_reason("Q.850;cause=16");
        assert_eq!(r.protocol(), "Q.850");
        assert_eq!(r.cause(), Some(16));
        assert_eq!(r.text(), None);
    }

    #[test]
    fn parse_reason_protocol_only() {
        let r = parse_reason("SIP");
        assert_eq!(r.protocol(), "SIP");
        assert_eq!(r.cause(), None);
        assert_eq!(r.text(), None);
    }

    #[test]
    fn parse_reason_unquoted_text() {
        let r = parse_reason("SIP;cause=200;text=OK");
        assert_eq!(r.text(), Some("OK"));
    }
}
