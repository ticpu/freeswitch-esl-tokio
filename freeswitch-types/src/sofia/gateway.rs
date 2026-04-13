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
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum GatewayRegState {
    #[cfg_attr(feature = "serde", serde(alias = "Unreged"))]
    Unreged,
    #[cfg_attr(feature = "serde", serde(alias = "Trying"))]
    Trying,
    #[cfg_attr(feature = "serde", serde(alias = "Register"))]
    Register,
    #[cfg_attr(feature = "serde", serde(alias = "Reged"))]
    Reged,
    #[cfg_attr(feature = "serde", serde(alias = "Unregister"))]
    Unregister,
    #[cfg_attr(feature = "serde", serde(alias = "Failed"))]
    Failed,
    #[cfg_attr(feature = "serde", serde(alias = "FailWait"))]
    FailWait,
    #[cfg_attr(feature = "serde", serde(alias = "Expired"))]
    Expired,
    #[cfg_attr(feature = "serde", serde(alias = "Noreg"))]
    Noreg,
    #[cfg_attr(feature = "serde", serde(alias = "Down"))]
    Down,
    #[cfg_attr(feature = "serde", serde(alias = "Timeout"))]
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
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum GatewayPingStatus {
    #[cfg_attr(feature = "serde", serde(alias = "Down"))]
    Down,
    #[cfg_attr(feature = "serde", serde(alias = "Up"))]
    Up,
    #[cfg_attr(feature = "serde", serde(alias = "Invalid"))]
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

    #[cfg(feature = "serde")]
    mod serde_tests {
        use super::*;

        #[test]
        fn reg_state_serde_uses_wire_format() {
            for &state in ALL_REG_STATES {
                let json = serde_json::to_string(&state).unwrap();
                let expected = format!("\"{}\"", state.as_str());
                assert_eq!(json, expected, "serde must serialize as wire format");
            }
        }

        #[test]
        fn reg_state_serde_round_trip() {
            for &state in ALL_REG_STATES {
                let json = serde_json::to_string(&state).unwrap();
                let parsed: GatewayRegState = serde_json::from_str(&json).unwrap();
                assert_eq!(parsed, state);
            }
        }

        #[test]
        fn reg_state_serde_accepts_pascal_case_alias() {
            let cases = [
                ("\"Noreg\"", GatewayRegState::Noreg),
                ("\"Reged\"", GatewayRegState::Reged),
                ("\"FailWait\"", GatewayRegState::FailWait),
                ("\"Unreged\"", GatewayRegState::Unreged),
            ];
            for (json, expected) in cases {
                let parsed: GatewayRegState = serde_json::from_str(json).unwrap();
                assert_eq!(parsed, expected, "PascalCase alias failed for {json}");
            }
        }

        #[test]
        fn ping_status_serde_uses_wire_format() {
            for &status in ALL_PING_STATUSES {
                let json = serde_json::to_string(&status).unwrap();
                let expected = format!("\"{}\"", status.as_str());
                assert_eq!(json, expected, "serde must serialize as wire format");
            }
        }

        #[test]
        fn ping_status_serde_round_trip() {
            for &status in ALL_PING_STATUSES {
                let json = serde_json::to_string(&status).unwrap();
                let parsed: GatewayPingStatus = serde_json::from_str(&json).unwrap();
                assert_eq!(parsed, status);
            }
        }

        #[test]
        fn ping_status_serde_accepts_pascal_case_alias() {
            let cases = [
                ("\"Down\"", GatewayPingStatus::Down),
                ("\"Up\"", GatewayPingStatus::Up),
                ("\"Invalid\"", GatewayPingStatus::Invalid),
            ];
            for (json, expected) in cases {
                let parsed: GatewayPingStatus = serde_json::from_str(json).unwrap();
                assert_eq!(parsed, expected, "PascalCase alias failed for {json}");
            }
        }
    }
}
