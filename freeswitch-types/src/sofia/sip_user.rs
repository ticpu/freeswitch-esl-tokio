//! SIP user ping status enum.

use std::fmt;
use std::str::FromStr;

/// Error returned when parsing an unrecognized SIP user ping status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSipUserPingStatusError(pub String);

impl fmt::Display for ParseSipUserPingStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown sip user ping status: {}", self.0)
    }
}

impl std::error::Error for ParseSipUserPingStatusError {}

/// SIP user ping status from `sofia::sip_user_state` events.
///
/// The `Ping-Status` header value, mapping to `sofia_sip_user_status_name()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum SipUserPingStatus {
    Unreachable,
    Reachable,
    Invalid,
}

impl SipUserPingStatus {
    /// Wire-format string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unreachable => "UNREACHABLE",
            Self::Reachable => "REACHABLE",
            Self::Invalid => "INVALID",
        }
    }
}

impl fmt::Display for SipUserPingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SipUserPingStatus {
    type Err = ParseSipUserPingStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "UNREACHABLE" => Ok(Self::Unreachable),
            "REACHABLE" => Ok(Self::Reachable),
            "INVALID" => Ok(Self::Invalid),
            _ => Err(ParseSipUserPingStatusError(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[SipUserPingStatus] = &[
        SipUserPingStatus::Unreachable,
        SipUserPingStatus::Reachable,
        SipUserPingStatus::Invalid,
    ];

    #[test]
    fn round_trip() {
        for &status in ALL {
            let wire = status.to_string();
            let parsed: SipUserPingStatus = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, status, "round-trip failed for {wire}");
        }
    }

    #[test]
    fn rejects_unknown() {
        assert!("BOGUS"
            .parse::<SipUserPingStatus>()
            .is_err());
        assert!("reachable"
            .parse::<SipUserPingStatus>()
            .is_err());
    }
}
