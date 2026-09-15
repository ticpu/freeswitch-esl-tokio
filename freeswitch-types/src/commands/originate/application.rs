//! The originate command's target and dialplan: the application or extension it runs, and the
//! dialplan module a transfer names.

use std::fmt;
use std::str::FromStr;

/// FreeSWITCH dialplan type for originate commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[non_exhaustive]
pub enum DialplanType {
    /// Inline dialplan: applications execute directly without XML lookup.
    Inline,
    /// XML dialplan: route through the XML dialplan engine.
    Xml,
}

impl DialplanType {
    pub(crate) fn wire_name(&self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Xml => "XML",
        }
    }
}

impl fmt::Display for DialplanType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire_name())
    }
}

/// The dialplan slot: a [`DialplanType`], or the name of a dialplan module it does not cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Dialplan {
    Typed(DialplanType),
    Named(String),
}

impl Dialplan {
    pub(crate) fn from_name(name: String) -> Self {
        match name.parse() {
            Ok(dp) => Self::Typed(dp),
            Err(_) => Self::Named(name),
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Typed(dp) => dp.wire_name(),
            Self::Named(name) => name,
        }
    }

    pub(crate) fn typed(&self) -> Option<&DialplanType> {
        match self {
            Self::Typed(dp) => Some(dp),
            Self::Named(_) => None,
        }
    }
}

parse_error! { ParseDialplanTypeError("dialplan type"); }

impl FromStr for DialplanType {
    type Err = ParseDialplanTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("inline") {
            Ok(Self::Inline)
        } else if s.eq_ignore_ascii_case("xml") {
            Ok(Self::Xml)
        } else {
            Err(ParseDialplanTypeError(s.to_string()))
        }
    }
}

/// A single dialplan application with optional arguments.
///
/// Formats differently depending on [`DialplanType`]:
/// - Inline: `name` or `name:args`
/// - XML: `&name(args)`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Application {
    name: String,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    args: Option<String>,
}

impl Application {
    /// Create an application with optional arguments.
    pub fn new(name: impl Into<String>, args: Option<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            args: args.map(|a| a.into()),
        }
    }

    /// Create an application with no arguments.
    pub fn simple(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: None,
        }
    }

    /// Park the channel (hold in place without bridging).
    pub fn park() -> Self {
        Self::simple("park")
    }

    /// Application name (e.g. `park`, `conference`, `socket`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Application arguments, if any.
    pub fn args(&self) -> Option<&str> {
        self.args
            .as_deref()
    }

    /// Mutable reference to the application name.
    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    /// Mutable reference to the application arguments.
    pub fn args_mut(&mut self) -> &mut Option<String> {
        &mut self.args
    }

    /// Format as inline (`name:args`) or XML (`&name(args)`) syntax.
    pub fn to_string_with_dialplan(&self, dialplan: &DialplanType) -> String {
        match dialplan {
            DialplanType::Inline => match &self.args {
                Some(args) => format!("{}:{}", self.name, args),
                None => self
                    .name
                    .clone(),
            },
            DialplanType::Xml => {
                let args = self
                    .args
                    .as_deref()
                    .unwrap_or("");
                format!("&{}({})", self.name, args)
            }
        }
    }
}

/// The target of an originate command: either a dialplan extension or
/// application(s) to execute directly.
///
/// FreeSWITCH syntax: `originate <endpoint> <target> [dialplan] ...`
/// where `<target>` is either a bare extension string (routes through
/// the dialplan engine) or `&app(args)` / `app:args` (executes inline).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum OriginateTarget {
    /// Route through the dialplan engine to this extension.
    Extension(String),
    /// Single application for XML dialplan: `&app(args)`.
    Application(Application),
    /// One or more applications for inline dialplan: `app:args,app:args`.
    InlineApplications(Vec<Application>),
}

impl From<Application> for OriginateTarget {
    fn from(app: Application) -> Self {
        Self::Application(app)
    }
}

impl From<Vec<Application>> for OriginateTarget {
    fn from(apps: Vec<Application>) -> Self {
        Self::InlineApplications(apps)
    }
}
