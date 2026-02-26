use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::{extract_variables, write_variables};
use crate::commands::originate::{OriginateError, Variables};

/// Runtime expression resolving directory group members:
/// `${group_call(group@domain[+order])}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GroupCall {
    /// Group name from the directory.
    pub group: String,
    /// Domain for the group lookup.
    pub domain: String,
    /// Distribution order: `A` (all), `E` (enterprise), `F` (first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Variables>,
}

impl GroupCall {
    /// Create a new group_call expression.
    pub fn new(group: impl Into<String>, domain: impl Into<String>) -> Self {
        Self {
            group: group.into(),
            domain: domain.into(),
            order: None,
            variables: None,
        }
    }

    /// Set the distribution order (A=all, E=enterprise, F=first).
    pub fn with_order(mut self, order: impl Into<String>) -> Self {
        self.order = Some(order.into());
        self
    }

    /// Set per-channel variables.
    pub fn with_variables(mut self, variables: Variables) -> Self {
        self.variables = Some(variables);
        self
    }
}

impl fmt::Display for GroupCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_variables(f, &self.variables)?;
        match &self.order {
            Some(o) => write!(f, "${{group_call({}@{}+{})}}", self.group, self.domain, o),
            None => write!(f, "${{group_call({}@{})}}", self.group, self.domain),
        }
    }
}

impl FromStr for GroupCall {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (variables, uri) = extract_variables(s)?;
        let inner = uri
            .strip_prefix("${group_call(")
            .and_then(|r| r.strip_suffix(")}"))
            .ok_or_else(|| OriginateError::ParseError("not a group_call expression".into()))?;
        let (group_at_domain, order) = if let Some((gd, o)) = inner.split_once('+') {
            (gd, Some(o.to_string()))
        } else {
            (inner, None)
        };
        let (group, domain) = group_at_domain
            .split_once('@')
            .ok_or_else(|| OriginateError::ParseError("group_call needs group@domain".into()))?;
        Ok(Self {
            group: group.into(),
            domain: domain.into(),
            order,
            variables,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_call_display() {
        let ep = GroupCall {
            group: "support".into(),
            domain: "domain.com".into(),
            order: None,
            variables: None,
        };
        assert_eq!(ep.to_string(), "${group_call(support@domain.com)}");
    }

    #[test]
    fn group_call_display_with_order() {
        let ep = GroupCall {
            group: "support".into(),
            domain: "domain.com".into(),
            order: Some("A".into()),
            variables: None,
        };
        assert_eq!(ep.to_string(), "${group_call(support@domain.com+A)}");
    }

    #[test]
    fn group_call_from_str() {
        let ep: GroupCall = "${group_call(support@domain.com)}"
            .parse()
            .unwrap();
        assert_eq!(ep.group, "support");
        assert_eq!(ep.domain, "domain.com");
        assert!(ep
            .order
            .is_none());
    }

    #[test]
    fn group_call_from_str_with_order() {
        let ep: GroupCall = "${group_call(support@domain.com+A)}"
            .parse()
            .unwrap();
        assert_eq!(ep.group, "support");
        assert_eq!(ep.domain, "domain.com");
        assert_eq!(
            ep.order
                .as_deref(),
            Some("A")
        );
    }

    #[test]
    fn group_call_round_trip() {
        let ep = GroupCall {
            group: "calltakers".into(),
            domain: "example.com".into(),
            order: Some("E".into()),
            variables: None,
        };
        let s = ep.to_string();
        let parsed: GroupCall = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    #[test]
    fn serde_group_call() {
        let ep = GroupCall {
            group: "support".into(),
            domain: "domain.com".into(),
            order: Some("A".into()),
            variables: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let parsed: GroupCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ep);
    }
}
