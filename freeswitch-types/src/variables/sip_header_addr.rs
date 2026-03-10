use std::fmt;
use std::str::FromStr;

use percent_encoding::percent_decode_str;

/// A SIP header field value containing a name-addr or addr-spec with
/// header-level parameters (RFC 3261 S20).
///
/// Handles the full grammar: `(name-addr / addr-spec) *(SEMI generic-param)`
///
/// Used for SIP headers like `From`, `To`, `Contact`, `P-Asserted-Identity`,
/// and `Refer-To` where parameters like `;tag=`, `;expires=`, and
/// `;serviceurn=` follow the URI.
///
/// ```
/// use freeswitch_types::variables::SipHeaderAddr;
///
/// let addr: SipHeaderAddr = r#""Alice" <sip:alice@example.com>;tag=abc123"#.parse().unwrap();
/// assert_eq!(addr.display_name(), Some("Alice"));
/// assert_eq!(addr.tag(), Some("abc123"));
/// ```
///
/// [`Display`](std::fmt::Display) always emits angle brackets around the URI,
/// even for bare addr-spec input. This is the canonical form required by
/// RFC 3261 when header-level parameters are present.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SipHeaderAddr {
    display_name: Option<String>,
    uri: sip_uri::Uri,
    params: Vec<(String, Option<String>)>,
}

/// Error returned when parsing a SIP header address value fails.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid SIP header address: {0}")]
pub struct ParseSipHeaderAddrError(pub String);

impl From<sip_uri::ParseUriError> for ParseSipHeaderAddrError {
    fn from(e: sip_uri::ParseUriError) -> Self {
        Self(e.to_string())
    }
}

impl From<sip_uri::ParseSipUriError> for ParseSipHeaderAddrError {
    fn from(e: sip_uri::ParseSipUriError) -> Self {
        Self(e.to_string())
    }
}

impl SipHeaderAddr {
    /// Create a new `SipHeaderAddr` with the given URI and no display name or params.
    pub fn new(uri: sip_uri::Uri) -> Self {
        SipHeaderAddr {
            display_name: None,
            uri,
            params: Vec::new(),
        }
    }

    /// Set the display name.
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Add a header-level parameter. The key is lowercased on insertion.
    pub fn with_param(mut self, key: impl Into<String>, value: Option<impl Into<String>>) -> Self {
        self.params
            .push((
                key.into()
                    .to_ascii_lowercase(),
                value.map(Into::into),
            ));
        self
    }

    /// The display name, if present.
    pub fn display_name(&self) -> Option<&str> {
        self.display_name
            .as_deref()
    }

    /// The URI.
    pub fn uri(&self) -> &sip_uri::Uri {
        &self.uri
    }

    /// If the URI is a SIP/SIPS URI, return a reference to it.
    pub fn sip_uri(&self) -> Option<&sip_uri::SipUri> {
        self.uri
            .as_sip()
    }

    /// If the URI is a tel: URI, return a reference to it.
    pub fn tel_uri(&self) -> Option<&sip_uri::TelUri> {
        self.uri
            .as_tel()
    }

    /// If the URI is a URN, return a reference to it.
    pub fn urn_uri(&self) -> Option<&sip_uri::UrnUri> {
        self.uri
            .as_urn()
    }

    /// Iterator over header-level parameters as `(key, value)` pairs.
    /// Keys are lowercased; values are preserved as-is.
    pub fn params(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_deref()))
    }

    /// Look up a header-level parameter by name (case-insensitive).
    ///
    /// Returns `Some(Some(value))` if the param has a value,
    /// `Some(None)` if it is a flag param (no value),
    /// `None` if the param is not present.
    pub fn param(&self, name: &str) -> Option<Option<&str>> {
        let needle = name.to_ascii_lowercase();
        self.params
            .iter()
            .find(|(k, _)| *k == needle)
            .map(|(_, v)| v.as_deref())
    }

    /// The `tag` parameter value, if present.
    pub fn tag(&self) -> Option<&str> {
        self.param("tag")
            .flatten()
    }
}

/// Parse a quoted string, returning (unescaped content, rest after closing quote).
fn parse_quoted_string(s: &str) -> Result<(String, &str), String> {
    if !s.starts_with('"') {
        return Err("expected opening quote".into());
    }

    let mut result = String::new();
    let mut chars = s[1..].char_indices();

    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                return Ok((result, &s[i + 2..]));
            }
            '\\' => {
                let (_, escaped) = chars
                    .next()
                    .ok_or("unterminated escape in quoted string")?;
                result.push(escaped);
            }
            _ => {
                result.push(c);
            }
        }
    }

    Err("unterminated quoted string".into())
}

/// Extract the URI from `<...>`, returning `(uri_str, rest_after_>)`.
fn extract_angle_uri(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('<')?;
    let end = s.find('>')?;
    Some((&s[..end], &s[end + 1..]))
}

/// Parse header-level parameters from the trailing portion after `>`.
/// Values are percent-decoded per SIP header rules (RFC 3261 S25).
fn parse_header_params(s: &str) -> Vec<(String, Option<String>)> {
    let mut params = Vec::new();
    for segment in s.split(';') {
        if segment.is_empty() {
            continue;
        }
        if let Some((key, value)) = segment.split_once('=') {
            params.push((
                key.to_ascii_lowercase(),
                Some(
                    percent_decode_str(value)
                        .decode_utf8_lossy()
                        .into_owned(),
                ),
            ));
        } else {
            params.push((segment.to_ascii_lowercase(), None));
        }
    }
    params
}

/// Check if a display name needs quoting (contains SIP special chars or whitespace).
fn needs_quoting(name: &str) -> bool {
    name.bytes()
        .any(|b| {
            matches!(
                b,
                b'"' | b'\\' | b'<' | b'>' | b',' | b';' | b':' | b'@' | b' ' | b'\t'
            )
        })
}

/// Escape a display name for use within double quotes.
fn escape_display_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl FromStr for SipHeaderAddr {
    type Err = ParseSipHeaderAddrError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let err = |msg: &str| ParseSipHeaderAddrError(msg.to_string());
        let s = input.trim();

        if s.is_empty() {
            return Err(err("empty input"));
        }

        // Case 1: quoted display name followed by <URI> and optional params
        if s.starts_with('"') {
            let (display_name, rest) = parse_quoted_string(s).map_err(|e| err(&e))?;
            let rest = rest.trim_start();
            let (uri_str, trailing) = extract_angle_uri(rest)
                .ok_or_else(|| err("expected '<URI>' after quoted display name"))?;
            let uri: sip_uri::Uri = uri_str.parse()?;
            let display_name = if display_name.is_empty() {
                None
            } else {
                Some(display_name)
            };
            let params = parse_header_params(trailing);
            return Ok(SipHeaderAddr {
                display_name,
                uri,
                params,
            });
        }

        // Case 2: <URI> without display name, with optional params
        if s.starts_with('<') {
            let (uri_str, trailing) = extract_angle_uri(s).ok_or_else(|| err("unclosed '<'"))?;
            let uri: sip_uri::Uri = uri_str.parse()?;
            let params = parse_header_params(trailing);
            return Ok(SipHeaderAddr {
                display_name: None,
                uri,
                params,
            });
        }

        // Case 3: unquoted display name followed by <URI> and optional params
        if let Some(angle_start) = s.find('<') {
            let display_name = s[..angle_start].trim();
            let display_name = if display_name.is_empty() {
                None
            } else {
                Some(display_name.to_string())
            };
            let (uri_str, trailing) =
                extract_angle_uri(&s[angle_start..]).ok_or_else(|| err("unclosed '<'"))?;
            let uri: sip_uri::Uri = uri_str.parse()?;
            let params = parse_header_params(trailing);
            return Ok(SipHeaderAddr {
                display_name,
                uri,
                params,
            });
        }

        // Case 4: bare addr-spec (no angle brackets, no display name)
        // RFC 3261 mandates angle brackets when URI has params, so all
        // ;params are parsed as part of the URI itself.
        let uri: sip_uri::Uri = s.parse()?;
        Ok(SipHeaderAddr {
            display_name: None,
            uri,
            params: Vec::new(),
        })
    }
}

impl fmt::Display for SipHeaderAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self
            .display_name
            .as_deref()
        {
            Some(name) if !name.is_empty() => {
                if needs_quoting(name) {
                    write!(f, "\"{}\" ", escape_display_name(name))?;
                } else {
                    write!(f, "{name} ")?;
                }
                write!(f, "<{}>", self.uri)?;
            }
            _ => {
                write!(f, "<{}>", self.uri)?;
            }
        }
        for (key, value) in &self.params {
            match value {
                Some(v) => write!(f, ";{key}={v}")?,
                None => write!(f, ";{key}")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_display_name_with_tag() {
        let addr: SipHeaderAddr = r#""Alice" <sip:alice@example.com>;tag=abc123"#
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), Some("Alice"));
        assert!(addr
            .sip_uri()
            .is_some());
        assert_eq!(addr.tag(), Some("abc123"));
    }

    #[test]
    fn angle_bracket_no_name_multiple_params() {
        let addr: SipHeaderAddr = "<sip:user@host>;tag=xyz;expires=3600"
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), None);
        assert_eq!(addr.tag(), Some("xyz"));
        assert_eq!(addr.param("expires"), Some(Some("3600")));
    }

    #[test]
    fn bare_addr_spec_no_params() {
        let addr: SipHeaderAddr = "sip:user@host"
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), None);
        assert!(addr
            .sip_uri()
            .is_some());
        assert_eq!(
            addr.params()
                .count(),
            0
        );
    }

    #[test]
    fn unquoted_display_name_with_params() {
        let addr: SipHeaderAddr = "Alice <sip:alice@example.com>;tag=abc"
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), Some("Alice"));
        assert_eq!(addr.tag(), Some("abc"));
    }

    #[test]
    fn ng911_refer_to_serviceurn() {
        let input = "<sip:user@esrp.example.com?Call-Info=x>;serviceurn=urn%3Aservice%3Apolice";
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), None);
        assert_eq!(addr.param("serviceurn"), Some(Some("urn:service:police")));
        let sip = addr
            .sip_uri()
            .unwrap();
        assert_eq!(
            sip.host()
                .to_string(),
            "esrp.example.com"
        );
    }

    #[test]
    fn p_asserted_identity_uri_params_no_header_params() {
        let input = r#""EXAMPLE CO" <sip:+15551234567;cpc=emergency@198.51.100.1;user=phone>"#;
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), Some("EXAMPLE CO"));
        assert_eq!(
            addr.params()
                .count(),
            0
        );
        let sip = addr
            .sip_uri()
            .unwrap();
        assert_eq!(sip.user(), Some("+15551234567"));
        assert_eq!(sip.param("user"), Some(&Some("phone".to_string())));
    }

    #[test]
    fn tel_uri_with_header_params() {
        let addr: SipHeaderAddr = "<tel:+15551234567>;expires=3600"
            .parse()
            .unwrap();
        assert!(addr
            .tel_uri()
            .is_some());
        assert_eq!(addr.param("expires"), Some(Some("3600")));
    }

    #[test]
    fn flag_param_no_value() {
        let addr: SipHeaderAddr = "<sip:user@host>;lr;tag=abc"
            .parse()
            .unwrap();
        assert_eq!(addr.param("lr"), Some(None));
        assert_eq!(addr.tag(), Some("abc"));
    }

    #[test]
    fn urn_uri_no_params() {
        let addr: SipHeaderAddr = "<urn:service:sos>"
            .parse()
            .unwrap();
        assert!(addr
            .urn_uri()
            .is_some());
        assert_eq!(
            addr.params()
                .count(),
            0
        );
    }

    #[test]
    fn empty_input_fails() {
        assert!(""
            .parse::<SipHeaderAddr>()
            .is_err());
    }

    #[test]
    fn display_roundtrip_quoted_name_with_params() {
        // "Alice" doesn't need quoting, so Display normalizes to unquoted
        let input = r#""Alice" <sip:alice@example.com>;tag=abc123"#;
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.to_string(), "Alice <sip:alice@example.com>;tag=abc123");
    }

    #[test]
    fn display_roundtrip_name_requiring_quotes() {
        let input = r#""Alice Smith" <sip:alice@example.com>;tag=abc123"#;
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.to_string(), input);
    }

    #[test]
    fn display_roundtrip_no_name_with_params() {
        let input = "<sip:user@host>;tag=xyz;expires=3600";
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.to_string(), input);
    }

    #[test]
    fn display_roundtrip_bare_uri() {
        let input = "sip:user@host";
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        // Bare URIs get angle-bracketed in Display
        assert_eq!(addr.to_string(), "<sip:user@host>");
    }

    #[test]
    fn display_roundtrip_flag_param() {
        let input = "<sip:user@host>;lr;tag=abc";
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.to_string(), input);
    }

    #[test]
    fn case_insensitive_param_lookup() {
        let addr: SipHeaderAddr = "<sip:user@host>;Tag=ABC;Expires=3600"
            .parse()
            .unwrap();
        assert_eq!(addr.param("tag"), Some(Some("ABC")));
        assert_eq!(addr.param("TAG"), Some(Some("ABC")));
        assert_eq!(addr.param("expires"), Some(Some("3600")));
    }

    #[test]
    fn tag_convenience_accessor() {
        let with_tag: SipHeaderAddr = "<sip:user@host>;tag=xyz"
            .parse()
            .unwrap();
        assert_eq!(with_tag.tag(), Some("xyz"));

        let without_tag: SipHeaderAddr = "<sip:user@host>"
            .parse()
            .unwrap();
        assert_eq!(without_tag.tag(), None);
    }

    #[test]
    fn builder_new() {
        let uri: sip_uri::Uri = "sip:alice@example.com"
            .parse()
            .unwrap();
        let addr = SipHeaderAddr::new(uri);
        assert_eq!(addr.display_name(), None);
        assert_eq!(
            addr.params()
                .count(),
            0
        );
        assert_eq!(addr.to_string(), "<sip:alice@example.com>");
    }

    #[test]
    fn builder_with_display_name_and_params() {
        let uri: sip_uri::Uri = "sip:alice@example.com"
            .parse()
            .unwrap();
        let addr = SipHeaderAddr::new(uri)
            .with_display_name("Alice")
            .with_param("tag", Some("abc123"));
        assert_eq!(addr.display_name(), Some("Alice"));
        assert_eq!(addr.tag(), Some("abc123"));
        assert_eq!(addr.to_string(), "Alice <sip:alice@example.com>;tag=abc123");
    }

    #[test]
    fn builder_flag_param() {
        let uri: sip_uri::Uri = "sip:proxy@example.com"
            .parse()
            .unwrap();
        let addr = SipHeaderAddr::new(uri).with_param("lr", None::<String>);
        assert_eq!(addr.param("lr"), Some(None));
        assert_eq!(addr.to_string(), "<sip:proxy@example.com>;lr");
    }

    #[test]
    fn escaped_quotes_in_display_name() {
        let input = r#""Say \"Hello\"" <sip:u@h>;tag=t"#;
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.display_name(), Some(r#"Say "Hello""#));
        assert_eq!(addr.tag(), Some("t"));
    }

    #[test]
    fn display_roundtrip_escaped_quotes() {
        let input = r#""Say \"Hello\"" <sip:u@h>;tag=t"#;
        let addr: SipHeaderAddr = input
            .parse()
            .unwrap();
        assert_eq!(addr.to_string(), input);
    }

    #[test]
    fn trailing_semicolon_ignored() {
        let addr: SipHeaderAddr = "<sip:user@host>;tag=abc;"
            .parse()
            .unwrap();
        assert_eq!(
            addr.params()
                .count(),
            1
        );
        assert_eq!(addr.tag(), Some("abc"));
    }

    #[test]
    fn params_iterator() {
        let addr: SipHeaderAddr = "<sip:user@host>;tag=abc;lr;expires=60"
            .parse()
            .unwrap();
        let params: Vec<_> = addr
            .params()
            .collect();
        assert_eq!(params.len(), 3);
        assert_eq!(params[0], ("tag", Some("abc")));
        assert_eq!(params[1], ("lr", None));
        assert_eq!(params[2], ("expires", Some("60")));
    }
}
