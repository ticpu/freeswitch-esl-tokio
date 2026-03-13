use std::fmt;

use super::EslArray;

/// One entry from a SIP Call-Info header: `<uri>;key=value;key=value`.
///
/// The data field contains the URI stripped of angle brackets.
/// Metadata keys are stored lowercased; values are preserved as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SipCallInfoEntry {
    /// The URI or data inside the angle brackets, with brackets stripped.
    pub data: String,
    /// Semicolon-delimited parameters as `(key, value)` pairs.
    /// Keys are lowercased at parse time; values are preserved as-is.
    /// A key with no `=` sign is stored with an empty string value.
    pub metadata: Vec<(String, String)>,
}

impl SipCallInfoEntry {
    /// Look up a metadata parameter by key (case-insensitive).
    pub fn param(&self, key: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find_map(|(k, v)| {
                if k.eq_ignore_ascii_case(key) {
                    Some(v.as_str())
                } else {
                    None
                }
            })
    }

    /// The `purpose` parameter value, if present.
    pub fn purpose(&self) -> Option<&str> {
        self.param("purpose")
    }

    /// Check if the `purpose` parameter ends with the given suffix
    /// (case-insensitive). This is the standard way to match NENA purpose
    /// values across namespace prefixes (`nena-CallId`, `emergency-CallId`).
    pub fn purpose_ends_with(&self, suffix: &str) -> bool {
        self.purpose()
            .is_some_and(|p| {
                p.to_ascii_lowercase()
                    .ends_with(&suffix.to_ascii_lowercase())
            })
    }
}

impl fmt::Display for SipCallInfoEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.data)?;
        for (key, value) in &self.metadata {
            if value.is_empty() {
                write!(f, ";{key}")?;
            } else {
                write!(f, ";{key}={value}")?;
            }
        }
        Ok(())
    }
}

/// Parsed SIP Call-Info header value. Contains zero or more entries.
///
/// Accepts both standard comma-separated SIP format and FreeSWITCH
/// `ARRAY::|:` encoding.
///
/// ```
/// use freeswitch_types::variables::SipCallInfo;
///
/// let raw = "<urn:example:call:123>;purpose=emergency-CallId,<https://example.com/data>;purpose=EmergencyCallData.ServiceInfo";
/// let info = SipCallInfo::parse(raw).unwrap();
/// assert_eq!(info.entries().len(), 2);
/// assert_eq!(info.entries()[0].purpose(), Some("emergency-CallId"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SipCallInfo(Vec<SipCallInfoEntry>);

/// Errors from parsing a SIP Call-Info header value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SipCallInfoError {
    /// The input string was empty or whitespace-only.
    Empty,
    /// An entry was found without angle brackets around the URI.
    MissingAngleBrackets(String),
}

impl fmt::Display for SipCallInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty Call-Info header"),
            Self::MissingAngleBrackets(raw) => {
                write!(f, "missing angle brackets in Call-Info entry: {raw}")
            }
        }
    }
}

impl std::error::Error for SipCallInfoError {}

fn parse_entry(raw: &str) -> Result<SipCallInfoEntry, SipCallInfoError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(SipCallInfoError::MissingAngleBrackets(raw.to_string()));
    }

    // Split on first ';' to separate data from metadata, like c911p does.
    // This avoids issues with ';' inside URIs before the metadata section.
    let (data_part, metadata_part) = match raw.split_once(';') {
        Some((d, m)) => (d, Some(m)),
        None => (raw, None),
    };

    let data = data_part
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    if data.is_empty() {
        return Err(SipCallInfoError::MissingAngleBrackets(raw.to_string()));
    }

    let mut metadata = Vec::new();
    if let Some(meta_str) = metadata_part {
        if !meta_str.is_empty() {
            for segment in meta_str.split(';') {
                let segment = segment.trim();
                if segment.is_empty() {
                    continue;
                }
                if let Some((key, value)) = segment.split_once('=') {
                    metadata.push((
                        key.trim()
                            .to_ascii_lowercase(),
                        value
                            .trim()
                            .to_string(),
                    ));
                } else {
                    metadata.push((segment.to_ascii_lowercase(), String::new()));
                }
            }
        }
    }

    Ok(SipCallInfoEntry { data, metadata })
}

use super::split_comma_entries;

impl SipCallInfo {
    /// Parse a raw Call-Info header value. Handles both standard
    /// comma-separated SIP format and FreeSWITCH `ARRAY::|:` encoding.
    pub fn parse(raw: &str) -> Result<Self, SipCallInfoError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(SipCallInfoError::Empty);
        }

        let entries = if let Some(arr) = EslArray::parse(raw) {
            arr.into_iter()
                .map(|s| parse_entry(&s))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            split_comma_entries(raw)
                .into_iter()
                .map(parse_entry)
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(Self(entries))
    }

    /// The parsed entries as a slice.
    pub fn entries(&self) -> &[SipCallInfoEntry] {
        &self.0
    }

    /// Consume self and return the entries as a `Vec`.
    pub fn into_entries(self) -> Vec<SipCallInfoEntry> {
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

    /// Find the first entry whose `purpose` parameter ends with the given
    /// suffix (case-insensitive).
    pub fn find_by_purpose_suffix(&self, suffix: &str) -> Option<&SipCallInfoEntry> {
        self.0
            .iter()
            .find(|e| e.purpose_ends_with(suffix))
    }

    /// Iterate over entries whose `purpose` parameter ends with the given
    /// suffix (case-insensitive).
    pub fn filter_by_purpose_suffix<'a>(
        &'a self,
        suffix: &'a str,
    ) -> impl Iterator<Item = &'a SipCallInfoEntry> {
        self.0
            .iter()
            .filter(move |e| e.purpose_ends_with(suffix))
    }
}

impl fmt::Display for SipCallInfo {
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

impl<'a> IntoIterator for &'a SipCallInfo {
    type Item = &'a SipCallInfoEntry;
    type IntoIter = std::slice::Iter<'a, SipCallInfoEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .iter()
    }
}

impl IntoIterator for SipCallInfo {
    type Item = SipCallInfoEntry;
    type IntoIter = std::vec::IntoIter<SipCallInfoEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- SipCallInfoEntry tests (mirrors c911p TestChannelXCallInfoItem) --

    #[test]
    fn entry_no_metadata() {
        let entry = parse_entry("<data>").unwrap();
        assert_eq!(entry.data, "data");
        assert!(entry
            .metadata
            .is_empty());
    }

    #[test]
    fn entry_no_metadata_trailing_semicolon() {
        let entry = parse_entry("<data>;").unwrap();
        assert_eq!(entry.data, "data");
        assert!(entry
            .metadata
            .is_empty());
    }

    #[test]
    fn entry_no_value_metadata() {
        let entry = parse_entry("<data>;meta1").unwrap();
        assert_eq!(
            entry
                .metadata
                .len(),
            1
        );
        assert_eq!(entry.metadata[0], ("meta1".to_string(), String::new()));
    }

    #[test]
    fn entry_empty_value_metadata() {
        let entry = parse_entry("<data>;meta1=").unwrap();
        assert_eq!(
            entry
                .metadata
                .len(),
            1
        );
        assert_eq!(entry.metadata[0], ("meta1".to_string(), String::new()));
    }

    #[test]
    fn entry_two_metadata_items() {
        let entry = parse_entry("<data>;meta1=one;meta2=two;").unwrap();
        assert_eq!(entry.data, "data");
        assert_eq!(
            entry
                .metadata
                .len(),
            2
        );
        assert_eq!(entry.param("meta1"), Some("one"));
        assert_eq!(entry.param("meta2"), Some("two"));
    }

    #[test]
    fn entry_strips_angle_brackets() {
        let entry = parse_entry("<data>;meta1=one;meta2=two;").unwrap();
        assert_eq!(entry.data, "data");
    }

    #[test]
    fn entry_uppercase_metadata_key_lowercased() {
        let entry = parse_entry("<data>;Meta-1=one").unwrap();
        assert!(entry
            .metadata
            .iter()
            .all(|(k, _)| k == &k.to_ascii_lowercase()));
        assert_eq!(entry.param("meta-1"), Some("one"));
    }

    #[test]
    fn entry_display_no_trailing_semicolon() {
        let entry = parse_entry("<data>;").unwrap();
        let s = entry.to_string();
        assert!(!s.ends_with(';'));
    }

    #[test]
    fn entry_display_metadata_no_trailing_semicolon() {
        let entry = parse_entry("<data>;meta=one;").unwrap();
        let s = entry.to_string();
        assert!(!s.ends_with(';'));
    }

    #[test]
    fn entry_display_contains_all_metadata() {
        let entry = parse_entry("<http://somedata/?arg=123>").unwrap();
        // Build entry with metadata manually since the URL contains ? and =
        let mut entry = entry;
        entry
            .metadata
            .push(("meta1".to_string(), "one".to_string()));
        entry
            .metadata
            .push(("meta2".to_string(), "two".to_string()));
        let s = entry.to_string();
        assert!(
            s.matches(';')
                .count()
                >= 2
        );
    }

    #[test]
    fn entry_display_no_value_key() {
        let entry = parse_entry("<data>;flagkey").unwrap();
        assert_eq!(entry.to_string(), "<data>;flagkey");
    }

    // -- SipCallInfo tests (mirrors c911p TestChannelXCallInfo) --

    const SAMPLE_EMERGENCY: &str = "\
<urn:emergency:uid:callid:20250401080740945abc123:bcf.example.com>;purpose=emergency-CallId,\
<urn:emergency:uid:incidentid:20250401080740945def456:bcf.example.com>;purpose=emergency-IncidentId,\
<https://adr.example.com/api/v1/adr/call/providerInfo/access?token=abc>;purpose=EmergencyCallData.ProviderInfo,\
<https://adr.example.com/api/v1/adr/call/serviceInfo?token=ghi>;purpose=EmergencyCallData.ServiceInfo";

    const SAMPLE_WITH_SITE: &str = "\
<urn:emergency:uid:callid:test:bcf.example.com>;purpose=emergency-CallId;site=bcf.example.com,\
<urn:emergency:uid:incidentid:test:bcf.example.com>;purpose=emergency-IncidentId";

    const SAMPLE_ARRAY: &str = "ARRAY::\
<urn:emergency:uid:callid:test123:bcf.example.com>;purpose=emergency-CallId|:\
<urn:emergency:uid:incidentid:test456:bcf.example.com>;purpose=emergency-IncidentId";

    // 8-entry fixture exercising legacy nena- prefix, EIDO purpose, trailing
    // semicolons, site param, and all 5 ADR subtypes.
    const SAMPLE_FULL: &str = "\
<urn:nena:callid:20190912100022147abc:bcf1.example.com>;purpose=nena-CallId,\
<https://eido.psap.example.com/EidoRetrievalService/urn:nena:incidentid:test>;purpose=emergency_incident_data_object,\
<urn:nena:incidentid:20190912100022147def:bcf1.example.com>;purpose=nena-IncidentId,\
<https://adr.example.com/api/v1/adr/call/providerInfo/access?token=a>;purpose=EmergencyCallData.ProviderInfo,\
<https://adr.example.com/api/v1/adr/call/providerInfo/telecom?token=b>;purpose=EmergencyCallData.ProviderInfo;site=bcf.example.com;,\
<https://adr.example.com/api/v1/adr/call/serviceInfo?token=c>;purpose=EmergencyCallData.ServiceInfo,\
<https://adr.example.com/api/v1/adr/call/subscriberInfo?token=d>;purpose=EmergencyCallData.SubscriberInfo,\
<https://adr.example.com/api/v1/adr/call/comment?token=e>;purpose=EmergencyCallData.Comment";

    #[test]
    fn parse_comma_separated() {
        let info = SipCallInfo::parse(SAMPLE_EMERGENCY).unwrap();
        assert_eq!(info.len(), 4);
        assert_eq!(info.entries()[0].purpose(), Some("emergency-CallId"));
        assert_eq!(info.entries()[1].purpose(), Some("emergency-IncidentId"));
    }

    #[test]
    fn parse_array_format() {
        let info = SipCallInfo::parse(SAMPLE_ARRAY).unwrap();
        assert_eq!(info.len(), 2);
        assert!(info.entries()[0]
            .data
            .contains("callid"));
        assert!(info.entries()[1]
            .data
            .contains("incidentid"));
    }

    #[test]
    fn parse_full_fixture_all_entries() {
        let info = SipCallInfo::parse(SAMPLE_FULL).unwrap();
        assert_eq!(info.len(), 8);
    }

    #[test]
    fn full_fixture_nena_prefix_callid() {
        let info = SipCallInfo::parse(SAMPLE_FULL).unwrap();
        let entry = info
            .find_by_purpose_suffix("-CallId")
            .unwrap();
        assert!(entry
            .data
            .contains("callid"));
        assert_eq!(entry.purpose(), Some("nena-CallId"));
    }

    #[test]
    fn full_fixture_legacy_eido_purpose() {
        let info = SipCallInfo::parse(SAMPLE_FULL).unwrap();
        let eido: Vec<_> = info
            .entries()
            .iter()
            .filter(|e| {
                e.purpose()
                    .is_some_and(|p| p.contains("incident_data_object"))
            })
            .collect();
        assert_eq!(eido.len(), 1);
        assert!(eido[0]
            .data
            .contains("EidoRetrievalService"));
    }

    #[test]
    fn full_fixture_trailing_semicolon_with_site() {
        let info = SipCallInfo::parse(SAMPLE_FULL).unwrap();
        let with_site: Vec<_> = info
            .entries()
            .iter()
            .filter(|e| {
                e.param("site")
                    .is_some()
            })
            .collect();
        assert_eq!(with_site.len(), 1);
        assert_eq!(with_site[0].param("site"), Some("bcf.example.com"));
    }

    #[test]
    fn find_by_purpose_suffix() {
        let info = SipCallInfo::parse(SAMPLE_EMERGENCY).unwrap();

        let call_id = info
            .find_by_purpose_suffix("-CallId")
            .unwrap();
        assert!(call_id
            .data
            .contains("callid"));

        let incident = info
            .find_by_purpose_suffix("-IncidentId")
            .unwrap();
        assert!(incident
            .data
            .contains("incidentid"));

        assert!(info
            .find_by_purpose_suffix("-Nonexistent")
            .is_none());
    }

    #[test]
    fn suffix_match_across_prefixes() {
        let legacy = "<urn:nena:callid:test:example.ca>;purpose=nena-CallId";
        let info = SipCallInfo::parse(legacy).unwrap();
        assert!(info
            .find_by_purpose_suffix("-CallId")
            .is_some());

        let modern = "<urn:emergency:uid:callid:test:example.ca>;purpose=emergency-CallId";
        let info = SipCallInfo::parse(modern).unwrap();
        assert!(info
            .find_by_purpose_suffix("-CallId")
            .is_some());
    }

    #[test]
    fn filter_multiple_matches() {
        let info = SipCallInfo::parse(SAMPLE_EMERGENCY).unwrap();
        let adr: Vec<_> = info
            .filter_by_purpose_suffix("Info")
            .collect();
        assert_eq!(adr.len(), 2);
    }

    #[test]
    fn filter_all_adr_in_full_fixture() {
        let info = SipCallInfo::parse(SAMPLE_FULL).unwrap();
        let adr: Vec<_> = info
            .filter_by_purpose_suffix("Info")
            .collect();
        assert_eq!(adr.len(), 4); // 2x ProviderInfo + ServiceInfo + SubscriberInfo
    }

    #[test]
    fn metadata_param_lookup() {
        let info = SipCallInfo::parse(SAMPLE_WITH_SITE).unwrap();
        assert_eq!(info.entries()[0].param("site"), Some("bcf.example.com"));
        assert_eq!(info.entries()[0].param("purpose"), Some("emergency-CallId"));
        assert!(info.entries()[1]
            .param("site")
            .is_none());
    }

    #[test]
    fn display_roundtrip() {
        let raw = "<urn:example:test>;purpose=test-purpose;site=example.com";
        let info = SipCallInfo::parse(raw).unwrap();
        assert_eq!(info.to_string(), raw);
    }

    #[test]
    fn display_comma_count_matches_entries() {
        let info = SipCallInfo::parse(SAMPLE_EMERGENCY).unwrap();
        let s = info.to_string();
        assert_eq!(
            s.matches(',')
                .count()
                + 1,
            info.len()
        );
    }

    #[test]
    fn empty_input() {
        assert!(matches!(
            SipCallInfo::parse(""),
            Err(SipCallInfoError::Empty)
        ));
    }
}
