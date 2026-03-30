//! Gateway registration state and ping status enums.

use std::fmt;
use std::str::FromStr;

/// Error returned when parsing an unrecognized gateway registration state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseGatewayRegStateError(pub String);

impl fmt::Display for ParseGatewayRegStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown gateway reg state: {}", self.0)
    }
}

impl std::error::Error for ParseGatewayRegStateError {}

/// Gateway registration state from `sofia::gateway_state` events.
///
/// The `State` header value, mapping to `reg_state_t` / `sofia_state_names[]`
/// in mod_sofia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum GatewayRegState {
    Unreged,
    Trying,
    Register,
    Reged,
    Unregister,
    Failed,
    FailWait,
    Expired,
    Noreg,
    Down,
    Timeout,
}

impl GatewayRegState {
    /// Wire-format string matching `sofia_state_names[]`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unreged => "UNREGED",
            Self::Trying => "TRYING",
            Self::Register => "REGISTER",
            Self::Reged => "REGED",
            Self::Unregister => "UNREGISTER",
            Self::Failed => "FAILED",
            Self::FailWait => "FAIL_WAIT",
            Self::Expired => "EXPIRED",
            Self::Noreg => "NOREG",
            Self::Down => "DOWN",
            Self::Timeout => "TIMEOUT",
        }
    }
}

impl fmt::Display for GatewayRegState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GatewayRegState {
    type Err = ParseGatewayRegStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "UNREGED" => Ok(Self::Unreged),
            "TRYING" => Ok(Self::Trying),
            "REGISTER" => Ok(Self::Register),
            "REGED" => Ok(Self::Reged),
            "UNREGISTER" => Ok(Self::Unregister),
            "FAILED" => Ok(Self::Failed),
            "FAIL_WAIT" => Ok(Self::FailWait),
            "EXPIRED" => Ok(Self::Expired),
            "NOREG" => Ok(Self::Noreg),
            "DOWN" => Ok(Self::Down),
            "TIMEOUT" => Ok(Self::Timeout),
            _ => Err(ParseGatewayRegStateError(s.to_string())),
        }
    }
}

/// Error returned when parsing an unrecognized gateway ping status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseGatewayPingStatusError(pub String);

impl fmt::Display for ParseGatewayPingStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown gateway ping status: {}", self.0)
    }
}

impl std::error::Error for ParseGatewayPingStatusError {}

/// Gateway ping status from `sofia::gateway_state` events.
///
/// The `Ping-Status` header value, mapping to `sofia_gateway_status_name()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum GatewayPingStatus {
    Down,
    Up,
    Invalid,
}

impl GatewayPingStatus {
    /// Wire-format string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Down => "DOWN",
            Self::Up => "UP",
            Self::Invalid => "INVALID",
        }
    }
}

impl fmt::Display for GatewayPingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GatewayPingStatus {
    type Err = ParseGatewayPingStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "DOWN" => Ok(Self::Down),
            "UP" => Ok(Self::Up),
            "INVALID" => Ok(Self::Invalid),
            _ => Err(ParseGatewayPingStatusError(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_REG_STATES: &[GatewayRegState] = &[
        GatewayRegState::Unreged,
        GatewayRegState::Trying,
        GatewayRegState::Register,
        GatewayRegState::Reged,
        GatewayRegState::Unregister,
        GatewayRegState::Failed,
        GatewayRegState::FailWait,
        GatewayRegState::Expired,
        GatewayRegState::Noreg,
        GatewayRegState::Down,
        GatewayRegState::Timeout,
    ];

    #[test]
    fn reg_state_round_trip() {
        for &state in ALL_REG_STATES {
            let wire = state.to_string();
            let parsed: GatewayRegState = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, state, "round-trip failed for {wire}");
        }
    }

    #[test]
    fn reg_state_count() {
        assert_eq!(ALL_REG_STATES.len(), 11);
    }

    #[test]
    fn reg_state_rejects_unknown() {
        assert!("BOGUS"
            .parse::<GatewayRegState>()
            .is_err());
        assert!("reged"
            .parse::<GatewayRegState>()
            .is_err());
    }

    const ALL_PING_STATUSES: &[GatewayPingStatus] = &[
        GatewayPingStatus::Down,
        GatewayPingStatus::Up,
        GatewayPingStatus::Invalid,
    ];

    #[test]
    fn ping_status_round_trip() {
        for &status in ALL_PING_STATUSES {
            let wire = status.to_string();
            let parsed: GatewayPingStatus = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, status, "round-trip failed for {wire}");
        }
    }

    #[test]
    fn ping_status_rejects_unknown() {
        assert!("BOGUS"
            .parse::<GatewayPingStatus>()
            .is_err());
        assert!("up"
            .parse::<GatewayPingStatus>()
            .is_err());
    }
}
