use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::{extract_variables, write_variables};
use crate::commands::originate::{OriginateError, Variables};

/// Internal loopback endpoint: `loopback/{extension}[/{context}]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LoopbackEndpoint {
    /// Extension number or pattern.
    pub extension: String,
    /// Dialplan context (defaults to `"default"`).
    pub context: String,
    /// Per-channel variables prepended as `{key=value}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Variables>,
}

impl LoopbackEndpoint {
    /// Create a new loopback endpoint.
    pub fn new(extension: impl Into<String>, context: impl Into<String>) -> Self {
        Self {
            extension: extension.into(),
            context: context.into(),
            variables: None,
        }
    }

    /// Set per-channel variables.
    pub fn with_variables(mut self, variables: Variables) -> Self {
        self.variables = Some(variables);
        self
    }
}

impl fmt::Display for LoopbackEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_variables(f, &self.variables)?;
        write!(f, "loopback/{}/{}", self.extension, self.context)
    }
}

impl FromStr for LoopbackEndpoint {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (variables, uri) = extract_variables(s)?;
        let rest = uri
            .strip_prefix("loopback/")
            .ok_or_else(|| OriginateError::ParseError("not a loopback endpoint".into()))?;
        let (extension, context) = rest
            .split_once('/')
            .unwrap_or((rest, "default"));
        Ok(Self {
            extension: extension.into(),
            context: context.into(),
            variables,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::originate::VariablesType;

    #[test]
    fn loopback_display() {
        let ep = LoopbackEndpoint {
            extension: "9199".into(),
            context: "default".into(),
            variables: None,
        };
        assert_eq!(ep.to_string(), "loopback/9199/default");
    }

    #[test]
    fn loopback_display_with_variables() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("loopback_initial_codec", "L16@48000h");
        let ep = LoopbackEndpoint {
            extension: "100".into(),
            context: "test".into(),
            variables: Some(vars),
        };
        assert_eq!(
            ep.to_string(),
            "{loopback_initial_codec=L16@48000h}loopback/100/test"
        );
    }

    #[test]
    fn loopback_from_str() {
        let ep: LoopbackEndpoint = "loopback/9199/test"
            .parse()
            .unwrap();
        assert_eq!(ep.extension, "9199");
        assert_eq!(ep.context, "test");
    }

    #[test]
    fn loopback_from_str_no_context_defaults() {
        let ep: LoopbackEndpoint = "loopback/9199"
            .parse()
            .unwrap();
        assert_eq!(ep.extension, "9199");
        assert_eq!(ep.context, "default");
    }

    #[test]
    fn loopback_round_trip() {
        let ep = LoopbackEndpoint {
            extension: "100".into(),
            context: "myctx".into(),
            variables: None,
        };
        let s = ep.to_string();
        let parsed: LoopbackEndpoint = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    #[test]
    fn serde_loopback_endpoint() {
        let ep = LoopbackEndpoint {
            extension: "9199".into(),
            context: "default".into(),
            variables: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let parsed: LoopbackEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ep);
    }
}
