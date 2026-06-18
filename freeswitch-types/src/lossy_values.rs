//! Types for tracking event headers whose values contained invalid UTF-8.

use std::fmt;

/// A header whose percent-decoded value was not valid UTF-8. Carries the
/// unparsed on-wire value (the percent-encoded source text, always ASCII) so
/// the app can re-decode it (e.g. as Latin-1) or audit it instead of being
/// stuck with the U+FFFD-substituted string in `headers`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LossyValue {
    key: String,
    raw_value: String,
}

impl LossyValue {
    /// Create a new lossy value entry.
    pub fn new(key: String, raw_value: String) -> Self {
        Self { key, raw_value }
    }

    /// The header key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The on-wire percent-encoded value before decode.
    pub fn raw_value(&self) -> &str {
        &self.raw_value
    }
}

/// Header keys whose percent-decoded value contained invalid UTF-8 and was
/// decoded lossily (U+FFFD substituted). Empty in the normal case.
///
/// `Display` renders the affected keys comma-separated for log/error context;
/// it never includes the values, which may carry PII (e.g. a dialed number).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct LossyValues(Vec<LossyValue>);

impl LossyValues {
    /// Returns `true` if no headers had lossy decoding.
    pub fn is_empty(&self) -> bool {
        self.0
            .is_empty()
    }

    /// Iterator over lossy values.
    pub fn iter(&self) -> impl Iterator<Item = &LossyValue> {
        self.0
            .iter()
    }

    /// Add a lossy value entry.
    pub fn push(&mut self, v: LossyValue) {
        self.0
            .push(v);
    }
}

impl fmt::Display for LossyValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self
            .0
            .iter()
            .map(|v| v.key())
            .collect();
        write!(f, "{}", keys.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_values_display_keys_only() {
        let mut lossy = LossyValues::default();
        lossy.push(LossyValue::new(
            "variable_dp_match".to_string(),
            "%E9foo".to_string(),
        ));
        lossy.push(LossyValue::new(
            "another_key".to_string(),
            "%FFbar".to_string(),
        ));

        let display = lossy.to_string();
        assert_eq!(display, "variable_dp_match, another_key");
        assert!(!display.contains("%E9"));
        assert!(!display.contains("foo"));
    }

    #[test]
    fn lossy_values_empty() {
        let lossy = LossyValues::default();
        assert!(lossy.is_empty());
        assert_eq!(
            lossy
                .iter()
                .count(),
            0
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut lossy = LossyValues::default();
        lossy.push(LossyValue::new("key1".to_string(), "%E9value".to_string()));

        let json = serde_json::to_string(&lossy).unwrap();
        let deserialized: LossyValues = serde_json::from_str(&json).unwrap();

        assert_eq!(lossy, deserialized);
        assert_eq!(
            deserialized
                .iter()
                .next()
                .unwrap()
                .key(),
            "key1"
        );
        assert_eq!(
            deserialized
                .iter()
                .next()
                .unwrap()
                .raw_value(),
            "%E9value"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_old_json_without_field() {
        let json = "[]";
        let lossy: LossyValues = serde_json::from_str(json).unwrap();
        assert!(lossy.is_empty());
    }
}
