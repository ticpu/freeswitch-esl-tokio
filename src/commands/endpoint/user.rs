use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::{extract_variables, write_variables};
use crate::commands::originate::{OriginateError, Variables};

/// Directory-based endpoint: `user/{name}[@{domain}]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserEndpoint {
    /// User name from the directory.
    pub name: String,
    /// Domain name (optional, uses default domain if absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Variables>,
}

impl fmt::Display for UserEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_variables(f, &self.variables)?;
        match &self.domain {
            Some(d) => write!(f, "user/{}@{}", self.name, d),
            None => write!(f, "user/{}", self.name),
        }
    }
}

impl FromStr for UserEndpoint {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (variables, uri) = extract_variables(s)?;
        let rest = uri
            .strip_prefix("user/")
            .ok_or_else(|| OriginateError::ParseError("not a user endpoint".into()))?;
        let (name, domain) = if let Some((n, d)) = rest.split_once('@') {
            (n.to_string(), Some(d.to_string()))
        } else {
            (rest.to_string(), None)
        };
        Ok(Self {
            name,
            domain,
            variables,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_endpoint_display() {
        let ep = UserEndpoint {
            name: "1000".into(),
            domain: Some("domain.com".into()),
            variables: None,
        };
        assert_eq!(ep.to_string(), "user/1000@domain.com");
    }

    #[test]
    fn user_endpoint_display_no_domain() {
        let ep = UserEndpoint {
            name: "1000".into(),
            domain: None,
            variables: None,
        };
        assert_eq!(ep.to_string(), "user/1000");
    }

    #[test]
    fn user_endpoint_from_str() {
        let ep: UserEndpoint = "user/1000@domain.com"
            .parse()
            .unwrap();
        assert_eq!(ep.name, "1000");
        assert_eq!(
            ep.domain
                .as_deref(),
            Some("domain.com")
        );
    }

    #[test]
    fn user_endpoint_from_str_no_domain() {
        let ep: UserEndpoint = "user/1000"
            .parse()
            .unwrap();
        assert_eq!(ep.name, "1000");
        assert!(ep
            .domain
            .is_none());
    }

    #[test]
    fn user_endpoint_round_trip() {
        let ep = UserEndpoint {
            name: "bob".into(),
            domain: Some("example.com".into()),
            variables: None,
        };
        let s = ep.to_string();
        let parsed: UserEndpoint = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    #[test]
    fn serde_user_endpoint() {
        let ep = UserEndpoint {
            name: "1000".into(),
            domain: Some("domain.com".into()),
            variables: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let parsed: UserEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ep);
    }

    #[test]
    fn serde_user_endpoint_no_domain() {
        let ep = UserEndpoint {
            name: "1000".into(),
            domain: None,
            variables: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let parsed: UserEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ep);
    }
}
