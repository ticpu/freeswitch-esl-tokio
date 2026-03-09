use std::fmt;
use std::str::FromStr;

/// Notification state for partial updates (RFC 4575 Section 4.1).
///
/// Used on `ConferenceInfo`, `Users`, `User`, `Endpoint`, `SidebarsByRef`,
/// and `SidebarsByVal` to indicate whether the document is a full snapshot,
/// a partial delta, or a deletion notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum State {
    /// Complete state — replaces any previously cached document.
    #[cfg_attr(feature = "serde", serde(rename = "full"))]
    Full,
    /// Incremental update — merge with previously cached state.
    #[cfg_attr(feature = "serde", serde(rename = "partial"))]
    Partial,
    /// Entity has been removed.
    #[cfg_attr(feature = "serde", serde(rename = "deleted"))]
    Deleted,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::Deleted => "deleted",
        })
    }
}

/// Error returned when parsing an invalid [`State`] string.
#[derive(Debug, Clone)]
pub struct ParseStateError(pub String);

impl fmt::Display for ParseStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid conference-info state: {:?}", self.0)
    }
}

impl std::error::Error for ParseStateError {}

impl FromStr for State {
    type Err = ParseStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full" => Ok(Self::Full),
            "partial" => Ok(Self::Partial),
            "deleted" => Ok(Self::Deleted),
            other => Err(ParseStateError(other.to_owned())),
        }
    }
}

/// Execution context shared by `joining-info`, `disconnection-info`, and
/// `referred` elements (RFC 4575 Section 5.6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct ExecutionInfo {
    /// ISO 8601 timestamp (e.g. `2026-02-24T14:26:16.217046Z`).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub when: Option<String>,
    /// Human-readable or protocol-defined reason string.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub reason: Option<String>,
    /// URI of the actor that triggered this event.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub by: Option<String>,
}

impl ExecutionInfo {
    /// Create an empty execution info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the timestamp.
    pub fn with_when(mut self, when: impl Into<String>) -> Self {
        self.when = Some(when.into());
        self
    }

    /// Set the reason.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the actor URI.
    pub fn with_by(mut self, by: impl Into<String>) -> Self {
        self.by = Some(by.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trip() {
        for (s, expected) in [
            ("full", State::Full),
            ("partial", State::Partial),
            ("deleted", State::Deleted),
        ] {
            let parsed: State = s
                .parse()
                .unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn state_invalid() {
        assert!("Full"
            .parse::<State>()
            .is_err());
        assert!("FULL"
            .parse::<State>()
            .is_err());
        assert!(""
            .parse::<State>()
            .is_err());
    }

    #[test]
    fn execution_info_builder() {
        let info = ExecutionInfo::new()
            .with_when("2026-02-24T14:26:16Z")
            .with_by("sip:alice@example.com");
        assert_eq!(
            info.when
                .as_deref(),
            Some("2026-02-24T14:26:16Z")
        );
        assert_eq!(
            info.by
                .as_deref(),
            Some("sip:alice@example.com")
        );
        assert!(info
            .reason
            .is_none());
    }
}
