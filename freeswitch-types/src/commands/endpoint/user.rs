use super::{after_prefix, check_field};
use crate::commands::originate::OriginateError;
use crate::commands::variables::Variables;

/// Directory-based endpoint: `user/{name}[@{domain}]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "config::UserEndpoint"))]
#[non_exhaustive]
pub struct UserEndpoint {
    /// User name from the directory.
    pub name: String,
    /// Domain name (optional, uses default domain if absent).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub domain: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub variables: Option<Variables>,
}

impl UserEndpoint {
    /// Create a new user endpoint.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            domain: None,
            variables: None,
        }
    }

    /// Set the domain.
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }
}

impl_dial_string_with_variables!(UserEndpoint, write_module_text, |this| match &this.domain {
    Some(d) => format!("user/{}@{d}", this.name),
    None => format!("user/{}", this.name),
});

impl UserEndpoint {
    /// Refuse an `@` in the name, where `user_outgoing_channel` starts the domain.
    pub(crate) fn check_deliverable(&self) -> Result<(), OriginateError> {
        check_field("user", "name", &self.name, &["@"])?;
        match &self.domain {
            Some(domain) => check_field("user", "domain", domain, &[]),
            None => Ok(()),
        }
    }

    pub(crate) fn parse_bare(text: &str) -> Result<Self, OriginateError> {
        let rest = after_prefix(text, "user/", "user")?;
        let ep = match rest.split_once('@') {
            Some((name, domain)) => Self::new(name).with_domain(domain),
            None => Self::new(rest),
        };
        ep.check_deliverable()?;
        Ok(ep)
    }
}

impl_endpoint_parse!(from_str: UserEndpoint);

impl_endpoint_parse!(config:
    UserEndpoint {
        name: String,
        ;
        domain: Option<String>,
        variables: Option<Variables>,
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_endpoint_display() {
        let ep = UserEndpoint {
            name: "1000".into(),
            domain: Some("example.com".into()),
            variables: None,
        };
        assert_eq!(ep.to_string(), "user/1000@example.com");
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
        let ep: UserEndpoint = "user/1000@example.com"
            .parse()
            .unwrap();
        assert_eq!(ep.name, "1000");
        assert_eq!(
            ep.domain
                .as_deref(),
            Some("example.com")
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
            domain: Some("example.com".into()),
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
