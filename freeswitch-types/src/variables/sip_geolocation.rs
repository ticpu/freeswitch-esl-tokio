use std::fmt;

/// A reference extracted from a SIP Geolocation header (RFC 6442).
///
/// Each entry is either a `cid:` reference to a multipart body part
/// (typically containing PIDF-LO XML) or a URL for location dereference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SipGeolocationRef {
    /// Content-ID reference to a MIME body part (e.g., `cid:uuid`).
    Cid(String),
    /// HTTP(S) or other URL for location dereference.
    Url(String),
}

impl fmt::Display for SipGeolocationRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cid(id) => write!(f, "<cid:{id}>"),
            Self::Url(url) => write!(f, "<{url}>"),
        }
    }
}

/// Parsed SIP Geolocation header value (RFC 6442).
///
/// Contains one or more `<uri>` references, comma-separated. Each reference
/// is classified as either a `cid:` body-part reference or a dereference URL.
///
/// ```
/// use freeswitch_types::variables::SipGeolocation;
///
/// let raw = "<cid:abc-123>, <https://lis.example.com/held/abc>";
/// let geo = SipGeolocation::parse(raw);
/// assert_eq!(geo.len(), 2);
/// assert!(geo.cid().is_some());
/// assert!(geo.url().is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SipGeolocation(Vec<SipGeolocationRef>);

impl SipGeolocation {
    /// Parse a raw Geolocation header value into typed references.
    pub fn parse(raw: &str) -> Self {
        let refs = raw
            .split(',')
            .filter_map(|entry| {
                let entry = entry.trim();
                let inner = entry
                    .strip_prefix('<')?
                    .strip_suffix('>')?;
                if inner.is_empty() {
                    return None;
                }
                if let Some(id) = inner.strip_prefix("cid:") {
                    Some(SipGeolocationRef::Cid(id.to_string()))
                } else {
                    Some(SipGeolocationRef::Url(inner.to_string()))
                }
            })
            .collect();
        Self(refs)
    }

    /// The parsed references as a slice.
    pub fn refs(&self) -> &[SipGeolocationRef] {
        &self.0
    }

    /// Number of references.
    pub fn len(&self) -> usize {
        self.0
            .len()
    }

    /// Returns `true` if there are no references.
    pub fn is_empty(&self) -> bool {
        self.0
            .is_empty()
    }

    /// The first `cid:` reference, if any.
    pub fn cid(&self) -> Option<&str> {
        self.0
            .iter()
            .find_map(|r| match r {
                SipGeolocationRef::Cid(id) => Some(id.as_str()),
                _ => None,
            })
    }

    /// The first URL reference, if any.
    pub fn url(&self) -> Option<&str> {
        self.0
            .iter()
            .find_map(|r| match r {
                SipGeolocationRef::Url(url) => Some(url.as_str()),
                _ => None,
            })
    }

    /// Iterate over all `cid:` references.
    pub fn cids(&self) -> impl Iterator<Item = &str> {
        self.0
            .iter()
            .filter_map(|r| match r {
                SipGeolocationRef::Cid(id) => Some(id.as_str()),
                _ => None,
            })
    }

    /// Iterate over all URL references.
    pub fn urls(&self) -> impl Iterator<Item = &str> {
        self.0
            .iter()
            .filter_map(|r| match r {
                SipGeolocationRef::Url(url) => Some(url.as_str()),
                _ => None,
            })
    }
}

impl fmt::Display for SipGeolocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, r) in self
            .0
            .iter()
            .enumerate()
        {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{r}")?;
        }
        Ok(())
    }
}

impl<'a> IntoIterator for &'a SipGeolocation {
    type Item = &'a SipGeolocationRef;
    type IntoIter = std::slice::Iter<'a, SipGeolocationRef>;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .iter()
    }
}

impl IntoIterator for SipGeolocation {
    type Item = SipGeolocationRef;
    type IntoIter = std::vec::IntoIter<SipGeolocationRef>;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cid_and_url() {
        let raw = "<cid:32863354-18b4-4069-bd00-7bced5fc6c9b>, <https://lis.example.com/api/v1/held/test>";
        let geo = SipGeolocation::parse(raw);
        assert_eq!(geo.len(), 2);
        assert_eq!(geo.cid(), Some("32863354-18b4-4069-bd00-7bced5fc6c9b"));
        assert!(geo
            .url()
            .unwrap()
            .contains("lis.example.com"));
    }

    #[test]
    fn single_cid() {
        let geo = SipGeolocation::parse("<cid:abc-123>");
        assert_eq!(geo.len(), 1);
        assert_eq!(geo.cid(), Some("abc-123"));
        assert!(geo
            .url()
            .is_none());
    }

    #[test]
    fn single_url() {
        let geo = SipGeolocation::parse("<https://lis.example.com/location>");
        assert_eq!(geo.len(), 1);
        assert!(geo
            .cid()
            .is_none());
        assert_eq!(geo.url(), Some("https://lis.example.com/location"));
    }

    #[test]
    fn empty_input() {
        let geo = SipGeolocation::parse("");
        assert!(geo.is_empty());
    }

    #[test]
    fn empty_brackets_skipped() {
        let geo = SipGeolocation::parse("<>, <cid:test>");
        assert_eq!(geo.len(), 1);
        assert_eq!(geo.cid(), Some("test"));
    }

    #[test]
    fn display_roundtrip() {
        let raw = "<cid:abc-123>, <https://lis.example.com/test>";
        let geo = SipGeolocation::parse(raw);
        assert_eq!(geo.to_string(), raw);
    }

    #[test]
    fn multiple_cids() {
        let raw = "<cid:first>, <cid:second>, <https://example.com/loc>";
        let geo = SipGeolocation::parse(raw);
        let cids: Vec<_> = geo
            .cids()
            .collect();
        assert_eq!(cids, vec!["first", "second"]);
        let urls: Vec<_> = geo
            .urls()
            .collect();
        assert_eq!(urls, vec!["https://example.com/loc"]);
    }
}
