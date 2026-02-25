//! FreeSWITCH endpoint types for originate and bridge dial strings.
//!
//! Each endpoint type corresponds to a real FreeSWITCH endpoint module
//! or runtime expression. Concrete structs implement the [`DialString`]
//! trait independently; the [`Endpoint`] enum wraps them for
//! serialization and polymorphic storage.

use std::fmt;
use std::str::FromStr;

use super::originate::{OriginateError, Variables, VariablesType};

/// Common interface for anything that formats as a FreeSWITCH dial string.
///
/// Implemented on each concrete endpoint struct and on the [`Endpoint`] enum.
/// Downstream crates can implement this on custom endpoint types.
pub trait DialString: fmt::Display {
    /// Per-endpoint variables, if any.
    fn variables(&self) -> Option<&Variables>;
    /// Mutable access to per-endpoint variables.
    fn variables_mut(&mut self) -> Option<&mut Variables>;
    /// Replace per-endpoint variables.
    fn set_variables(&mut self, vars: Option<Variables>);
}

// ---------------------------------------------------------------------------
// Concrete endpoint structs
// ---------------------------------------------------------------------------

/// SIP endpoint via a named profile: `sofia/{profile}/{destination}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SofiaEndpoint {
    /// SIP profile name (e.g. `internal`, `external`).
    pub profile: String,
    /// SIP URI or destination number.
    pub destination: String,
    /// Per-channel variables prepended as `{key=value}`.
    pub variables: Option<Variables>,
}

/// SIP endpoint via a configured gateway:
/// `sofia/gateway/[{profile}::]{gateway}/{destination}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SofiaGateway {
    /// Gateway name as configured in the SIP profile.
    pub gateway: String,
    /// Destination number or SIP user part.
    pub destination: String,
    /// SIP profile name to qualify the gateway lookup.
    pub profile: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    pub variables: Option<Variables>,
}

/// Internal loopback endpoint: `loopback/{extension}[/{context}]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackEndpoint {
    /// Extension number or pattern.
    pub extension: String,
    /// Dialplan context (defaults to `"default"`).
    pub context: String,
    /// Per-channel variables prepended as `{key=value}`.
    pub variables: Option<Variables>,
}

/// Directory-based endpoint: `user/{name}[@{domain}]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEndpoint {
    /// User name from the directory.
    pub name: String,
    /// Domain name (optional, uses default domain if absent).
    pub domain: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    pub variables: Option<Variables>,
}

/// Runtime expression resolving registered SIP contacts:
/// `${sofia_contact([profile/]user@domain)}`.
///
/// The library produces the expression string; FreeSWITCH evaluates it
/// at call time. Use `profile: Some("*".into())` to search all profiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SofiaContact {
    /// User part of the contact lookup.
    pub user: String,
    /// Domain for the contact lookup.
    pub domain: String,
    /// SIP profile name, or `"*"` for all profiles.
    pub profile: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    pub variables: Option<Variables>,
}

/// Runtime expression resolving directory group members:
/// `${group_call(group@domain[+order])}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCall {
    /// Group name from the directory.
    pub group: String,
    /// Domain for the group lookup.
    pub domain: String,
    /// Distribution order: `A` (all), `E` (enterprise), `F` (first).
    pub order: Option<String>,
    /// Per-channel variables prepended as `{key=value}`.
    pub variables: Option<Variables>,
}

/// Bridge to a specific hangup cause: `error/{cause}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorEndpoint {
    /// Hangup cause string (e.g. `user_busy`, `no_answer`).
    pub cause: String,
}

// ---------------------------------------------------------------------------
// Endpoint enum
// ---------------------------------------------------------------------------

/// Polymorphic endpoint wrapping all concrete types.
///
/// Use this in [`Originate`](super::originate::Originate) and
/// [`BridgeDialString`] where any endpoint type must be accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// `sofia/{profile}/{destination}`
    Sofia(SofiaEndpoint),
    /// `sofia/gateway/[{profile}::]{gateway}/{destination}`
    SofiaGateway(SofiaGateway),
    /// `loopback/{extension}[/{context}]`
    Loopback(LoopbackEndpoint),
    /// `user/{name}[@{domain}]`
    User(UserEndpoint),
    /// `${sofia_contact([profile/]user@domain)}`
    SofiaContact(SofiaContact),
    /// `${group_call(group@domain[+order])}`
    GroupCall(GroupCall),
    /// `error/{cause}`
    Error(ErrorEndpoint),
}

// ---------------------------------------------------------------------------
// Stub Display impls (red phase -- produce wrong output)
// ---------------------------------------------------------------------------

fn write_variables(f: &mut fmt::Formatter<'_>, vars: &Option<Variables>) -> fmt::Result {
    if let Some(vars) = vars {
        if !vars.is_empty() {
            write!(f, "{}", vars)?;
        }
    }
    Ok(())
}

impl fmt::Display for SofiaEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for SofiaGateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for LoopbackEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for UserEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for SofiaContact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for GroupCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for ErrorEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sofia(ep) => ep.fmt(f),
            Self::SofiaGateway(ep) => ep.fmt(f),
            Self::Loopback(ep) => ep.fmt(f),
            Self::User(ep) => ep.fmt(f),
            Self::SofiaContact(ep) => ep.fmt(f),
            Self::GroupCall(ep) => ep.fmt(f),
            Self::Error(ep) => ep.fmt(f),
        }
    }
}

// ---------------------------------------------------------------------------
// Stub FromStr impls (red phase -- always error)
// ---------------------------------------------------------------------------

impl FromStr for SofiaEndpoint {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for SofiaGateway {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for LoopbackEndpoint {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for UserEndpoint {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for SofiaContact {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for GroupCall {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for ErrorEndpoint {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

impl FromStr for Endpoint {
    type Err = OriginateError;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(OriginateError::ParseError("not implemented".into()))
    }
}

// ---------------------------------------------------------------------------
// Stub DialString impls (red phase)
// ---------------------------------------------------------------------------

macro_rules! impl_dial_string_with_variables {
    ($ty:ty) => {
        impl DialString for $ty {
            fn variables(&self) -> Option<&Variables> {
                self.variables
                    .as_ref()
            }
            fn variables_mut(&mut self) -> Option<&mut Variables> {
                self.variables
                    .as_mut()
            }
            fn set_variables(&mut self, vars: Option<Variables>) {
                self.variables = vars;
            }
        }
    };
}

impl_dial_string_with_variables!(SofiaEndpoint);
impl_dial_string_with_variables!(SofiaGateway);
impl_dial_string_with_variables!(LoopbackEndpoint);
impl_dial_string_with_variables!(UserEndpoint);
impl_dial_string_with_variables!(SofiaContact);
impl_dial_string_with_variables!(GroupCall);

impl DialString for ErrorEndpoint {
    fn variables(&self) -> Option<&Variables> {
        None
    }
    fn variables_mut(&mut self) -> Option<&mut Variables> {
        None
    }
    fn set_variables(&mut self, _vars: Option<Variables>) {}
}

impl DialString for Endpoint {
    fn variables(&self) -> Option<&Variables> {
        match self {
            Self::Sofia(ep) => ep.variables(),
            Self::SofiaGateway(ep) => ep.variables(),
            Self::Loopback(ep) => ep.variables(),
            Self::User(ep) => ep.variables(),
            Self::SofiaContact(ep) => ep.variables(),
            Self::GroupCall(ep) => ep.variables(),
            Self::Error(ep) => ep.variables(),
        }
    }
    fn variables_mut(&mut self) -> Option<&mut Variables> {
        match self {
            Self::Sofia(ep) => ep.variables_mut(),
            Self::SofiaGateway(ep) => ep.variables_mut(),
            Self::Loopback(ep) => ep.variables_mut(),
            Self::User(ep) => ep.variables_mut(),
            Self::SofiaContact(ep) => ep.variables_mut(),
            Self::GroupCall(ep) => ep.variables_mut(),
            Self::Error(ep) => ep.variables_mut(),
        }
    }
    fn set_variables(&mut self, vars: Option<Variables>) {
        match self {
            Self::Sofia(ep) => ep.set_variables(vars),
            Self::SofiaGateway(ep) => ep.set_variables(vars),
            Self::Loopback(ep) => ep.set_variables(vars),
            Self::User(ep) => ep.set_variables(vars),
            Self::SofiaContact(ep) => ep.set_variables(vars),
            Self::GroupCall(ep) => ep.set_variables(vars),
            Self::Error(ep) => ep.set_variables(vars),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // === SofiaEndpoint ===

    #[test]
    fn sofia_endpoint_display() {
        let ep = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000@domain.com".into(),
            variables: None,
        };
        assert_eq!(ep.to_string(), "sofia/internal/1000@domain.com");
    }

    #[test]
    fn sofia_endpoint_display_with_variables() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("originate_timeout", "30");
        let ep = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000@domain.com".into(),
            variables: Some(vars),
        };
        assert_eq!(
            ep.to_string(),
            "{originate_timeout=30}sofia/internal/1000@domain.com"
        );
    }

    #[test]
    fn sofia_endpoint_from_str() {
        let ep: SofiaEndpoint = "sofia/internal/1000@domain.com"
            .parse()
            .unwrap();
        assert_eq!(ep.profile, "internal");
        assert_eq!(ep.destination, "1000@domain.com");
        assert!(ep
            .variables
            .is_none());
    }

    #[test]
    fn sofia_endpoint_from_str_with_variables() {
        let ep: SofiaEndpoint = "{originate_timeout=30}sofia/internal/1000@domain.com"
            .parse()
            .unwrap();
        assert_eq!(ep.profile, "internal");
        assert_eq!(ep.destination, "1000@domain.com");
        assert_eq!(
            ep.variables
                .unwrap()
                .get("originate_timeout"),
            Some("30")
        );
    }

    #[test]
    fn sofia_endpoint_round_trip() {
        let ep = SofiaEndpoint {
            profile: "external".into(),
            destination: "sip:user@host:5060".into(),
            variables: None,
        };
        let s = ep.to_string();
        let parsed: SofiaEndpoint = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    // === SofiaGateway ===

    #[test]
    fn sofia_gateway_display() {
        let ep = SofiaGateway {
            gateway: "my_provider".into(),
            destination: "18005551234".into(),
            profile: None,
            variables: None,
        };
        assert_eq!(ep.to_string(), "sofia/gateway/my_provider/18005551234");
    }

    #[test]
    fn sofia_gateway_display_with_profile() {
        let ep = SofiaGateway {
            gateway: "my_provider".into(),
            destination: "18005551234".into(),
            profile: Some("external".into()),
            variables: None,
        };
        assert_eq!(
            ep.to_string(),
            "sofia/gateway/external::my_provider/18005551234"
        );
    }

    #[test]
    fn sofia_gateway_display_with_variables() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("sip_h_X-Custom", "value");
        let ep = SofiaGateway {
            gateway: "gw1".into(),
            destination: "1234".into(),
            profile: None,
            variables: Some(vars),
        };
        assert_eq!(
            ep.to_string(),
            "{sip_h_X-Custom=value}sofia/gateway/gw1/1234"
        );
    }

    #[test]
    fn sofia_gateway_from_str() {
        let ep: SofiaGateway = "sofia/gateway/my_provider/18005551234"
            .parse()
            .unwrap();
        assert_eq!(ep.gateway, "my_provider");
        assert_eq!(ep.destination, "18005551234");
        assert!(ep
            .profile
            .is_none());
    }

    #[test]
    fn sofia_gateway_from_str_with_profile() {
        let ep: SofiaGateway = "sofia/gateway/external::my_provider/18005551234"
            .parse()
            .unwrap();
        assert_eq!(ep.gateway, "my_provider");
        assert_eq!(ep.destination, "18005551234");
        assert_eq!(
            ep.profile
                .as_deref(),
            Some("external")
        );
    }

    #[test]
    fn sofia_gateway_round_trip() {
        let ep = SofiaGateway {
            gateway: "carrier".into(),
            destination: "15551234567".into(),
            profile: Some("external".into()),
            variables: None,
        };
        let s = ep.to_string();
        let parsed: SofiaGateway = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    // === LoopbackEndpoint ===

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

    // === UserEndpoint ===

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

    // === SofiaContact ===

    #[test]
    fn sofia_contact_display() {
        let ep = SofiaContact {
            user: "1000".into(),
            domain: "domain.com".into(),
            profile: None,
            variables: None,
        };
        assert_eq!(ep.to_string(), "${sofia_contact(1000@domain.com)}");
    }

    #[test]
    fn sofia_contact_display_with_profile() {
        let ep = SofiaContact {
            user: "1000".into(),
            domain: "domain.com".into(),
            profile: Some("internal".into()),
            variables: None,
        };
        assert_eq!(ep.to_string(), "${sofia_contact(internal/1000@domain.com)}");
    }

    #[test]
    fn sofia_contact_display_all_profiles() {
        let ep = SofiaContact {
            user: "1000".into(),
            domain: "domain.com".into(),
            profile: Some("*".into()),
            variables: None,
        };
        assert_eq!(ep.to_string(), "${sofia_contact(*/1000@domain.com)}");
    }

    #[test]
    fn sofia_contact_display_with_variables() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("presence_id", "1000@domain.com");
        let ep = SofiaContact {
            user: "1000".into(),
            domain: "domain.com".into(),
            profile: None,
            variables: Some(vars),
        };
        assert_eq!(
            ep.to_string(),
            "{presence_id=1000@domain.com}${sofia_contact(1000@domain.com)}"
        );
    }

    #[test]
    fn sofia_contact_from_str() {
        let ep: SofiaContact = "${sofia_contact(1000@domain.com)}"
            .parse()
            .unwrap();
        assert_eq!(ep.user, "1000");
        assert_eq!(ep.domain, "domain.com");
        assert!(ep
            .profile
            .is_none());
    }

    #[test]
    fn sofia_contact_from_str_with_profile() {
        let ep: SofiaContact = "${sofia_contact(internal/1000@domain.com)}"
            .parse()
            .unwrap();
        assert_eq!(ep.user, "1000");
        assert_eq!(ep.domain, "domain.com");
        assert_eq!(
            ep.profile
                .as_deref(),
            Some("internal")
        );
    }

    #[test]
    fn sofia_contact_round_trip() {
        let ep = SofiaContact {
            user: "bob".into(),
            domain: "example.com".into(),
            profile: Some("*".into()),
            variables: None,
        };
        let s = ep.to_string();
        let parsed: SofiaContact = s
            .parse()
            .unwrap();
        assert_eq!(parsed, ep);
    }

    // === GroupCall ===

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

    // === ErrorEndpoint ===

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

    // === Endpoint enum FromStr ===

    #[test]
    fn endpoint_from_str_sofia() {
        let ep: Endpoint = "sofia/internal/1000@domain.com"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::Sofia(_)));
    }

    #[test]
    fn endpoint_from_str_sofia_gateway() {
        let ep: Endpoint = "sofia/gateway/my_gw/1234"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::SofiaGateway(_)));
    }

    #[test]
    fn endpoint_from_str_loopback() {
        let ep: Endpoint = "loopback/9199/test"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::Loopback(_)));
    }

    #[test]
    fn endpoint_from_str_user() {
        let ep: Endpoint = "user/1000@domain.com"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::User(_)));
    }

    #[test]
    fn endpoint_from_str_sofia_contact() {
        let ep: Endpoint = "${sofia_contact(1000@domain.com)}"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::SofiaContact(_)));
    }

    #[test]
    fn endpoint_from_str_group_call() {
        let ep: Endpoint = "${group_call(support@domain.com+A)}"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::GroupCall(_)));
    }

    #[test]
    fn endpoint_from_str_error() {
        let ep: Endpoint = "error/user_busy"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::Error(_)));
    }

    #[test]
    fn endpoint_from_str_unknown_errors() {
        let result = "verto/1234".parse::<Endpoint>();
        assert!(result.is_err());
    }

    #[test]
    fn endpoint_from_str_with_variables() {
        let ep: Endpoint = "{timeout=30}sofia/internal/1000@domain.com"
            .parse()
            .unwrap();
        if let Endpoint::Sofia(inner) = &ep {
            assert_eq!(inner.profile, "internal");
            assert!(inner
                .variables
                .is_some());
        } else {
            panic!("expected Sofia variant");
        }
    }

    // === Endpoint enum Display delegation ===

    #[test]
    fn endpoint_display_delegates_to_inner() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000@domain.com".into(),
            variables: None,
        });
        assert_eq!(ep.to_string(), "sofia/internal/1000@domain.com");
    }

    // === DialString trait ===

    #[test]
    fn dial_string_variables_returns_some() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "v");
        let ep = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000".into(),
            variables: Some(vars),
        };
        assert!(ep
            .variables()
            .is_some());
        assert_eq!(
            ep.variables()
                .unwrap()
                .get("k"),
            Some("v")
        );
    }

    #[test]
    fn dial_string_variables_returns_none() {
        let ep = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000".into(),
            variables: None,
        };
        assert!(ep
            .variables()
            .is_none());
    }

    #[test]
    fn dial_string_set_variables() {
        let mut ep = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000".into(),
            variables: None,
        };
        let mut vars = Variables::new(VariablesType::Channel);
        vars.insert("k", "v");
        ep.set_variables(Some(vars));
        assert!(ep
            .variables()
            .is_some());
    }

    #[test]
    fn dial_string_error_endpoint_no_variables() {
        let ep = ErrorEndpoint {
            cause: "user_busy".into(),
        };
        assert!(ep
            .variables()
            .is_none());
    }

    #[test]
    fn dial_string_on_endpoint_enum() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "v");
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000".into(),
            variables: Some(vars),
        });
        assert!(ep
            .variables()
            .is_some());
    }
}
