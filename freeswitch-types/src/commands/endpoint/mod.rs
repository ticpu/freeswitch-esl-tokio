//! FreeSWITCH endpoint types for originate and bridge dial strings.
//!
//! Each endpoint type corresponds to a real FreeSWITCH endpoint module
//! or runtime expression. Concrete structs implement the [`DialString`]
//! trait independently; the [`Endpoint`] enum wraps them for
//! serialization and polymorphic storage.

/// Emit the [`DialString`] impl and the `with_variables` builder for an endpoint
/// struct holding `variables: Option<Variables>`. Given a body, also emit the
/// target-aware `write_for` — variable-block prologue included — and the
/// [`Display`](std::fmt::Display) that renders it for the default target.
macro_rules! impl_dial_string_with_variables {
    ($ty:ty) => {
        impl $crate::commands::endpoint::DialString for $ty {
            fn variables(&self) -> Option<&$crate::commands::variables::Variables> {
                self.variables
                    .as_ref()
            }
            fn variables_mut(&mut self) -> Option<&mut $crate::commands::variables::Variables> {
                self.variables
                    .as_mut()
            }
            fn set_variables(&mut self, vars: Option<$crate::commands::variables::Variables>) {
                self.variables = vars;
            }
        }

        impl $ty {
            /// Set per-channel variables.
            pub fn with_variables(
                mut self,
                variables: $crate::commands::variables::Variables,
            ) -> Self {
                self.variables = Some(variables);
                self
            }
        }
    };
    ($ty:ty, |$this:ident, $f:ident| $body:expr) => {
        impl_dial_string_with_variables!($ty);

        impl $ty {
            pub(super) fn write_for(
                &self,
                $f: &mut ::std::fmt::Formatter<'_>,
                target: $crate::commands::variables::DialStringTarget,
            ) -> ::std::fmt::Result {
                $crate::commands::endpoint::write_variables($f, &self.variables, target)?;
                let $this = self;
                $body
            }
        }

        impl ::std::fmt::Display for $ty {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.write_for(
                    f,
                    $crate::commands::variables::DialStringCarrier::EslApi.into(),
                )
            }
        }
    };
}

/// Dispatch a [`DialString`] method to whichever concrete endpoint the
/// [`Endpoint`] variant wraps.
macro_rules! forward_to_variant {
    ($self:ident, $method:ident $(, $arg:expr)?) => {
        match $self {
            Self::Sofia(ep) => ep.$method($($arg)?),
            Self::SofiaGateway(ep) => ep.$method($($arg)?),
            Self::Loopback(ep) => ep.$method($($arg)?),
            Self::User(ep) => ep.$method($($arg)?),
            Self::SofiaContact(ep) => ep.$method($($arg)?),
            Self::GroupCall(ep) => ep.$method($($arg)?),
            Self::Error(ep) => ep.$method($($arg)?),
            Self::PortAudio(ep) | Self::PulseAudio(ep) | Self::Alsa(ep) => ep.$method($($arg)?),
        }
    };
}

mod audio;
mod error;
mod group_call;
mod loopback;
mod sofia;
mod user;

pub use audio::AudioEndpoint;
pub use error::ErrorEndpoint;
pub use group_call::{GroupCall, GroupCallOrder, ParseGroupCallOrderError};
pub use loopback::LoopbackEndpoint;
pub use sofia::{SofiaContact, SofiaEndpoint, SofiaGateway};
pub use user::UserEndpoint;

use std::fmt;
use std::str::FromStr;

use super::find_matching_bracket;
use super::originate::OriginateError;
use super::variables::{DialStringCarrier, DialStringTarget, Variables, VariablesType};

type PrefixParser = fn(&str) -> Result<Endpoint, OriginateError>;

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
// Helpers
// ---------------------------------------------------------------------------

fn write_variables(
    f: &mut fmt::Formatter<'_>,
    vars: &Option<Variables>,
    target: DialStringTarget,
) -> fmt::Result {
    if let Some(vars) = vars {
        if !vars.is_empty() {
            write!(f, "{}", vars.display_for(target))?;
        }
    }
    Ok(())
}

/// Strip the leading variable block then a fixed endpoint prefix. A prefix that
/// does not itself end a path segment (`alsa`, not `sofia/`) must be followed by
/// `/` or by nothing, or `alsafoo/bar` strips to a bare `alsa`.
pub(super) fn strip_endpoint_prefix<'a>(
    s: &'a str,
    prefix: &str,
    kind: &str,
    carrier: DialStringCarrier,
) -> Result<(Option<Variables>, &'a str), OriginateError> {
    let (variables, uri) = extract_variables(s, carrier.into())?;
    let rest = uri
        .strip_prefix(prefix)
        .filter(|rest| prefix.ends_with('/') || rest.is_empty() || rest.starts_with('/'))
        .ok_or_else(|| OriginateError::ParseError(format!("not a {} endpoint", kind)))?;
    Ok((variables, rest))
}

/// Every scope an endpoint may carry directly ahead of its module name.
const ANY_SCOPE: &[VariablesType] = &[
    VariablesType::Default,
    VariablesType::Enterprise,
    VariablesType::Channel,
];

/// Extract a leading variable block (`{...}`, `[...]`, or `<...>`) from a
/// dial string, returning the parsed variables and the remaining URI portion.
fn extract_variables(
    s: &str,
    target: DialStringTarget,
) -> Result<(Option<Variables>, &str), OriginateError> {
    extract_scoped_variables(s, target, ANY_SCOPE)
}

/// Extract a leading variable block whose brackets name one of `scopes`, so a
/// caller that owns only some of them leaves the rest to whoever follows.
///
/// Uses depth-aware bracket matching so nested brackets in values (e.g.
/// `<sip_h_Call-Info=<url>>`) don't cause premature closure.
pub(super) fn extract_scoped_variables<'a>(
    s: &'a str,
    target: DialStringTarget,
    scopes: &[VariablesType],
) -> Result<(Option<Variables>, &'a str), OriginateError> {
    let first = s
        .as_bytes()
        .first()
        .copied();
    let Some((open, close_ch)) = scopes
        .iter()
        .map(|scope| scope.delimiters())
        .find(|(open, _)| first == Some(*open as u8))
    else {
        return Ok((None, s));
    };
    let close = find_matching_bracket(s, open, close_ch)
        .ok_or_else(|| OriginateError::ParseError(format!("unclosed {} in dial string", open)))?;
    let var_str = &s[..=close];
    let vars = Variables::parse_for(var_str, target)?;
    let vars = if vars.is_empty() { None } else { Some(vars) };
    Ok((vars, s[close + 1..].trim_matches(' ')))
}

// ---------------------------------------------------------------------------
// Endpoint enum
// ---------------------------------------------------------------------------

/// Polymorphic endpoint wrapping all concrete types.
///
/// Use this in [`Originate`](super::originate::Originate) and
/// [`BridgeDialString`](super::bridge::BridgeDialString) where any endpoint type must be accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
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
    /// `portaudio[/{destination}]`
    #[cfg_attr(feature = "serde", serde(rename = "portaudio"))]
    PortAudio(AudioEndpoint),
    /// `pulseaudio[/{destination}]`
    #[cfg_attr(feature = "serde", serde(rename = "pulseaudio"))]
    PulseAudio(AudioEndpoint),
    /// `alsa[/{destination}]`
    Alsa(AudioEndpoint),
}

// ---------------------------------------------------------------------------
// From impls
// ---------------------------------------------------------------------------

impl From<SofiaEndpoint> for Endpoint {
    fn from(ep: SofiaEndpoint) -> Self {
        Self::Sofia(ep)
    }
}

impl From<SofiaGateway> for Endpoint {
    fn from(ep: SofiaGateway) -> Self {
        Self::SofiaGateway(ep)
    }
}

impl From<LoopbackEndpoint> for Endpoint {
    fn from(ep: LoopbackEndpoint) -> Self {
        Self::Loopback(ep)
    }
}

impl From<UserEndpoint> for Endpoint {
    fn from(ep: UserEndpoint) -> Self {
        Self::User(ep)
    }
}

impl From<SofiaContact> for Endpoint {
    fn from(ep: SofiaContact) -> Self {
        Self::SofiaContact(ep)
    }
}

impl From<GroupCall> for Endpoint {
    fn from(ep: GroupCall) -> Self {
        Self::GroupCall(ep)
    }
}

impl From<ErrorEndpoint> for Endpoint {
    fn from(ep: ErrorEndpoint) -> Self {
        Self::Error(ep)
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl Endpoint {
    pub(crate) fn write_for(
        &self,
        f: &mut fmt::Formatter<'_>,
        target: DialStringTarget,
    ) -> fmt::Result {
        match self {
            Self::Sofia(ep) => ep.write_for(f, target),
            Self::SofiaGateway(ep) => ep.write_for(f, target),
            Self::Loopback(ep) => ep.write_for(f, target),
            Self::User(ep) => ep.write_for(f, target),
            Self::SofiaContact(ep) => ep.write_for(f, target),
            Self::GroupCall(ep) => ep.write_for(f, target),
            Self::Error(ep) => fmt::Display::fmt(ep, f),
            Self::PortAudio(ep) => ep.write_with_prefix(f, "portaudio", target),
            Self::PulseAudio(ep) => ep.write_with_prefix(f, "pulseaudio", target),
            Self::Alsa(ep) => ep.write_with_prefix(f, "alsa", target),
        }
    }

    /// Render for a named carrier or [`DialStringTarget`] rather than the
    /// [`DialStringCarrier::EslApi`] default of [`Display`](fmt::Display).
    pub fn display_for(&self, target: impl Into<DialStringTarget>) -> EndpointDisplay<'_> {
        EndpointDisplay {
            endpoint: self,
            target: target.into(),
        }
    }

    /// Parse a dial string written for a named carrier or [`DialStringTarget`],
    /// mirroring [`display_for`](Self::display_for). [`FromStr`] uses the
    /// [`DialStringCarrier::EslApi`] default.
    pub fn parse_for(s: &str, target: impl Into<DialStringTarget>) -> Result<Self, OriginateError> {
        let (argument, target) = target
            .into()
            .read_argument(s)?;
        // Take the leading block at the caller's target, then let the endpoint
        // parse what is left; re-attaching avoids every endpoint's FromStr
        // having to thread a target it would only forward.
        let (variables, rest) = extract_variables(&argument, target)?;
        let mut endpoint = Self::parse_bare(rest)?;
        if variables.is_some() {
            endpoint.set_variables(variables);
            if endpoint
                .variables()
                .is_none()
            {
                return Err(OriginateError::VariablesNotSupported(endpoint.kind()));
            }
        }
        Ok(endpoint)
    }

    /// The module name this variant renders, for diagnostics.
    fn kind(&self) -> &'static str {
        match self {
            Self::Sofia(_) => "sofia",
            Self::SofiaGateway(_) => "sofia gateway",
            Self::Loopback(_) => "loopback",
            Self::User(_) => "user",
            Self::SofiaContact(_) => "sofia_contact",
            Self::GroupCall(_) => "group_call",
            Self::Error(_) => "error",
            Self::PortAudio(_) => "portaudio",
            Self::PulseAudio(_) => "pulseaudio",
            Self::Alsa(_) => "alsa",
        }
    }

    /// Module prefix to parser, first match wins.
    const PREFIX_PARSERS: &[(&str, PrefixParser)] = &[
        ("${sofia_contact(", |u| Ok(Self::SofiaContact(u.parse()?))),
        ("${group_call(", |u| Ok(Self::GroupCall(u.parse()?))),
        ("error/", |u| Ok(Self::Error(u.parse()?))),
        ("loopback/", |u| Ok(Self::Loopback(u.parse()?))),
        // Must precede "sofia/", which also matches a gateway string.
        ("sofia/gateway/", |u| Ok(Self::SofiaGateway(u.parse()?))),
        ("sofia/", |u| Ok(Self::Sofia(u.parse()?))),
        ("user/", |u| Ok(Self::User(u.parse()?))),
        ("portaudio", |u| {
            Ok(Self::PortAudio(AudioEndpoint::parse_with_prefix(
                u,
                "portaudio",
            )?))
        }),
        ("pulseaudio", |u| {
            Ok(Self::PulseAudio(AudioEndpoint::parse_with_prefix(
                u,
                "pulseaudio",
            )?))
        }),
        ("alsa", |u| {
            Ok(Self::Alsa(AudioEndpoint::parse_with_prefix(u, "alsa")?))
        }),
    ];

    /// Dispatch on the module prefix of a dial string whose variable block has
    /// already been taken off.
    pub(crate) fn parse_bare(uri: &str) -> Result<Self, OriginateError> {
        let (_, parse) = Self::PREFIX_PARSERS
            .iter()
            .find(|(prefix, _)| uri.starts_with(prefix))
            .ok_or_else(|| OriginateError::UnknownEndpointType(uri.to_string()))?;
        parse(uri)
    }
}

/// Renders an [`Endpoint`] for one target. Returned by
/// [`Endpoint::display_for`].
#[derive(Debug, Clone, Copy)]
pub struct EndpointDisplay<'a> {
    endpoint: &'a Endpoint,
    target: DialStringTarget,
}

impl fmt::Display for EndpointDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.target
            .write_argument(f, |f, target| {
                self.endpoint
                    .write_for(f, target)
            })
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_for(f, DialStringCarrier::EslApi.into())
    }
}

// ---------------------------------------------------------------------------
// FromStr
// ---------------------------------------------------------------------------

impl FromStr for Endpoint {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_for(s, DialStringCarrier::EslApi)
    }
}

// ---------------------------------------------------------------------------
// DialString impls
// ---------------------------------------------------------------------------

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
        forward_to_variant!(self, variables)
    }
    fn variables_mut(&mut self) -> Option<&mut Variables> {
        forward_to_variant!(self, variables_mut)
    }
    fn set_variables(&mut self, vars: Option<Variables>) {
        forward_to_variant!(self, set_variables, vars)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::variables::VariablesType;

    // --- extract_variables depth-aware bracket matching ---

    #[test]
    fn extract_variables_nested_angle_brackets() {
        let (vars, rest) = extract_variables(
            "<sip_h_Call-Info=<url>>sofia/gw/x",
            DialStringCarrier::EslApi.into(),
        )
        .unwrap();
        assert_eq!(rest, "sofia/gw/x");
        assert!(vars.is_some());
    }

    #[test]
    fn extract_variables_nested_curly_brackets() {
        let (vars, rest) = extract_variables(
            "{a={b}}sofia/internal/1000",
            DialStringCarrier::EslApi.into(),
        )
        .unwrap();
        assert_eq!(rest, "sofia/internal/1000");
        assert!(vars.is_some());
    }

    #[test]
    fn extract_variables_unclosed_returns_error() {
        let result = extract_variables("{a=b", DialStringCarrier::EslApi.into());
        assert!(result.is_err());
    }

    // --- Endpoint enum FromStr dispatch ---

    #[test]
    fn endpoint_from_str_sofia() {
        let ep: Endpoint = "sofia/internal/1000@example.com"
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
        let ep: Endpoint = "user/1000@example.com"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::User(_)));
    }

    #[test]
    fn endpoint_from_str_sofia_contact() {
        let ep: Endpoint = "${sofia_contact(1000@example.com)}"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::SofiaContact(_)));
    }

    #[test]
    fn endpoint_from_str_group_call() {
        let ep: Endpoint = "${group_call(support@example.com+A)}"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::GroupCall(_)));
    }

    #[test]
    fn endpoint_from_str_error() {
        let ep: Endpoint = "error/USER_BUSY"
            .parse()
            .unwrap();
        assert!(matches!(ep, Endpoint::Error(_)));
    }

    /// `ErrorEndpoint` has nowhere to keep a block, so accepting one loses
    /// every variable it named without a word to the caller.
    #[test]
    fn a_block_on_an_endpoint_that_cannot_hold_one_is_refused() {
        for carrier in [DialStringCarrier::EslApi, DialStringCarrier::Dialplan] {
            let msg = Endpoint::parse_for("{a=b}error/USER_BUSY", carrier)
                .expect_err(&format!("accepted at {carrier:?}"))
                .to_string();
            assert!(msg.contains("error"), "does not name the type: {msg}");
        }
        assert!("{a=b}error/USER_BUSY"
            .parse::<Endpoint>()
            .is_err());
    }

    /// The two entry points have to agree: `from_str` is `parse_for` at the
    /// default carrier, not a second dispatch with its own rules.
    #[test]
    fn from_str_matches_parse_for_at_the_default_carrier() {
        for input in [
            "sofia/internal/1000@example.com",
            "{a=b}sofia/internal/1000@example.com",
            "<a=b>loopback/9199/default",
            "[a=b]user/bob@example.com",
            "{a=b}error/USER_BUSY",
            "verto/1234",
        ] {
            assert_eq!(
                input
                    .parse::<Endpoint>()
                    .is_ok(),
                Endpoint::parse_for(input, DialStringCarrier::EslApi).is_ok(),
                "{input}"
            );
        }
    }

    /// A target naming only a carrier is that carrier: both spellings render and
    /// parse identically.
    #[test]
    fn a_target_and_its_bare_carrier_agree() {
        use crate::commands::variables::{BlockParse, DialStringTarget};

        let input = r"{a=it\\\\\\'s}sofia/internal/1000@example.com";
        let target = DialStringTarget::new(DialStringCarrier::Dialplan)
            .with_block_parse(BlockParse::PairSplitCleans);
        let by_target = Endpoint::parse_for(input, target).unwrap();
        let by_carrier = Endpoint::parse_for(input, DialStringCarrier::Dialplan).unwrap();
        assert_eq!(by_target, by_carrier);
        assert_eq!(
            by_target
                .display_for(target)
                .to_string(),
            input
        );
    }

    /// The leg split and the block parse skip and trim spaces only, so any other whitespace
    /// the switch keeps is endpoint text.
    #[test]
    fn whitespace_other_than_a_space_is_kept() {
        let ep = Endpoint::parse_for("<v0==>loopback/\u{b}", DialStringCarrier::EslApi).unwrap();
        let Endpoint::Loopback(loopback) = &ep else {
            panic!("expected Loopback: {ep:?}");
        };
        assert_eq!(loopback.extension, "\u{b}");

        assert!(Variables::parse_for("\u{b}{k=v}", DialStringCarrier::Dialplan).is_err());

        let bridge = crate::commands::BridgeDialString::parse_with(
            "loopback/9199/test\t",
            crate::commands::BlockParse::PairSplitCleans,
        )
        .unwrap();
        assert_eq!(bridge.to_string(), "loopback/9199/test\t");
    }

    #[test]
    fn endpoint_from_str_unknown_errors() {
        let result = "verto/1234".parse::<Endpoint>();
        assert!(result.is_err());
    }

    #[test]
    fn endpoint_from_str_with_variables() {
        let ep: Endpoint = "{timeout=30}sofia/internal/1000@example.com"
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

    // --- Display delegation ---

    #[test]
    fn endpoint_display_delegates_to_inner() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000@example.com".into(),
            variables: None,
        });
        assert_eq!(ep.to_string(), "sofia/internal/1000@example.com");
    }

    // --- DialString trait ---

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
        let ep = ErrorEndpoint::new(crate::channel::HangupCause::UserBusy);
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

    // --- Serde: Endpoint enum ---

    /// One case per variant: the externally tagged name is the config key a
    /// deployment writes, so a rename is a break and has to show up here.
    #[test]
    fn serde_endpoint_enum_tags_round_trip() {
        let cases: [(&str, Endpoint); 7] = [
            (
                "sofia",
                SofiaEndpoint::new("internal", "1000@example.com").into(),
            ),
            ("sofia_gateway", SofiaGateway::new("gw1", "1234").into()),
            (
                "loopback",
                LoopbackEndpoint::new("9199")
                    .with_context("default")
                    .into(),
            ),
            (
                "user",
                UserEndpoint::new("bob")
                    .with_domain("example.com")
                    .into(),
            ),
            (
                "sofia_contact",
                SofiaContact::new("1000", "example.com").into(),
            ),
            (
                "group_call",
                GroupCall::new("support", "example.com")
                    .with_order(GroupCallOrder::All)
                    .into(),
            ),
            (
                "error",
                ErrorEndpoint::new(crate::channel::HangupCause::UserBusy).into(),
            ),
        ];
        for (tag, ep) in cases {
            let json = serde_json::to_string(&ep).unwrap();
            assert!(json.contains(&format!("\"{tag}\"")), "{tag}: {json}");
            assert_eq!(serde_json::from_str::<Endpoint>(&json).unwrap(), ep);
        }
    }

    #[test]
    fn serde_endpoint_skips_none_variables() {
        let ep = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000".into(),
            variables: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(!json.contains("variables"));
    }

    #[test]
    fn serde_endpoint_skips_none_profile() {
        let ep = SofiaGateway {
            gateway: "gw".into(),
            destination: "1234".into(),
            profile: None,
            variables: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(!json.contains("profile"));
    }

    // --- Audio endpoints through Endpoint enum ---

    /// The three audio modules share one struct and differ only in the prefix
    /// their variant supplies, so each row has to name its own module.
    #[test]
    fn audio_endpoints_render_and_parse_per_module() {
        type Variant = fn(AudioEndpoint) -> Endpoint;
        let cases: [(Variant, &str, &str); 6] = [
            (Endpoint::PortAudio, "portaudio", "portaudio/auto_answer"),
            (Endpoint::PortAudio, "portaudio", "portaudio"),
            (Endpoint::PulseAudio, "pulseaudio", "pulseaudio/auto_answer"),
            (Endpoint::PulseAudio, "pulseaudio", "pulseaudio"),
            (Endpoint::Alsa, "alsa", "alsa/auto_answer"),
            (Endpoint::Alsa, "alsa", "alsa"),
        ];
        for (variant, module, wire) in cases {
            let destination = wire
                .strip_prefix(module)
                .and_then(|rest| rest.strip_prefix('/'))
                .map(str::to_string);
            let ep = variant(AudioEndpoint {
                destination: destination.clone(),
                variables: None,
            });
            assert_eq!(ep.to_string(), wire);

            let parsed: Endpoint = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, ep, "{wire}");
            assert!(
                parsed
                    .variables()
                    .is_none(),
                "{wire}"
            );

            let json = serde_json::to_string(&ep).unwrap();
            assert!(json.contains(&format!("\"{module}\"")), "{wire}: {json}");
            assert_eq!(serde_json::from_str::<Endpoint>(&json).unwrap(), ep);
        }
    }

    #[test]
    fn audio_endpoint_carries_variables() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("codec", "PCMU");
        let ep = Endpoint::PortAudio(
            AudioEndpoint::new()
                .with_destination("auto_answer")
                .with_variables(vars),
        );
        assert_eq!(ep.to_string(), "{codec=PCMU}portaudio/auto_answer");
        assert_eq!(
            ep.to_string()
                .parse::<Endpoint>()
                .unwrap(),
            ep
        );
    }

    /// A renderer and a parser that disagree on the carrier hand back a
    /// different value than was put in — silently, for every endpoint type.
    #[test]
    fn every_endpoint_type_round_trips_at_the_dialplan_carrier() {
        let mut vars = Variables::new(VariablesType::Channel);
        vars.insert("path", r"C:\path");
        vars.insert("other", "a,b");

        let with_vars = |mut ep: Endpoint| {
            ep.set_variables(Some(vars.clone()));
            ep
        };
        let cases: [Endpoint; 10] = [
            with_vars(SofiaEndpoint::new("internal", "1000@example.com").into()),
            with_vars(
                SofiaGateway::new("gw", "1234")
                    .with_profile("external")
                    .into(),
            ),
            with_vars(
                LoopbackEndpoint::new("9199")
                    .with_context("default")
                    .into(),
            ),
            with_vars(
                UserEndpoint::new("bob")
                    .with_domain("example.com")
                    .into(),
            ),
            with_vars(
                SofiaContact::new("1000", "example.com")
                    .with_profile("*")
                    .into(),
            ),
            with_vars(
                GroupCall::new("support", "example.com")
                    .with_order(GroupCallOrder::All)
                    .into(),
            ),
            ErrorEndpoint::new(crate::channel::HangupCause::UserBusy).into(),
            with_vars(Endpoint::PortAudio(
                AudioEndpoint::new().with_destination("auto_answer"),
            )),
            with_vars(Endpoint::PulseAudio(AudioEndpoint::new())),
            with_vars(Endpoint::Alsa(
                AudioEndpoint::new().with_destination("auto_answer"),
            )),
        ];

        for ep in cases {
            let rendered = ep
                .display_for(DialStringCarrier::Dialplan)
                .to_string();
            let back = Endpoint::parse_for(&rendered, DialStringCarrier::Dialplan)
                .unwrap_or_else(|e| panic!("{rendered} failed to parse: {e}"));
            assert_eq!(back, ep, "rendered {rendered}");
        }
    }

    #[test]
    fn every_endpoint_type_round_trips_at_an_argv_separator() {
        let target = DialStringTarget::new(DialStringCarrier::EslApi)
            .with_argv_separator('~')
            .expect("'~' separates originate's arguments");
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("path", r"C:\path");
        vars.insert("tilde", "x~y");
        vars.insert("spaced", "a b");
        vars.insert("quote", "it's");

        let with_vars = |mut ep: Endpoint| {
            ep.set_variables(Some(vars.clone()));
            ep
        };
        let cases: [Endpoint; 8] = [
            with_vars(SofiaEndpoint::new("internal", "1000@example.com").into()),
            with_vars(
                SofiaGateway::new("gw", "1234")
                    .with_profile("external")
                    .into(),
            ),
            with_vars(
                LoopbackEndpoint::new("9199")
                    .with_context("default")
                    .into(),
            ),
            with_vars(
                UserEndpoint::new("bob")
                    .with_domain("example.com")
                    .into(),
            ),
            with_vars(SofiaContact::new("1000", "example.com").into()),
            with_vars(GroupCall::new("support", "example.com").into()),
            ErrorEndpoint::new(crate::channel::HangupCause::UserBusy).into(),
            with_vars(Endpoint::Alsa(
                AudioEndpoint::new().with_destination("auto_answer"),
            )),
        ];

        for ep in cases {
            let rendered = ep
                .display_for(target)
                .to_string();
            let back = Endpoint::parse_for(&rendered, target)
                .unwrap_or_else(|e| panic!("{rendered} failed to parse: {e}"));
            assert_eq!(back, ep, "rendered {rendered}");
        }
    }

    #[test]
    fn an_endpoint_cut_by_its_argv_separator_is_refused() {
        let target = DialStringTarget::new(DialStringCarrier::EslApi)
            .with_argv_separator('~')
            .expect("'~' separates originate's arguments");
        assert!(Endpoint::parse_for("loopback/9199/test~error/USER_BUSY", target).is_err());
        assert!(Endpoint::parse_for(r"{k=x\~y}loopback/9199/test", target).is_ok());
    }

    #[test]
    fn an_endpoint_cut_by_the_blank_split_is_refused() {
        for input in [
            "{v=a b}loopback/9199/test",
            "loopback/9199/test error/USER_BUSY",
            r"{v=x\\'y}loopback/9199/test",
        ] {
            assert!(
                Endpoint::parse_for(input, DialStringCarrier::EslApi).is_err(),
                "{input}"
            );
        }
        assert!(
            Endpoint::parse_for("{v='a b'}loopback/9199/test", DialStringCarrier::EslApi).is_ok()
        );
        assert!(
            Endpoint::parse_for("{v=a b}loopback/9199/test", DialStringCarrier::Dialplan).is_ok()
        );
    }

    // --- From impls ---

    #[test]
    fn from_sofia_endpoint() {
        let inner = SofiaEndpoint {
            profile: "internal".into(),
            destination: "1000@example.com".into(),
            variables: None,
        };
        let ep: Endpoint = inner
            .clone()
            .into();
        assert_eq!(ep, Endpoint::Sofia(inner));
    }

    #[test]
    fn from_sofia_gateway() {
        let inner = SofiaGateway {
            gateway: "gw1".into(),
            destination: "1234".into(),
            profile: None,
            variables: None,
        };
        let ep: Endpoint = inner
            .clone()
            .into();
        assert_eq!(ep, Endpoint::SofiaGateway(inner));
    }

    #[test]
    fn from_loopback_endpoint() {
        let inner = LoopbackEndpoint::new("9199").with_context("default");
        let ep: Endpoint = inner
            .clone()
            .into();
        assert_eq!(ep, Endpoint::Loopback(inner));
    }

    #[test]
    fn from_user_endpoint() {
        let inner = UserEndpoint {
            name: "bob".into(),
            domain: Some("example.com".into()),
            variables: None,
        };
        let ep: Endpoint = inner
            .clone()
            .into();
        assert_eq!(ep, Endpoint::User(inner));
    }

    #[test]
    fn from_sofia_contact() {
        let inner = SofiaContact {
            user: "1000".into(),
            domain: "example.com".into(),
            profile: None,
            variables: None,
        };
        let ep: Endpoint = inner
            .clone()
            .into();
        assert_eq!(ep, Endpoint::SofiaContact(inner));
    }

    #[test]
    fn from_group_call() {
        let inner = GroupCall::new("support", "example.com").with_order(GroupCallOrder::All);
        let ep: Endpoint = inner
            .clone()
            .into();
        assert_eq!(ep, Endpoint::GroupCall(inner));
    }

    #[test]
    fn from_error_endpoint() {
        let inner = ErrorEndpoint::new(crate::channel::HangupCause::UserBusy);
        let ep: Endpoint = inner.into();
        assert_eq!(ep, Endpoint::Error(inner));
    }

    // --- Endpoint text through the leg splits ---

    fn tilde() -> DialStringTarget {
        DialStringTarget::new(DialStringCarrier::EslApi)
            .with_argv_separator('~')
            .expect("'~' separates originate's arguments")
    }

    /// Endpoint text meets the carrier's pass and both leg splits, so it escapes like a `{}`
    /// value with the leg separators added.
    #[test]
    fn endpoint_text_is_escaped_for_the_leg_splits() {
        let api = DialStringTarget::new(DialStringCarrier::EslApi);
        let dialplan = DialStringTarget::new(DialStringCarrier::Dialplan);
        let separators: Endpoint = LoopbackEndpoint::new("a,b")
            .with_context("c|d")
            .into();
        let cases: [(DialStringTarget, Endpoint, &str); 9] = [
            (api, separators.clone(), r"loopback/a\,b/c\|d"),
            (dialplan, separators, r"loopback/a\,b/c\|d"),
            (
                api,
                LoopbackEndpoint::new(r"C:\x").into(),
                r"loopback/C:\\\\\\\\x",
            ),
            (
                api,
                LoopbackEndpoint::new("it's").into(),
                r"loopback/it\\\\\\\'s",
            ),
            (
                dialplan,
                LoopbackEndpoint::new("it's").into(),
                r"loopback/it\\\\\\'s",
            ),
            (
                api,
                SofiaEndpoint::new("internal", "a b").into(),
                "'sofia/internal/a b'",
            ),
            (
                api,
                UserEndpoint::new("bob")
                    .with_domain("end ")
                    .into(),
                r"user/bob@end\\\\s",
            ),
            (
                dialplan,
                SofiaEndpoint::new("internal", "pa$$").into(),
                r"\'sofia/internal/pa\$\$",
            ),
            (
                dialplan,
                SofiaEndpoint::new("internal", "${v}").into(),
                r"\'sofia/internal/\${v}",
            ),
        ];
        for (target, ep, want) in cases {
            assert_eq!(
                ep.display_for(target)
                    .to_string(),
                want,
                "{ep:?} at {target:?}"
            );
        }
    }

    const HOSTILE_FIELDS: &[&str] = &[
        "a b", "it's", r"C:\p", "x,y", "p|q", " edge ", "pa$$", "${v}", "x~y", "q\"r", "[b]",
        "tab\t", r"a\,b",
    ];

    fn hostile_endpoints(field: &str) -> [Endpoint; 5] {
        [
            SofiaEndpoint::new("internal", field).into(),
            SofiaGateway::new("gw", field)
                .with_profile("external")
                .into(),
            LoopbackEndpoint::new(field)
                .with_context(field)
                .into(),
            UserEndpoint::new(field)
                .with_domain(field)
                .into(),
            Endpoint::Alsa(AudioEndpoint::new().with_destination(field)),
        ]
    }

    /// The port of the switch's passes reads back the module text, and the parser the endpoint.
    #[test]
    fn hostile_fields_arrive_and_round_trip_at_every_target() {
        use crate::commands::flattened::pipeline;

        let targets = [
            DialStringTarget::new(DialStringCarrier::EslApi),
            DialStringTarget::new(DialStringCarrier::Dialplan),
            tilde(),
        ];
        for field in HOSTILE_FIELDS {
            for ep in hostile_endpoints(field) {
                for target in targets {
                    let rendered = ep
                        .display_for(target)
                        .to_string();
                    let list = pipeline::read(&rendered, target)
                        .unwrap_or_else(|e| panic!("{rendered:?} at {target:?}: {e:?}"));
                    let legs: Vec<&str> = list
                        .threads
                        .iter()
                        .flat_map(|thread| &thread.groups)
                        .flatten()
                        .map(|leg| {
                            leg.endpoint
                                .as_str()
                        })
                        .collect();
                    assert_eq!(legs, [ep.module_text()], "{rendered:?} at {target:?}");
                    assert_eq!(
                        Endpoint::parse_for(&rendered, target)
                            .unwrap_or_else(|e| panic!("{rendered:?} at {target:?}: {e}")),
                        ep,
                        "{rendered:?} at {target:?}"
                    );
                }
            }
        }
    }

    /// Each value carries `SECRET`, which no refusal may quote.
    #[test]
    fn fields_the_switch_cannot_receive_are_refused_at_parse() {
        for input in [
            "sofia/GATEWAY/SECRET/1000",
            "sofia/SECRET^x/1000",
            "sofia/gateway/SECRET^x/1000",
            "loopback/SECRET//xml",
            "loopback/SECRET/test/",
            "loopback/SECRET:_:x/test",
            "user/SECRET:_:x@example.com",
        ] {
            let msg = Endpoint::parse_for(input, DialStringCarrier::Dialplan)
                .expect_err(input)
                .to_string();
            assert!(
                !msg.contains("SECRET"),
                "{input}: error quoted its input: {msg}"
            );
        }
    }

    #[test]
    fn fields_the_switch_cannot_receive_are_refused_at_config_load() {
        for json in [
            r#"{"sofia":{"profile":"SECRET/x","destination":"1"}}"#,
            r#"{"sofia":{"profile":"GateWay","destination":"SECRET"}}"#,
            r#"{"sofia":{"profile":"SECRET^x","destination":"1"}}"#,
            r#"{"sofia":{"profile":"internal","destination":"SECRET:_:x"}}"#,
            r#"{"sofia_gateway":{"gateway":"SECRET::x","destination":"1"}}"#,
            r#"{"sofia_gateway":{"gateway":"g","profile":"SECRET::x","destination":"1"}}"#,
            r#"{"sofia_gateway":{"gateway":"g","profile":"SECRET:","destination":"1"}}"#,
            r#"{"sofia_gateway":{"gateway":"SECRET/x","destination":"1"}}"#,
            r#"{"sofia_gateway":{"gateway":"SECRET^x","destination":"1"}}"#,
            r#"{"loopback":{"extension":"SECRET/x"}}"#,
            r#"{"loopback":{"extension":"SECRET","context":""}}"#,
            r#"{"loopback":{"extension":"9199","context":"SECRET/x"}}"#,
            r#"{"loopback":{"extension":"SECRET","dialplan":""}}"#,
            r#"{"loopback":{"extension":"app=bridge:SECRET","context":"test"}}"#,
            r#"{"loopback":{"extension":"APP=SECRET/x:y"}}"#,
            r#"{"user":{"name":"SECRET@x","domain":"example.com"}}"#,
            r#"{"user":{"name":"bob","domain":"SECRET:_:x"}}"#,
            r#"{"portaudio":{"destination":"SECRET:_:x"}}"#,
        ] {
            let msg = serde_json::from_str::<Endpoint>(json)
                .expect_err(json)
                .to_string();
            assert!(
                !msg.contains("SECRET"),
                "{json}: error quoted its input: {msg}"
            );
        }
        for json in [
            r#"{"loopback":{"extension":"app=bridge:null/farend"}}"#,
            r#"{"sofia":{"profile":"internal","destination":"sip:a/b^c@example.com"}}"#,
            r#"{"sofia":{"profile":"a::b","destination":"1"}}"#,
            r#"{"sofia_gateway":{"gateway":":g","profile":"p","destination":"1"}}"#,
            r#"{"sofia_gateway":{"gateway":"g::h","profile":"p","destination":"1"}}"#,
            r#"{"loopback":{"extension":"9199","dialplan":"a/b"}}"#,
            r#"{"user":{"name":"bob","domain":"a@b"}}"#,
        ] {
            assert!(serde_json::from_str::<Endpoint>(json).is_ok(), "{json}");
        }
        assert!(
            serde_json::from_str::<LoopbackEndpoint>(r#"{"extension":"SECRET","context":""}"#)
                .is_err()
        );
    }

    /// mod_loopback runs `app=<name>[:<args>]` and reads no context or dialplan after it, so the
    /// whole text is the extension.
    #[test]
    fn a_loopback_application_is_one_extension() {
        let ep: LoopbackEndpoint = "loopback/app=bridge:null/farend"
            .parse()
            .unwrap();
        assert_eq!(ep.extension, "app=bridge:null/farend");
        assert_eq!(ep.context, None);
        assert_eq!(ep.to_string(), "loopback/app=bridge:null/farend");
    }

    /// `:_:` splits the dial string into threads, so no endpoint carries it.
    #[test]
    fn the_enterprise_separator_is_refused_in_any_field() {
        for input in ["loopback/9199/SECRET:_:x", "{k=v}sofia/internal/SECRET:_:x"] {
            let msg = Endpoint::parse_for(input, DialStringCarrier::EslApi)
                .expect_err(input)
                .to_string();
            assert!(!msg.contains("SECRET"), "{input}: {msg}");
        }
    }
}
