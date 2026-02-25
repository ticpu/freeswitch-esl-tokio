use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::commands::originate::OriginateError;

/// Bridge to a specific hangup cause: `error/{cause}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorEndpoint {
    /// Hangup cause string (e.g. `user_busy`, `no_answer`).
    pub cause: String,
}

impl fmt::Display for ErrorEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error/{}", self.cause)
    }
}

impl FromStr for ErrorEndpoint {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let cause = s
            .strip_prefix("error/")
            .ok_or_else(|| OriginateError::ParseError("not an error endpoint".into()))?;
        Ok(Self {
            cause: cause.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_endpoint_display() {
        let ep = ErrorEndpoint {
            cause: "user_busy".into(),
        };
        assert_eq!(ep.to_string(), "error/user_busy");
    }

    #[test]
    fn error_endpoint_from_str() {
        let ep: ErrorEndpoint = "error/user_busy"
            .parse()
            .unwrap();
        assert_eq!(ep.cause, "user_busy");
    }

    #[test]
    fn error_endpoint_round_trip() {
        let ep = ErrorEndpoint {
            cause: "no_answer".into(),
        };
        let s = ep.to_string();
        let parsed: ErrorEndpoint = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    #[test]
    fn serde_error_endpoint() {
        let ep = ErrorEndpoint {
            cause: "user_busy".into(),
        };
        let json = serde_json::to_string(&ep).unwrap();
        let parsed: ErrorEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ep);
    }
}
