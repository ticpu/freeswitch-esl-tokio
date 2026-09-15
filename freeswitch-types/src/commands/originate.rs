//! Originate command builder with endpoint configuration, variable scoping,
//! and automatic quoting for socket application arguments.

use std::fmt::{self, Write as _};
use std::num::ParseIntError;
use std::str::FromStr;
use std::time::Duration;

use super::endpoint::{EndpointFieldFault, ParseGroupCallOrderError};
use super::variables::{
    breaks_a_split, write_escaped, BlockParse, DialStringCarrier, DialStringTarget,
    InvalidArgvSeparator,
};
use super::{clean_argument, originate_quote, originate_split, STRIPPED_WHITESPACE};
use crate::channel::ParseHangupCauseError;
use crate::tokenizer::{delimiter_override, trace};

pub use super::variables::{Variables, VariablesType};

/// FreeSWITCH keyword for omitted positional arguments.
///
/// `switch_separate_string` converts `"undef"` to NULL, making it the
/// canonical placeholder when a later positional arg forces earlier ones
/// to be present on the wire.
const UNDEF: &str = "undef";

/// The context FreeSWITCH itself falls back to, emitted when a later
/// positional argument forces the slot to be present.
pub(super) const DEFAULT_CONTEXT: &str = "default";

/// The separator an inline action list uses unless one is named.
pub(super) const DEFAULT_INLINE_DELIMITER: char = ',';

/// Reject an inline separator the hunt's split breaks on, or `:`, where it splits an application
/// from its data.
pub(super) fn check_inline_delimiter(delimiter: char) -> Result<(), OriginateError> {
    if delimiter == ':' || breaks_a_split(delimiter) {
        return Err(OriginateError::InvalidInlineDelimiter(delimiter));
    }
    Ok(())
}

/// Reject a target `originate_function` reads as something else: it runs any target opening `&`
/// and more as an application, and ends that application's arguments at the first `)`.
pub(super) fn check_target_readable(target: &OriginateTarget) -> Result<(), OriginateError> {
    match target {
        OriginateTarget::Extension(extension)
            if extension.len() > 1 && extension.starts_with('&') =>
        {
            Err(OriginateError::ExtensionReadsAsApplication)
        }
        OriginateTarget::Application(app)
            if app
                .name()
                .contains(['(', ')'])
                || app
                    .args()
                    .is_some_and(|args| args.contains(')')) =>
        {
            Err(OriginateError::ParenthesisInApplication {
                application: app
                    .name()
                    .to_string(),
            })
        }
        _ => Ok(()),
    }
}

/// Reject a dialplan `target` cannot run under: `originate_function` hands the target to the
/// inline hunt only under `inline`, so an extension there and an action list elsewhere misfire.
fn check_dialplan_fits(
    target: &OriginateTarget,
    dialplan: Option<&Dialplan>,
) -> Result<(), OriginateError> {
    let inline = matches!(dialplan, Some(Dialplan::Typed(DialplanType::Inline)));
    match target {
        OriginateTarget::Extension(_) if inline => Err(OriginateError::ExtensionWithInlineDialplan),
        OriginateTarget::InlineApplications(_) if dialplan.is_some() && !inline => {
            Err(OriginateError::InlineApplicationsWithDialplan)
        }
        _ => Ok(()),
    }
}

/// Render `apps` as one inline action list, each action escaped once for the split
/// `inline_dialplan_hunt` runs on `delimiter`, whose cleanup reads the escapes and trims
/// the action's edges; the originate line's own split escapes the whole list again.
fn render_inline(apps: &[Application], delimiter: char) -> String {
    let mut list = String::new();
    for (i, app) in apps
        .iter()
        .enumerate()
    {
        if i > 0 {
            list.push(delimiter);
        }
        // Writing to a String cannot fail.
        let _ = write_escaped(
            &mut list,
            delimiter,
            app.to_string_with_dialplan(&DialplanType::Inline),
        );
    }
    list
}

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
    fn wire_name(&self) -> &'static str {
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
enum Dialplan {
    Typed(DialplanType),
    Named(String),
}

impl Dialplan {
    fn from_name(name: String) -> Self {
        match name.parse() {
            Ok(dp) => Self::Typed(dp),
            Err(_) => Self::Named(name),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Typed(dp) => dp.wire_name(),
            Self::Named(name) => name,
        }
    }

    fn typed(&self) -> Option<&DialplanType> {
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

pub use super::endpoint::Endpoint;

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

/// Originate command builder: `originate <endpoint> <target> [dialplan] [context] [cid_name] [cid_num] [timeout]`.
///
/// Constructed via [`Originate::extension`], [`Originate::application`], or
/// [`Originate::inline`]. Invalid states (Extension + Inline dialplan, inline
/// apps under another dialplan, empty inline apps) are rejected at construction
/// time rather than at `Display`.
///
/// Optional fields are set via consuming-self chaining methods:
///
/// ```
/// # use std::time::Duration;
/// # use freeswitch_types::commands::*;
/// let cmd = Originate::application(
///     Endpoint::Loopback(LoopbackEndpoint::new("9196").with_context("default")),
///     Application::simple("park"),
/// )
/// .cid_name("Alice")
/// .cid_num("5551234")
/// .timeout(Duration::from_secs(30));
/// ```
///
/// [`with_argv_separator`](Self::with_argv_separator) splits the arguments on a separator
/// instead of spaces. An `Originate` dials one [`Endpoint`], so a multi-leg list read into a
/// [`FlattenedDialString`](super::FlattenedDialString) goes out on the caller's own line
/// through its [`display_raw`](super::FlattenedDialString::display_raw).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Originate {
    endpoint: Endpoint,
    target: OriginateTarget,
    dialplan: Option<Dialplan>,
    context: Option<String>,
    cid_name: Option<String>,
    cid_num: Option<String>,
    timeout: Option<Duration>,
    /// `None` renders the conventional comma-separated list; `Some` prefixes
    /// the target with `m:<delim>:` so the hunt splits on that instead.
    inline_delimiter: Option<char>,
    /// `Some` opens the line with `^^<sep>` and splits every argument on it.
    argv_separator: Option<char>,
}

#[cfg(feature = "serde")]
mod serde_support {
    use super::*;

    /// A config's dialplan: a [`DialplanType`] spelled as its serde form, or any other name.
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(untagged)]
    pub(super) enum DialplanField {
        Typed(DialplanType),
        Named(String),
    }

    /// Intermediate type for serde, mirroring the old public-field layout.
    #[derive(serde::Serialize, serde::Deserialize)]
    pub(super) struct OriginateRaw {
        pub endpoint: Endpoint,
        #[serde(flatten)]
        pub target: OriginateTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub dialplan: Option<DialplanField>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub context: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cid_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cid_num: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub timeout_secs: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub argv_separator: Option<char>,
    }

    impl TryFrom<OriginateRaw> for Originate {
        type Error = OriginateError;

        fn try_from(raw: OriginateRaw) -> Result<Self, Self::Error> {
            let dialplan = raw
                .dialplan
                .map(|field| match field {
                    DialplanField::Typed(dp) => Dialplan::Typed(dp),
                    DialplanField::Named(name) => Dialplan::from_name(name),
                });
            check_dialplan_fits(&raw.target, dialplan.as_ref())?;
            if dialplan
                .as_ref()
                .is_some_and(|dp| {
                    dp.name()
                        .eq_ignore_ascii_case(UNDEF)
                })
            {
                return Err(OriginateError::UndefPositional("dialplan"));
            }
            if let OriginateTarget::InlineApplications(ref apps) = raw.target {
                if apps.is_empty() {
                    return Err(OriginateError::EmptyInlineApplications);
                }
            }
            for (field, value) in [
                ("context", &raw.context),
                ("cid_name", &raw.cid_name),
                ("cid_num", &raw.cid_num),
            ] {
                if value
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(UNDEF))
                {
                    return Err(OriginateError::UndefPositional(field));
                }
            }
            let originate = Self {
                endpoint: raw.endpoint,
                target: raw.target,
                dialplan,
                context: raw.context,
                cid_name: raw.cid_name,
                cid_num: raw.cid_num,
                timeout: raw
                    .timeout_secs
                    .map(Duration::from_secs),
                // A config file names applications, not a wire separator.
                inline_delimiter: None,
                argv_separator: None,
            };
            if originate
                .target_text()
                .eq_ignore_ascii_case(UNDEF)
            {
                return Err(OriginateError::UndefPositional("target"));
            }
            check_target_readable(&originate.target)?;
            match raw.argv_separator {
                Some(sep) => originate.with_argv_separator(sep),
                None => Ok(originate),
            }
        }
    }

    impl From<Originate> for OriginateRaw {
        fn from(o: Originate) -> Self {
            Self {
                endpoint: o.endpoint,
                target: o.target,
                dialplan: o
                    .dialplan
                    .map(|dp| match dp {
                        Dialplan::Typed(dp) => DialplanField::Typed(dp),
                        Dialplan::Named(name) => DialplanField::Named(name),
                    }),
                context: o.context,
                cid_name: o.cid_name,
                cid_num: o.cid_num,
                timeout_secs: o
                    .timeout
                    .map(|d| d.as_secs()),
                argv_separator: o.argv_separator,
            }
        }
    }

    impl serde::Serialize for Originate {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            OriginateRaw::from(self.clone()).serialize(serializer)
        }
    }

    impl<'de> serde::Deserialize<'de> for Originate {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let raw = OriginateRaw::deserialize(deserializer)?;
            Originate::try_from(raw).map_err(serde::de::Error::custom)
        }
    }
}

impl Originate {
    /// Route through the dialplan engine to an extension.
    ///
    /// `undef` in any case aborts the switch, which asserts the target it reads as absent is
    /// set; parse and config load refuse it, this does not.
    pub fn extension(endpoint: Endpoint, extension: impl Into<String>) -> Self {
        Self {
            endpoint,
            target: OriginateTarget::Extension(extension.into()),
            dialplan: None,
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
            inline_delimiter: None,
            argv_separator: None,
        }
    }

    /// Execute a single XML-format application on the answered channel.
    ///
    /// `originate` ends the arguments at the first `)`, so an argument carrying one, such as a
    /// `tone_stream://%(500,0,800)` spec, goes through [`inline`](Self::inline).
    pub fn application(endpoint: Endpoint, app: Application) -> Self {
        Self {
            endpoint,
            target: OriginateTarget::Application(app),
            dialplan: None,
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
            inline_delimiter: None,
            argv_separator: None,
        }
    }

    /// Execute inline applications on the answered channel.
    ///
    /// FreeSWITCH's inline dialplan splits the action list on `,`, so an
    /// argument carrying one — a `tone_stream://%(500,0,800)` spec, a
    /// `{a=1,b=2}` block on a bridge target — would be read as an action
    /// boundary and silently become applications nobody wrote. Rendering
    /// escapes every occurrence, which the switch undoes when it splits.
    ///
    /// Escaping happens at render time, over the arguments as they stand then,
    /// so rewriting one afterwards — through
    /// [`args_mut`](Application::args_mut), through
    /// [`target_mut`](Self::target_mut), or by substituting into a template —
    /// cannot leave the command inconsistent.
    ///
    /// Each action is escaped for the hunt's split and again for the line's, so quotes,
    /// backslashes and edge spaces in an argument arrive as written.
    ///
    /// Returns `Err` if the iterator yields no applications.
    pub fn inline(
        endpoint: Endpoint,
        apps: impl IntoIterator<Item = Application>,
    ) -> Result<Self, OriginateError> {
        let apps: Vec<Application> = apps
            .into_iter()
            .collect();
        if apps.is_empty() {
            return Err(OriginateError::EmptyInlineApplications);
        }
        Ok(Self {
            endpoint,
            target: OriginateTarget::InlineApplications(apps),
            dialplan: None,
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
            inline_delimiter: None,
            argv_separator: None,
        })
    }

    /// Execute inline applications, naming the separator yourself.
    ///
    /// Prefer [`Originate::inline`], which uses the conventional comma. Reach
    /// for this when the exact wire form matters — matching a command a switch
    /// already logged, or keeping the form stable across a config that gets
    /// diffed. Occurrences inside an argument are escaped either way, so the
    /// choice never has to depend on the data.
    ///
    /// Returns `Err` if the iterator yields no applications, or if `delimiter`
    /// cannot separate a list: `:`, where the hunt splits an application from its
    /// data, and what [`DialStringTarget::with_argv_separator`] refuses for the
    /// switch's reasons, since the hunt's split is the same one: space, controls,
    /// non-ASCII, `\`, `'` and lowercase `n r t s`. Parse refuses the same.
    pub fn inline_with_delimiter(
        endpoint: Endpoint,
        apps: impl IntoIterator<Item = Application>,
        delimiter: char,
    ) -> Result<Self, OriginateError> {
        let apps: Vec<Application> = apps
            .into_iter()
            .collect();
        if apps.is_empty() {
            return Err(OriginateError::EmptyInlineApplications);
        }
        check_inline_delimiter(delimiter)?;
        Ok(Self {
            endpoint,
            target: OriginateTarget::InlineApplications(apps),
            dialplan: None,
            context: None,
            cid_name: None,
            cid_num: None,
            timeout: None,
            // The comma needs no prefix, so asking for it is asking for the default.
            inline_delimiter: (delimiter != ',').then_some(delimiter),
            argv_separator: None,
        })
    }

    /// The separator this command renders its inline action list with, or
    /// `None` for the conventional comma.
    pub fn inline_delimiter(&self) -> Option<char> {
        self.inline_delimiter
    }

    /// Open the line with `^^<sep>`, so `originate` splits its arguments on `sep` rather
    /// than on spaces.
    ///
    /// Each argument is escaped once for that split, so a caller id, an application argument
    /// or a variable value carries spaces and quotes without quoting. An absent slot a later
    /// one forces reads `undef`; an empty value stays an empty argument, and the switch reads
    /// an empty context as the leg's own rather than `default`.
    ///
    /// Returns `Err` for a separator [`DialStringTarget::with_argv_separator`] refuses.
    ///
    /// ```
    /// # use freeswitch_types::commands::*;
    /// let cmd = Originate::application(
    ///     Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("default")),
    ///     Application::new("socket", Some("127.0.0.1:8040 async full")),
    /// )
    /// .cid_name("Front Desk")
    /// .with_argv_separator('~')?;
    ///
    /// assert_eq!(
    ///     cmd.to_string(),
    ///     "originate ^^~loopback/9199/default~&socket(127.0.0.1:8040 async full)~undef~undef~Front Desk"
    /// );
    /// # Ok::<(), OriginateError>(())
    /// ```
    pub fn with_argv_separator(mut self, sep: char) -> Result<Self, OriginateError> {
        Self::argv_target(BlockParse::default(), sep)?;
        self.argv_separator = Some(sep);
        Ok(self)
    }

    /// The separator `originate`'s arguments are split on, or `None` for spaces.
    pub fn argv_separator(&self) -> Option<char> {
        self.argv_separator
    }

    /// Set the dialplan type.
    ///
    /// Returns `Err` if setting `Inline` on an `Extension` target, or anything but `Inline` on
    /// inline applications, which `originate` would transfer as an extension.
    pub fn dialplan(self, dp: DialplanType) -> Result<Self, OriginateError> {
        self.with_dialplan(Dialplan::Typed(dp))
    }

    /// Name the dialplan module, for one [`DialplanType`] does not cover. The switch looks the
    /// name up when the channel is transferred, so a name no module registers hangs it up.
    ///
    /// A name reading as a [`DialplanType`] in any case sets that type. Refused as
    /// [`dialplan`](Self::dialplan) refuses: `inline` on an `Extension` target, any other name
    /// on inline applications.
    pub fn dialplan_raw(self, name: impl Into<String>) -> Result<Self, OriginateError> {
        self.with_dialplan(Dialplan::from_name(name.into()))
    }

    fn with_dialplan(mut self, dialplan: Dialplan) -> Result<Self, OriginateError> {
        check_dialplan_fits(&self.target, Some(&dialplan))?;
        self.dialplan = Some(dialplan);
        Ok(self)
    }

    /// Set the dialplan context. `undef` in any case reads as absent on the switch.
    pub fn context(mut self, ctx: impl Into<String>) -> Self {
        self.context = Some(ctx.into());
        self
    }

    /// Set the caller ID name. `undef` in any case reads as absent on the switch.
    pub fn cid_name(mut self, name: impl Into<String>) -> Self {
        self.cid_name = Some(name.into());
        self
    }

    /// Set the caller ID number. `undef` in any case reads as absent on the switch.
    pub fn cid_num(mut self, num: impl Into<String>) -> Self {
        self.cid_num = Some(num.into());
        self
    }

    /// Set the originate timeout. Sub-second precision is truncated to whole
    /// seconds on the wire and in serde round-trips.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// The dial endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Mutable reference to the dial endpoint.
    pub fn endpoint_mut(&mut self) -> &mut Endpoint {
        &mut self.endpoint
    }

    /// The originate target (extension, application, or inline apps).
    pub fn target(&self) -> &OriginateTarget {
        &self.target
    }

    /// Mutable reference to the originate target.
    pub fn target_mut(&mut self) -> &mut OriginateTarget {
        &mut self.target
    }

    /// The dialplan type, if explicitly set. `None` as well for a dialplan set by name; see
    /// [`dialplan_name`](Self::dialplan_name).
    pub fn dialplan_type(&self) -> Option<&DialplanType> {
        self.dialplan
            .as_ref()
            .and_then(Dialplan::typed)
    }

    /// The dialplan as the switch receives it, typed or named, if set.
    pub fn dialplan_name(&self) -> Option<&str> {
        self.dialplan
            .as_ref()
            .map(Dialplan::name)
    }

    /// The dialplan context, if set.
    pub fn context_str(&self) -> Option<&str> {
        self.context
            .as_deref()
    }

    /// The caller ID name, if set.
    pub fn caller_id_name(&self) -> Option<&str> {
        self.cid_name
            .as_deref()
    }

    /// The caller ID number, if set.
    pub fn caller_id_number(&self) -> Option<&str> {
        self.cid_num
            .as_deref()
    }

    /// The timeout as a `Duration`, if set.
    pub fn timeout_duration(&self) -> Option<Duration> {
        self.timeout
    }

    /// The timeout in whole seconds, if set.
    pub fn timeout_seconds(&self) -> Option<u64> {
        self.timeout
            .map(|d| d.as_secs())
    }

    /// Override the dialplan type after construction.
    pub fn set_dialplan(&mut self, dp: Option<DialplanType>) {
        self.dialplan = dp.map(Dialplan::Typed);
    }

    /// Override the dialplan context after construction. `undef` in any case reads as absent
    /// on the switch.
    pub fn set_context(&mut self, ctx: Option<impl Into<String>>) {
        self.context = ctx.map(|c| c.into());
    }

    /// Override the caller ID name after construction. `undef` in any case reads as absent
    /// on the switch.
    pub fn set_cid_name(&mut self, name: Option<impl Into<String>>) {
        self.cid_name = name.map(|n| n.into());
    }

    /// Override the caller ID number after construction. `undef` in any case reads as absent
    /// on the switch.
    pub fn set_cid_num(&mut self, num: Option<impl Into<String>>) {
        self.cid_num = num.map(|n| n.into());
    }

    /// Override the timeout after construction.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }

    /// Dialplan, context, cid_name, cid_num and timeout through the last one set. FreeSWITCH
    /// reads them by position, so an absent slot before that is `None` rather than skipped.
    fn positional_tail(&self) -> Vec<Option<String>> {
        let dialplan = match (&self.target, &self.dialplan) {
            (OriginateTarget::InlineApplications(_), None) => {
                Some(DialplanType::Inline.wire_name())
            }
            (_, dialplan) => dialplan
                .as_ref()
                .map(Dialplan::name),
        };
        let slots = [
            dialplan.map(str::to_string),
            self.context
                .clone(),
            self.cid_name
                .clone(),
            self.cid_num
                .clone(),
            self.timeout
                .map(|t| {
                    t.as_secs()
                        .to_string()
                }),
        ];
        let present = slots
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |last| last + 1);
        slots
            .into_iter()
            .take(present)
            .collect()
    }

    /// The target and positionals after the endpoint, an absent slot a later one forces written
    /// `undef`. On blanks each goes through [`originate_quote`]; on a separator each is escaped
    /// once.
    fn write_arguments(&self, f: &mut fmt::Formatter<'_>, target: &str) -> fmt::Result {
        let tail = self.positional_tail();
        let arguments = std::iter::once(Some(target)).chain(
            tail.iter()
                .map(Option::as_deref),
        );
        for (at, argument) in arguments.enumerate() {
            match (self.argv_separator, argument.unwrap_or(UNDEF)) {
                (None, text) => write!(f, " {}", originate_quote(text))?,
                // A trailing separator adds no argument; quotes keep a final empty one.
                (Some(sep), "") if at == tail.len() => write!(f, "{sep}''")?,
                (Some(sep), text) => {
                    f.write_char(sep)?;
                    write_escaped(&mut *f, sep, text)?;
                }
            }
        }
        Ok(())
    }
}

/// Renders an [`Originate`] for one parser revision. Returned by
/// [`Originate::display_with`].
#[derive(Debug, Clone, Copy)]
pub struct OriginateDisplay<'a> {
    originate: &'a Originate,
    block_parse: BlockParse,
}

impl fmt::Display for OriginateDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.originate
            .write_with(f, self.block_parse)
    }
}

impl fmt::Display for Originate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_with(f, BlockParse::default())
    }
}

impl FromStr for Originate {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_with(s, BlockParse::default())
    }
}

impl Originate {
    /// Render for a switch running `block_parse`, rather than the default
    /// revision [`Display`](fmt::Display) uses.
    pub fn display_with(&self, block_parse: BlockParse) -> OriginateDisplay<'_> {
        OriginateDisplay {
            originate: self,
            block_parse,
        }
    }

    /// The target argument before the split's quoting or escaping.
    fn target_text(&self) -> String {
        match &self.target {
            OriginateTarget::Extension(ext) => ext.clone(),
            OriginateTarget::Application(app) => app.to_string_with_dialplan(&DialplanType::Xml),
            OriginateTarget::InlineApplications(apps) => match self.inline_delimiter {
                Some(delimiter) => format!("m:{delimiter}:{}", render_inline(apps, delimiter)),
                None => render_inline(apps, DEFAULT_INLINE_DELIMITER),
            },
        }
    }

    fn write_with(&self, f: &mut fmt::Formatter<'_>, block_parse: BlockParse) -> fmt::Result {
        let dial_target = match self.argv_separator {
            // The field holds only separators `with_argv_separator` accepted.
            Some(sep) => {
                f.write_str("originate ^^")?;
                f.write_char(sep)?;
                Self::argv_target(block_parse, sep).map_err(|_| fmt::Error)?
            }
            None => {
                f.write_str("originate ")?;
                Self::dial_target(block_parse)
            }
        };
        write!(
            f,
            "{}",
            self.endpoint
                .display_for(dial_target)
        )?;
        self.write_arguments(f, &self.target_text())
    }

    fn dial_target(block_parse: BlockParse) -> DialStringTarget {
        DialStringTarget::new(DialStringCarrier::EslApi).with_block_parse(block_parse)
    }

    fn argv_target(block_parse: BlockParse, sep: char) -> Result<DialStringTarget, OriginateError> {
        Self::dial_target(block_parse)
            .with_argv_separator(sep)
            .map_err(OriginateError::InvalidArgvSeparator)
    }

    /// Parse an originate written for a switch running `block_parse`, mirroring
    /// [`display_with`](Self::display_with).
    pub fn parse_with(s: &str, block_parse: BlockParse) -> Result<Self, OriginateError> {
        let s = s
            .strip_prefix("originate")
            .unwrap_or(s)
            .trim_matches(STRIPPED_WHITESPACE);
        // `^^ ` names the blank split itself.
        let sep = delimiter_override(&trace(s))
            .0
            .filter(|&sep| sep != ' ');
        let dial_target = match sep {
            Some(sep) => Self::argv_target(block_parse, sep)?,
            None => Self::dial_target(block_parse),
        };
        let mut args = originate_split(s, ' ')?.into_iter();

        let endpoint_str = args
            .next()
            .ok_or_else(|| OriginateError::ParseError("empty originate".into()))?;
        let endpoint = Endpoint::parse_for(&endpoint_str, dial_target)?;

        let Slots {
            target: target_str,
            dialplan,
            context,
            cid_name,
            cid_num,
            timeout,
        } = Slots::read(args.map(|token| clean_argument(&token, sep)))?;

        let target = super::parse_originate_target(
            &target_str,
            dialplan
                .as_ref()
                .and_then(Dialplan::typed),
        )?;

        let timeout = match timeout {
            None => None,
            Some(value) => match value.parse::<u64>() {
                Ok(secs) => Some(Duration::from_secs(secs)),
                Err(source) => return Err(OriginateError::InvalidTimeout { value, source }),
            },
        };

        // Validate via constructors then set parsed fields directly (same module)
        let mut orig = match target {
            OriginateTarget::Extension(ref ext) => Self::extension(endpoint, ext.clone()),
            OriginateTarget::Application(ref app) => Self::application(endpoint, app.clone()),
            OriginateTarget::InlineApplications(ref apps) => Self::inline(endpoint, apps.clone())?,
        };
        // Keep what was on the wire rather than what the constructor would have
        // chosen, so a parsed command renders back byte for byte.
        if matches!(orig.target, OriginateTarget::InlineApplications(_)) {
            orig.inline_delimiter = super::split_inline_prefix(&target_str).0;
        }
        orig.dialplan = dialplan;
        orig.context = context;
        orig.cid_name = cid_name;
        orig.cid_num = cid_num;
        orig.timeout = timeout;
        orig.argv_separator = sep;
        Ok(orig)
    }
}

/// An originate's arguments after the endpoint, as the switch reads them.
struct Slots {
    target: String,
    dialplan: Option<Dialplan>,
    context: Option<String>,
    cid_name: Option<String>,
    cid_num: Option<String>,
    timeout: Option<String>,
}

impl Slots {
    /// The tokens after the endpoint, each already through its split's cleanup, read strictly
    /// by position as `originate_function` does.
    fn read(args: impl Iterator<Item = String>) -> Result<Self, OriginateError> {
        let mut args = args.map(undef_to_none);
        let target = match args.next() {
            None => {
                return Err(OriginateError::ParseError(
                    "missing target in originate".into(),
                ))
            }
            Some(None) => return Err(OriginateError::UndefPositional("target")),
            Some(Some(target)) => target,
        };
        let dialplan = args
            .next()
            .flatten()
            .map(Dialplan::from_name);
        let mut next = || {
            args.next()
                .flatten()
        };
        let (context, cid_name, cid_num, timeout) = (next(), next(), next(), next());
        if args
            .next()
            .is_some()
        {
            return Err(OriginateError::ParseError(
                "originate takes at most seven arguments".into(),
            ));
        }
        Ok(Self {
            target,
            dialplan,
            context,
            cid_name,
            cid_num,
            timeout,
        })
    }
}

/// FreeSWITCH's `undef` placeholder, in any case, read as the absent value it stands for.
fn undef_to_none(value: String) -> Option<String> {
    (!value.eq_ignore_ascii_case(UNDEF)).then_some(value)
}

/// Errors from originate command parsing or construction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OriginateError {
    /// A single-quoted token was never closed.
    UnclosedQuote(String),
    /// General parse failure with a description.
    ParseError(String),
    /// Inline originate requires at least one application.
    EmptyInlineApplications,
    /// Extension target cannot use inline dialplan.
    ExtensionWithInlineDialplan,
    /// A dial string carried a variable block for an endpoint type that has
    /// nowhere to keep it, such as `error/`. Carries that type's name.
    VariablesNotSupported(&'static str),
    /// The timeout argument is not a whole number of seconds.
    InvalidTimeout {
        /// The rejected token.
        value: String,
        /// Why it is not a number.
        source: ParseIntError,
    },
    /// An `error/` endpoint named a cause this crate does not know.
    UnknownHangupCause {
        /// The rejected token.
        value: String,
        /// The hangup-cause parse failure.
        source: ParseHangupCauseError,
    },
    /// A `group_call` expression carried an unknown order suffix.
    UnknownGroupCallOrder {
        /// The rejected token.
        value: String,
        /// The order parse failure.
        source: ParseGroupCallOrderError,
    },
    /// A dial string whose leading path segment names no endpoint type.
    UnknownEndpointType(String),
    /// An inline separator the hunt's split breaks on, or `:`. Carries it.
    InvalidInlineDelimiter(char),
    /// A `^^` argument separator [`DialStringTarget::with_argv_separator`] refuses.
    InvalidArgvSeparator(InvalidArgvSeparator),
    /// A positional argument reads `undef`, which the switch takes as absent. Names the field.
    UndefPositional(&'static str),
    /// An extension opens with `&` and more, which `originate` runs as an application.
    ExtensionReadsAsApplication,
    /// An `&name(args)` application whose name carries a parenthesis or whose arguments carry
    /// `)`, where `originate` ends the arguments. [`Originate::inline`] delivers such arguments.
    ParenthesisInApplication {
        /// The application name.
        application: String,
    },
    /// Inline applications under a dialplan other than `inline`, which transfers them as an
    /// extension.
    InlineApplicationsWithDialplan,
    /// An endpoint field the switch reads as something else, whatever the escaping.
    UndeliverableEndpointField {
        /// The endpoint type.
        endpoint: &'static str,
        /// The field.
        field: &'static str,
        /// What the switch does with it.
        fault: EndpointFieldFault,
    },
    /// A `[` in one leg's endpoint text closes in a later leg of the same group, so the switch
    /// reads every leg between as one. Names the group.
    BracketSpansLegs {
        /// Index of the group.
        group: usize,
    },
}

impl std::fmt::Display for OriginateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnclosedQuote(s) => {
                write!(f, "unclosed quote in the final token ({} bytes)", s.len())
            }
            Self::ParseError(s) => write!(f, "parse error: {s}"),
            Self::EmptyInlineApplications => {
                f.write_str("inline originate requires at least one application")
            }
            Self::ExtensionWithInlineDialplan => {
                f.write_str("extension target is incompatible with inline dialplan")
            }
            Self::VariablesNotSupported(kind) => {
                write!(f, "a {kind} endpoint carries no variable block")
            }
            Self::InvalidTimeout { value, .. } => write!(
                f,
                "timeout is not a whole number of seconds ({} bytes)",
                value.len()
            ),
            Self::UnknownHangupCause { value, .. } => write!(
                f,
                "unknown hangup cause in an error endpoint ({} bytes)",
                value.len()
            ),
            Self::UnknownGroupCallOrder { value, .. } => {
                write!(f, "unknown group_call order suffix ({} bytes)", value.len())
            }
            Self::InvalidInlineDelimiter(_) => {
                f.write_str("the named separator cannot separate an inline action list")
            }
            Self::UnknownEndpointType(s) => {
                write!(f, "unknown endpoint type ({} bytes)", s.len())
            }
            Self::InvalidArgvSeparator(_) => f.write_str("unusable originate argument separator"),
            Self::UndefPositional(field) => write!(
                f,
                "{field} reads as the undef placeholder, which the switch takes as absent"
            ),
            Self::ExtensionReadsAsApplication => f.write_str(
                "the extension opens with & and more, which originate runs as an application",
            ),
            Self::ParenthesisInApplication { .. } => f.write_str(
                "an application's name carries a parenthesis or its arguments carry ), \
                 where originate ends the arguments; Originate::inline delivers such \
                 arguments in an inline action list",
            ),
            Self::InlineApplicationsWithDialplan => f.write_str(
                "inline applications run only under the inline dialplan; \
                 any other transfers them as an extension",
            ),
            Self::UndeliverableEndpointField {
                endpoint,
                field,
                fault,
            } => write!(f, "the {field} of a {endpoint} endpoint {fault}"),
            Self::BracketSpansLegs { group } => write!(
                f,
                "a bracket in group {group} closes in a later leg, which the switch reads as one leg"
            ),
        }
    }
}

impl std::error::Error for OriginateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidTimeout { source, .. } => Some(source),
            Self::UnknownHangupCause { source, .. } => Some(source),
            Self::UnknownGroupCallOrder { source, .. } => Some(source),
            Self::InvalidArgvSeparator(source) => Some(source),
            Self::UnclosedQuote(_)
            | Self::ParseError(_)
            | Self::EmptyInlineApplications
            | Self::ExtensionWithInlineDialplan
            | Self::VariablesNotSupported(_)
            | Self::UnknownEndpointType(_)
            | Self::InvalidInlineDelimiter(_)
            | Self::UndefPositional(_)
            | Self::ExtensionReadsAsApplication
            | Self::ParenthesisInApplication { .. }
            | Self::InlineApplicationsWithDialplan
            | Self::UndeliverableEndpointField { .. }
            | Self::BracketSpansLegs { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::endpoint::{LoopbackEndpoint, SofiaEndpoint, SofiaGateway};

    /// `switch_strip_whitespace` strips tab, newline, vertical tab, CR and space only.
    #[test]
    fn parse_strips_only_the_switch_whitespace() {
        let extension = |line: &str| match Originate::from_str(line)
            .unwrap()
            .target()
        {
            OriginateTarget::Extension(ext) => ext.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            extension("originate \t\x0bloopback/9199/test 9199 \x0b\r\n"),
            "9199"
        );
        assert_eq!(
            extension("originate loopback/9199/test 9199\x0c"),
            "9199\x0c"
        );
        assert_eq!(
            extension("originate loopback/9199/test 9199\u{a0}"),
            "9199\u{a0}"
        );
    }

    // --- Endpoint ---

    #[test]
    fn endpoint_uri_only() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        assert_eq!(ep.to_string(), "sofia/internal/123@example.com");
    }

    #[test]
    fn endpoint_uri_with_variable() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "1");
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: Some(vars),
        });
        assert_eq!(
            ep.to_string(),
            "{one_variable=1}sofia/internal/123@example.com"
        );
    }

    #[test]
    fn endpoint_variable_with_quote() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "one'quote");
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: Some(vars),
        });
        // An endpoint renders for the ESL API carrier, which is the deeper of
        // the two escapings.
        assert_eq!(
            ep.to_string(),
            r"{one_variable=one\\\\\\\'quote}sofia/internal/123@example.com"
        );
    }

    /// `Display` and `FromStr` are the default revision, not a second render path.
    #[test]
    fn display_and_from_str_are_the_default_revision() {
        use crate::commands::variables::BlockParse;

        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("cid", "it's");
        let orig = Originate::application(
            Endpoint::SofiaGateway(SofiaGateway::new("gw", "1234").with_variables(vars)),
            Application::simple("park"),
        );

        let rendered = orig
            .display_with(BlockParse::PairSplitCleans)
            .to_string();
        assert_eq!(rendered, orig.to_string());
        assert_eq!(
            Originate::parse_with(&rendered, BlockParse::PairSplitCleans)
                .unwrap_or_else(|e| panic!("{rendered} failed to parse: {e}")),
            orig
        );
    }

    #[test]
    fn loopback_endpoint_display() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "1");
        let ep = Endpoint::Loopback(
            LoopbackEndpoint::new("aUri")
                .with_context("aContext")
                .with_variables(vars),
        );
        assert_eq!(ep.to_string(), "{one_variable=1}loopback/aUri/aContext");
    }

    #[test]
    fn sofia_gateway_endpoint_display() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("one_variable", "1");
        let ep = Endpoint::SofiaGateway(SofiaGateway {
            destination: "aUri".into(),
            profile: None,
            gateway: "internal".into(),
            variables: Some(vars),
        });
        assert_eq!(
            ep.to_string(),
            "{one_variable=1}sofia/gateway/internal/aUri"
        );
    }

    // --- Application ---

    #[test]
    fn application_xml_format() {
        let app = Application::new("testApp", Some("testArg"));
        assert_eq!(
            app.to_string_with_dialplan(&DialplanType::Xml),
            "&testApp(testArg)"
        );
    }

    #[test]
    fn application_inline_format() {
        let app = Application::new("testApp", Some("testArg"));
        assert_eq!(
            app.to_string_with_dialplan(&DialplanType::Inline),
            "testApp:testArg"
        );
    }

    #[test]
    fn application_inline_no_args() {
        let app = Application::simple("park");
        assert_eq!(app.to_string_with_dialplan(&DialplanType::Inline), "park");
    }

    // --- Originate ---

    #[test]
    fn originate_xml_display() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::application(ep, Application::new("conference", Some("1")))
            .dialplan(DialplanType::Xml)
            .unwrap();
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com &conference(1) XML"
        );
    }

    #[test]
    fn originate_inline_display() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::inline(ep, vec![Application::new("conference", Some("1"))])
            .unwrap()
            .dialplan(DialplanType::Inline)
            .unwrap();
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com conference:1 inline"
        );
    }

    #[test]
    fn originate_extension_display() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::extension(ep, "1000")
            .dialplan(DialplanType::Xml)
            .unwrap()
            .context("default");
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com 1000 XML default"
        );
    }

    #[test]
    fn originate_extension_round_trip() {
        let input = "originate sofia/internal/test@example.com 1000 XML default";
        let parsed: Originate = input
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), input);
        assert!(matches!(parsed.target(), OriginateTarget::Extension(ref e) if e == "1000"));
    }

    #[test]
    fn originate_extension_no_dialplan() {
        let input = "originate sofia/internal/test@example.com 1000";
        let parsed: Originate = input
            .parse()
            .unwrap();
        assert!(matches!(parsed.target(), OriginateTarget::Extension(ref e) if e == "1000"));
        assert_eq!(parsed.to_string(), input);
    }

    #[test]
    fn originate_extension_with_inline_errors() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let result = Originate::extension(ep, "1000").dialplan(DialplanType::Inline);
        assert!(result.is_err());
    }

    #[test]
    fn originate_empty_inline_errors() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let result = Originate::inline(ep, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn originate_from_string_round_trip() {
        let input = "originate {test='variable with quote'}sofia/internal/test@example.com 123";
        let orig: Originate = input
            .parse()
            .unwrap();
        assert!(matches!(orig.target(), OriginateTarget::Extension(ref e) if e == "123"));
        assert_eq!(orig.to_string(), input);
    }

    #[test]
    fn originate_socket_app_quoted() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
        let orig = Originate::application(
            ep,
            Application::new("socket", Some("127.0.0.1:8040 async full")),
        );
        assert_eq!(
            orig.to_string(),
            "originate loopback/9199/test '&socket(127.0.0.1:8040 async full)'"
        );
    }

    #[test]
    fn originate_socket_round_trip() {
        let input = "originate loopback/9199/test '&socket(127.0.0.1:8040 async full)'";
        let parsed: Originate = input
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), input);
        if let OriginateTarget::Application(ref app) = parsed.target() {
            assert_eq!(app.args(), Some("127.0.0.1:8040 async full"));
        } else {
            panic!("expected Application target");
        }
    }

    #[test]
    fn originate_display_round_trip() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::application(ep, Application::new("conference", Some("1")))
            .dialplan(DialplanType::Xml)
            .unwrap();
        let s = orig.to_string();
        let parsed: Originate = s
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn originate_inline_no_args_round_trip() {
        let input = "originate sofia/internal/123@example.com park inline";
        let parsed: Originate = input
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), input);
        if let OriginateTarget::InlineApplications(ref apps) = parsed.target() {
            assert!(apps[0]
                .args()
                .is_none());
        } else {
            panic!("expected InlineApplications target");
        }
    }

    #[test]
    fn originate_inline_multi_app_round_trip() {
        let input =
            "originate sofia/internal/123@example.com playback:/tmp/test.wav,hangup:NORMAL_CLEARING inline";
        let parsed: Originate = input
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), input);
    }

    #[test]
    fn originate_inline_auto_dialplan() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::inline(ep, vec![Application::simple("park")]).unwrap();
        assert!(orig
            .to_string()
            .contains("inline"));
    }

    // --- DialplanType ---

    #[test]
    fn dialplan_type_display() {
        assert_eq!(DialplanType::Inline.to_string(), "inline");
        assert_eq!(DialplanType::Xml.to_string(), "XML");
    }

    #[test]
    fn dialplan_type_from_str() {
        assert_eq!(
            "inline"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Inline
        );
        assert_eq!(
            "XML"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Xml
        );
    }

    #[test]
    fn dialplan_type_from_str_case_insensitive() {
        assert_eq!(
            "xml"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Xml
        );
        assert_eq!(
            "Xml"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Xml
        );
        assert_eq!(
            "INLINE"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Inline
        );
        assert_eq!(
            "Inline"
                .parse::<DialplanType>()
                .unwrap(),
            DialplanType::Inline
        );
    }

    // --- Serde ---

    #[test]
    fn serde_dialplan_type_xml() {
        let json = serde_json::to_string(&DialplanType::Xml).unwrap();
        assert_eq!(json, "\"xml\"");
        let parsed: DialplanType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DialplanType::Xml);
    }

    #[test]
    fn serde_dialplan_type_inline() {
        let json = serde_json::to_string(&DialplanType::Inline).unwrap();
        assert_eq!(json, "\"inline\"");
        let parsed: DialplanType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DialplanType::Inline);
    }

    #[test]
    fn serde_application() {
        let app = Application::new("park", None::<&str>);
        let json = serde_json::to_string(&app).unwrap();
        let parsed: Application = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, app);
    }

    #[test]
    fn serde_application_with_args() {
        let app = Application::new("conference", Some("1"));
        let json = serde_json::to_string(&app).unwrap();
        let parsed: Application = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, app);
    }

    #[test]
    fn serde_application_skips_none_args() {
        let app = Application::new("park", None::<&str>);
        let json = serde_json::to_string(&app).unwrap();
        assert!(!json.contains("args"));
    }

    #[test]
    fn serde_originate_application_round_trip() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::application(ep, Application::new("park", None::<&str>))
            .dialplan(DialplanType::Xml)
            .unwrap()
            .context("default")
            .cid_name("Test")
            .cid_num("5551234")
            .timeout(Duration::from_secs(30));
        let json = serde_json::to_string(&orig).unwrap();
        assert!(json.contains("\"application\""));
        let parsed: Originate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn serde_originate_extension() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "extension": "1000",
            "dialplan": "xml",
            "context": "default"
        }"#;
        let orig: Originate = serde_json::from_str(json).unwrap();
        assert!(matches!(orig.target(), OriginateTarget::Extension(ref e) if e == "1000"));
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com 1000 XML default"
        );
    }

    #[test]
    fn serde_originate_extension_with_inline_rejected() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "extension": "1000",
            "dialplan": "inline"
        }"#;
        let result = serde_json::from_str::<Originate>(json);
        assert!(result.is_err());
    }

    #[test]
    fn serde_originate_empty_inline_rejected() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "inline_applications": []
        }"#;
        let result = serde_json::from_str::<Originate>(json);
        assert!(result.is_err());
    }

    #[test]
    fn serde_originate_inline_applications() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "inline_applications": [
                {"name": "playback", "args": "/tmp/test.wav"},
                {"name": "hangup", "args": "NORMAL_CLEARING"}
            ]
        }"#;
        let orig: Originate = serde_json::from_str(json).unwrap();
        if let OriginateTarget::InlineApplications(ref apps) = orig.target() {
            assert_eq!(apps.len(), 2);
        } else {
            panic!("expected InlineApplications");
        }
        assert!(orig
            .to_string()
            .contains("inline"));
    }

    /// A config file states applications as structured data, so it carries no
    /// separator to get wrong — and must not have to grow one.
    #[test]
    fn serde_inline_gets_a_separator_without_naming_one() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "inline_applications": [
                {"name": "playback", "args": "tone_stream://%(500,0,800)"},
                {"name": "park"}
            ]
        }"#;
        let orig: Originate = serde_json::from_str(json).unwrap();

        assert_eq!(orig.inline_delimiter(), None);
        assert!(orig
            .to_string()
            .contains(r"playback:tone_stream://%(500\\,0\\,800),park"));

        // Serializing back must not introduce a separator field the source
        // never had.
        let round_tripped = serde_json::to_string(&orig).unwrap();
        assert!(!round_tripped.contains("delimiter"));
        let reparsed: Originate = serde_json::from_str(&round_tripped).unwrap();
        assert_eq!(reparsed.to_string(), orig.to_string());
    }

    #[test]
    fn serde_originate_skips_none_fields() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::application(ep, Application::new("park", None::<&str>));
        let json = serde_json::to_string(&orig).unwrap();
        assert!(!json.contains("dialplan"));
        assert!(!json.contains("context"));
        assert!(!json.contains("cid_name"));
        assert!(!json.contains("cid_num"));
        assert!(!json.contains("timeout"));
    }

    #[test]
    fn serde_originate_to_wire_format() {
        let json = r#"{
            "endpoint": {"sofia": {"profile": "internal", "destination": "123@example.com"}},
            "application": {"name": "park"},
            "dialplan": "xml",
            "context": "default"
        }"#;
        let orig: Originate = serde_json::from_str(json).unwrap();
        let wire = orig.to_string();
        assert!(wire.starts_with("originate"));
        assert!(wire.contains("sofia/internal/123@example.com"));
        assert!(wire.contains("&park()"));
        assert!(wire.contains("XML"));
    }

    // --- Application::simple ---

    #[test]
    fn application_simple_no_args() {
        let app = Application::simple("park");
        assert_eq!(app.name(), "park");
        assert!(app
            .args()
            .is_none());
    }

    #[test]
    fn application_simple_xml_format() {
        let app = Application::simple("park");
        assert_eq!(app.to_string_with_dialplan(&DialplanType::Xml), "&park()");
    }

    // --- OriginateTarget From impls ---

    #[test]
    fn originate_target_from_application() {
        let target: OriginateTarget = Application::simple("park").into();
        assert!(matches!(target, OriginateTarget::Application(_)));
    }

    #[test]
    fn originate_target_from_vec() {
        let target: OriginateTarget = vec![
            Application::new("conference", Some("1")),
            Application::new("hangup", Some("NORMAL_CLEARING")),
        ]
        .into();
        if let OriginateTarget::InlineApplications(apps) = target {
            assert_eq!(apps.len(), 2);
        } else {
            panic!("expected InlineApplications");
        }
    }

    #[test]
    fn originate_target_application_wire_format() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::application(ep, Application::simple("park"));
        assert_eq!(
            orig.to_string(),
            "originate sofia/internal/123@example.com &park()"
        );
    }

    #[test]
    fn originate_timeout_only_fills_positional_gaps() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
        let cmd = Originate::application(ep, Application::simple("park"))
            .timeout(Duration::from_secs(30));
        assert_eq!(
            cmd.to_string(),
            "originate loopback/9199/test &park() undef undef undef undef 30"
        );
    }

    #[test]
    fn originate_cid_num_only_fills_preceding_gaps() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
        let cmd = Originate::application(ep, Application::simple("park")).cid_num("5551234");
        assert_eq!(
            cmd.to_string(),
            "originate loopback/9199/test &park() undef undef undef 5551234"
        );
    }

    #[test]
    fn originate_context_only_fills_dialplan() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
        let cmd = Originate::extension(ep, "1000").context("myctx");
        assert_eq!(
            cmd.to_string(),
            "originate loopback/9199/test 1000 undef myctx"
        );
    }

    /// A slot forced present by a later one is `undef`, which reads back as absent.
    #[test]
    fn originate_gap_filler_round_trips() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
        let cmd = Originate::application(ep, Application::simple("park")).cid_name("Alice");
        let wire = cmd.to_string();
        let parsed: Originate = wire
            .parse()
            .unwrap();
        assert_eq!(parsed, cmd);
        assert_eq!(parsed.to_string(), wire);
    }

    #[test]
    fn serde_originate_full_round_trip_with_variables() {
        let mut ep_vars = Variables::new(VariablesType::Default);
        ep_vars.insert("originate_timeout", "30");
        ep_vars.insert("sip_h_X-Custom", "value with spaces");
        let ep = Endpoint::SofiaGateway(SofiaGateway {
            gateway: "my_provider".into(),
            destination: "18005551234".into(),
            profile: Some("external".into()),
            variables: Some(ep_vars),
        });
        let orig = Originate::application(ep, Application::new("park", None::<&str>))
            .dialplan(DialplanType::Xml)
            .unwrap()
            .context("public")
            .cid_name("Test Caller")
            .cid_num("5551234")
            .timeout(Duration::from_secs(60));
        let json = serde_json::to_string(&orig).unwrap();
        let parsed: Originate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, orig);
        assert_eq!(parsed.to_string(), orig.to_string());
    }

    #[test]
    fn serde_originate_inline_round_trip_with_all_fields() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("default"));
        let orig = Originate::inline(
            ep,
            vec![
                Application::new("playback", Some("/tmp/test.wav")),
                Application::new("hangup", Some("NORMAL_CLEARING")),
            ],
        )
        .unwrap()
        .dialplan(DialplanType::Inline)
        .unwrap()
        .context("default")
        .cid_name("IVR")
        .cid_num("0000")
        .timeout(Duration::from_secs(45));
        let json = serde_json::to_string(&orig).unwrap();
        let parsed: Originate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, orig);
        assert_eq!(parsed.to_string(), orig.to_string());
    }

    #[test]
    fn originate_context_named_inline() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::extension(ep, "1000")
            .dialplan(DialplanType::Xml)
            .unwrap()
            .context("inline");
        let wire = orig.to_string();
        assert!(wire.contains("XML inline"), "wire: {}", wire);
        let parsed: Originate = wire
            .parse()
            .unwrap();
        // "inline" is consumed as the dialplan type, not the context
        // This is an accepted limitation of positional parsing
        assert_eq!(parsed.to_string(), wire);
    }

    #[test]
    fn originate_context_named_xml() {
        let ep = Endpoint::Sofia(SofiaEndpoint {
            profile: "internal".into(),
            destination: "123@example.com".into(),
            variables: None,
        });
        let orig = Originate::extension(ep, "1000")
            .dialplan(DialplanType::Xml)
            .unwrap()
            .context("XML");
        let wire = orig.to_string();
        // "XML XML" - first is dialplan, second is context
        assert!(wire.contains("XML XML"), "wire: {}", wire);
        let parsed: Originate = wire
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), wire);
    }

    /// A dial string carries caller-id and `sip_h_*` values, so a rejection
    /// names the field it failed on and leaves the bytes on the error.
    #[test]
    fn errors_name_the_field_and_never_quote_the_input() {
        let cases: [(&str, &str); 4] = [
            (
                "originate sofia/a/b 1000 XML default undef undef 30s",
                "30s",
            ),
            ("originate error/NO_SUCH_CAUSE 1000", "NO_SUCH_CAUSE"),
            ("originate ${group_call(support@example.com+Z)} 1000", "+Z"),
            ("originate verto/15551234567 1000", "15551234567"),
        ];
        for (input, secret) in cases {
            let msg = input
                .parse::<Originate>()
                .expect_err(input)
                .to_string();
            assert!(!msg.contains(secret), "{input} quoted its input: {msg}");
        }

        let msg = originate_split("originate 'never closed", ' ')
            .expect_err("unclosed quote")
            .to_string();
        assert!(!msg.contains("never closed"), "quoted its input: {msg}");
    }

    /// A rejected timeout, cause or order has a cause of its own; stringifying
    /// it into a message drops the chain a caller would match on.
    #[test]
    fn errors_keep_their_source() {
        use std::error::Error;

        for input in [
            "originate sofia/a/b 1000 XML default undef undef 30s",
            "originate error/NO_SUCH_CAUSE 1000",
            "originate ${group_call(support@example.com+Z)} 1000",
        ] {
            assert!(
                input
                    .parse::<Originate>()
                    .expect_err(input)
                    .source()
                    .is_some(),
                "{input} has no source"
            );
        }
    }

    /// A caller-id with a space is quoted on the way out, so the quotes are
    /// this crate's own framing and have to come back off on the way in. Left
    /// on, they are re-quoted at every hop and the switch dials the quotes.
    #[test]
    fn quoted_caller_id_round_trips_without_gaining_quotes() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
        let cmd = Originate::application(ep, Application::simple("park"))
            .cid_name("Outbound Call")
            .cid_num("555 1234");
        let wire = cmd.to_string();
        assert_eq!(
            wire,
            "originate loopback/9199/test &park() undef undef 'Outbound Call' '555 1234'"
        );

        let parsed: Originate = wire
            .parse()
            .unwrap();
        assert_eq!(parsed.caller_id_name(), Some("Outbound Call"));
        assert_eq!(parsed.caller_id_number(), Some("555 1234"));
        assert_eq!(parsed.to_string(), wire);
    }

    /// The mutators are the config-driven path: deserialize a template, then
    /// override per call. Clearing a field is half of that and had no coverage.
    #[test]
    fn originate_mutators_set_and_clear_every_optional_field() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("default"));
        let mut cmd = Originate::extension(ep, "1000");

        cmd.set_dialplan(Some(DialplanType::Xml));
        cmd.set_context(Some("public"));
        cmd.set_cid_name(Some("Alice"));
        cmd.set_cid_num(Some("5551234"));
        cmd.set_timeout(Some(Duration::from_secs(30)));
        assert_eq!(
            cmd.to_string(),
            "originate loopback/9199/default 1000 XML public Alice 5551234 30"
        );
        assert_eq!(cmd.timeout_duration(), Some(Duration::from_secs(30)));

        // The turbofish is not decoration: `Option<impl Into<String>>` leaves a
        // bare `None` with no type to infer, so clearing a field needs one.
        cmd.set_dialplan(None);
        cmd.set_context(None::<String>);
        cmd.set_cid_name(None::<String>);
        cmd.set_cid_num(None::<String>);
        cmd.set_timeout(None);
        assert_eq!(cmd.to_string(), "originate loopback/9199/default 1000");
    }

    #[test]
    fn originate_mut_accessors_reach_the_endpoint_and_the_target() {
        let ep = Endpoint::Sofia(SofiaEndpoint::new("internal", "1000@example.com"));
        let mut cmd = Originate::application(ep, Application::new("socket", Some("old")));

        *cmd.endpoint_mut() = Endpoint::Loopback(LoopbackEndpoint::new("9199"));
        if let OriginateTarget::Application(app) = cmd.target_mut() {
            *app.name_mut() = "playback".into();
            *app.args_mut() = Some("/tmp/test.wav".into());
        } else {
            panic!("expected Application target");
        }

        assert_eq!(
            cmd.to_string(),
            "originate loopback/9199 &playback(/tmp/test.wav)"
        );
    }

    #[test]
    fn originate_accessors() {
        let ep = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("default"));
        let cmd = Originate::extension(ep, "1000")
            .dialplan(DialplanType::Xml)
            .unwrap()
            .context("default")
            .cid_name("Alice")
            .cid_num("5551234")
            .timeout(Duration::from_secs(30));

        assert!(matches!(cmd.target(), OriginateTarget::Extension(ref e) if e == "1000"));
        assert_eq!(cmd.dialplan_type(), Some(&DialplanType::Xml));
        assert_eq!(cmd.context_str(), Some("default"));
        assert_eq!(cmd.caller_id_name(), Some("Alice"));
        assert_eq!(cmd.caller_id_number(), Some("5551234"));
        assert_eq!(cmd.timeout_seconds(), Some(30));
    }

    fn null_endpoint() -> Endpoint {
        Endpoint::Loopback(LoopbackEndpoint::new("9199"))
    }

    #[test]
    fn inline_without_commas_keeps_the_default_separator() {
        let cmd = Originate::inline(
            null_endpoint(),
            [
                Application::simple("answer"),
                Application::new("playback", Some("silence_stream://300")),
            ],
        )
        .unwrap();

        assert_eq!(
            cmd.to_string(),
            "originate loopback/9199 answer,playback:silence_stream://300 inline"
        );
        assert_eq!(cmd.inline_delimiter(), None);
    }

    /// A bare comma inside an argument is read as an action separator by
    /// `inline_dialplan_hunt`, producing applications the caller never wrote
    /// and no error anywhere. Escaping it is what the switch's own
    /// `cleanup_separated_string` undoes on the far side.
    #[test]
    fn inline_escapes_a_separator_inside_an_argument() {
        let cmd = Originate::inline(
            null_endpoint(),
            [
                Application::new("playback", Some("tone_stream://%(500,0,800)")),
                Application::simple("park"),
            ],
        )
        .unwrap();

        assert_eq!(
            cmd.to_string(),
            r"originate loopback/9199 'playback:tone_stream://%(500\\,0\\,800),park' inline"
        );
    }

    /// The whole point of escaping over separator juggling: an argument
    /// rewritten after construction cannot invalidate anything, because
    /// nothing was decided at construction.
    #[test]
    fn escaping_survives_arguments_rewritten_after_construction() {
        let mut cmd = Originate::inline(
            null_endpoint(),
            [Application::new("bridge", Some("${codecs}sofia/gw/1"))],
        )
        .unwrap();

        let OriginateTarget::InlineApplications(apps) = cmd.target_mut() else {
            panic!("expected InlineApplications");
        };
        *apps[0].args_mut() = Some("{absolute_codec_string=G722,PCMU}sofia/gw/1".to_string());

        assert!(cmd
            .to_string()
            .contains(r"G722\\,PCMU"));
    }

    #[test]
    fn inline_escaped_separator_round_trips() {
        let cmd = Originate::inline(
            null_endpoint(),
            [
                Application::new("playback", Some("tone_stream://%(500,0,800)")),
                Application::new("bridge", Some("{a=1,b=2}null/farend")),
            ],
        )
        .unwrap();

        let rendered = cmd.to_string();
        let parsed: Originate = rendered
            .parse()
            .unwrap();
        assert_eq!(parsed.to_string(), rendered);
        assert_eq!(parsed.target(), cmd.target());
    }

    #[test]
    fn inline_with_delimiter_overrides_the_choice() {
        let cmd = Originate::inline_with_delimiter(
            null_endpoint(),
            [Application::simple("answer"), Application::simple("park")],
            ';',
        )
        .unwrap();

        assert_eq!(cmd.inline_delimiter(), Some(';'));
        assert!(cmd
            .to_string()
            .contains("m:;:answer;park"));
    }

    #[test]
    fn inline_with_delimiter_rejects_colon() {
        // `inline_dialplan_hunt` splits application from data on the first
        // colon, so a colon separator cannot be recovered.
        let err =
            Originate::inline_with_delimiter(null_endpoint(), [Application::simple("park")], ':')
                .unwrap_err();
        assert!(matches!(err, OriginateError::InvalidInlineDelimiter(':')));
    }

    /// The hunt's split pairs quotes, reads `\n r t s` as escapes and takes a non-ASCII
    /// separator as one byte, so such a separator never carries every argument.
    #[test]
    fn inline_delimiter_refuses_what_breaks_the_hunt_split() {
        let parsed = [
            ' ', '\'', '\\', ':', '\t', '\n', '\u{1}', '\u{7f}', 'n', 'r', 't', 's',
        ];
        for delimiter in parsed
            .into_iter()
            .chain(['é'])
        {
            assert_eq!(
                Originate::inline_with_delimiter(null_endpoint(), [Application::park()], delimiter),
                Err(OriginateError::InvalidInlineDelimiter(delimiter)),
                "{delimiter:?}"
            );
        }
        for delimiter in parsed {
            let list = originate_quote(&format!("m:{delimiter}:park"));
            let line = format!("originate loopback/9199 {list} inline");
            assert_eq!(
                line.parse::<Originate>(),
                Err(OriginateError::InvalidInlineDelimiter(delimiter)),
                "{line:?}"
            );
        }
    }

    #[test]
    fn invalid_inline_delimiter_display_omits_it() {
        let shown = OriginateError::InvalidInlineDelimiter('~').to_string();
        assert!(!shown.contains('~'), "{shown}");
    }

    /// An explicit separator that appears in an argument is escaped like any
    /// other, so naming one never has to be conditional on the data.
    #[test]
    fn inline_with_delimiter_escapes_its_own_separator() {
        let cmd = Originate::inline_with_delimiter(
            null_endpoint(),
            [Application::new("playback", Some("a|b"))],
            '|',
        )
        .unwrap();

        assert_eq!(
            cmd.to_string(),
            r"originate loopback/9199 'm:|:playback:a\\|b' inline"
        );
    }

    #[test]
    /// No argument can make a list unrenderable, so there is no separator
    /// exhaustion to report.
    fn inline_renders_an_argument_made_only_of_separators() {
        let cmd = Originate::inline(
            null_endpoint(),
            [Application::new("playback", Some(",|;~^!"))],
        )
        .unwrap();

        assert_eq!(
            cmd.to_string(),
            r"originate loopback/9199 'playback:\\,|;~^!' inline"
        );
    }

    /// Escaped for the hunt's split and again for the line's, a pair of quotes reaches the
    /// application as written; measured on a live switch.
    #[test]
    fn inline_delivers_an_argument_carrying_two_quotes() {
        let cases = [
            (
                Originate::inline(
                    null_endpoint(),
                    [Application::new(
                        "set",
                        Some("c=${cond('${v}' != '' ? red : black)}"),
                    )],
                )
                .unwrap(),
                r"originate loopback/9199 'set:c=${cond(\\\'${v}\\\' != \\\'\\\' ? red : black)}' inline",
            ),
            (
                Originate::inline_with_delimiter(
                    null_endpoint(),
                    [Application::new("playback", Some("a'b'c"))],
                    '|',
                )
                .unwrap(),
                r"originate loopback/9199 'm:|:playback:a\\\'b\\\'c' inline",
            ),
        ];
        for (cmd, wire) in cases {
            assert_eq!(cmd.to_string(), wire);
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire:?} failed to parse: {e}"));
            assert_eq!(parsed.target(), cmd.target(), "{wire:?}");
        }
    }

    #[test]
    fn parses_a_prefixed_inline_target() {
        let parsed: Originate = "originate loopback/9199 'm:|:answer|playback:a,b' inline"
            .parse()
            .unwrap();

        assert_eq!(parsed.inline_delimiter(), Some('|'));
        let OriginateTarget::InlineApplications(ref apps) = parsed.target() else {
            panic!("expected InlineApplications");
        };
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[1].args(), Some("a,b"));
    }

    fn test_endpoint() -> Endpoint {
        Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"))
    }

    fn spaced_endpoint() -> Endpoint {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "a b");
        Endpoint::Loopback(
            LoopbackEndpoint::new("9199")
                .with_context("test")
                .with_variables(vars),
        )
    }

    fn separated_commands() -> Vec<(Originate, &'static str)> {
        vec![
            (
                Originate::application(
                    spaced_endpoint(),
                    Application::new("socket", Some("127.0.0.1:8040 async full")),
                )
                .cid_name("it's ~ here")
                .with_argv_separator('~')
                .unwrap(),
                r"originate ^^~{k=\'a b\'}loopback/9199/test~&socket(127.0.0.1:8040 async full)~undef~undef~it\'s \~ here",
            ),
            (
                Originate::extension(test_endpoint(), "1000")
                    .context("")
                    .cid_name(" Lead ")
                    .cid_num("")
                    .with_argv_separator('~')
                    .unwrap(),
                r"originate ^^~loopback/9199/test~1000~undef~~\sLead\s~''",
            ),
            (
                Originate::application(test_endpoint(), Application::simple("park"))
                    .timeout(Duration::from_secs(30))
                    .with_argv_separator('~')
                    .unwrap(),
                "originate ^^~loopback/9199/test~&park()~undef~undef~undef~undef~30",
            ),
            (
                Originate::inline(
                    null_endpoint(),
                    [
                        Application::new("playback", Some("tone_stream://%(500,0,800)")),
                        Application::simple("park"),
                    ],
                )
                .unwrap()
                .dialplan(DialplanType::Inline)
                .unwrap()
                .with_argv_separator('~')
                .unwrap(),
                r"originate ^^~loopback/9199~playback:tone_stream://%(500\\,0\\,800),park~inline",
            ),
            (
                Originate::extension(test_endpoint(), "1000")
                    .dialplan(DialplanType::Xml)
                    .unwrap()
                    .context("ctx with space")
                    .cid_num("555 1234")
                    .with_argv_separator('!')
                    .unwrap(),
                "originate ^^!loopback/9199/test!1000!XML!ctx with space!undef!555 1234",
            ),
        ]
    }

    /// Each argument is escaped once for the split on the separator, an absent slot a later
    /// one forces reads `undef`, and an empty value stays an argument.
    #[test]
    fn argv_separator_renders_each_argument_escaped_once() {
        for (cmd, wire) in separated_commands() {
            assert_eq!(cmd.to_string(), wire);
            assert_eq!(
                cmd.display_with(BlockParse::PairSplitCleans)
                    .to_string(),
                wire
            );
        }
    }

    #[test]
    fn argv_separator_round_trips() {
        for (cmd, wire) in separated_commands() {
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire} failed to parse: {e}"));
            assert_eq!(parsed, cmd, "{wire}");
            assert_eq!(parsed.to_string(), wire);
        }
    }

    #[test]
    fn argv_separator_is_absent_by_default() {
        assert_eq!(
            Originate::extension(test_endpoint(), "1000").argv_separator(),
            None
        );
        assert_eq!(
            "originate loopback/9199/test 1000"
                .parse::<Originate>()
                .unwrap()
                .argv_separator(),
            None
        );
    }

    /// Measured: `undef` in any case and quoting reads as absent, `\undef` keeps its
    /// backslash, an empty token is an empty value.
    #[test]
    fn argv_separator_parse_reads_the_switch_spellings() {
        let parsed: Originate = r"originate ^^~loopback/9199/test~&park()~XML~~'UNDEF'~\undef"
            .parse()
            .unwrap();
        assert_eq!(parsed.argv_separator(), Some('~'));
        assert_eq!(parsed.dialplan_type(), Some(&DialplanType::Xml));
        assert_eq!(parsed.context_str(), Some(""));
        assert_eq!(parsed.caller_id_name(), None);
        assert_eq!(parsed.caller_id_number(), Some(r"\undef"));

        let parsed: Originate =
            r"originate ^^~loopback/9199/test~&park()~undef~undef~  Lead Trail  ~\s\sx"
                .parse()
                .unwrap();
        assert_eq!(parsed.dialplan_type(), None);
        assert_eq!(parsed.context_str(), None);
        assert_eq!(parsed.caller_id_name(), Some("Lead Trail"));
        assert_eq!(parsed.caller_id_number(), Some("  x"));
    }

    #[test]
    fn argv_separator_parse_takes_the_endpoint_at_the_separator_target() {
        let parsed: Originate = r"originate ^^~{k=\'a b\'}loopback/9199/test~&park()"
            .parse()
            .unwrap();
        assert_eq!(parsed.endpoint(), &spaced_endpoint());
    }

    #[test]
    fn an_unusable_argv_separator_is_refused() {
        let err = Originate::application(test_endpoint(), Application::simple("park"))
            .with_argv_separator('\'')
            .unwrap_err();
        assert_eq!(
            err,
            OriginateError::InvalidArgvSeparator(InvalidArgvSeparator::Unusable('\''))
        );
        assert!(std::error::Error::source(&err).is_some());

        assert_eq!(
            "originate ^^|loopback/9199/test|&park()"
                .parse::<Originate>()
                .unwrap_err(),
            OriginateError::InvalidArgvSeparator(InvalidArgvSeparator::Unusable('|'))
        );
    }

    /// `originate_function` answers usage past seven arguments.
    #[test]
    fn argv_separator_parse_refuses_what_the_switch_refuses() {
        assert!(
            "originate ^^~loopback/9199/test~&park()~XML~default~a~b~30~extra"
                .parse::<Originate>()
                .is_err()
        );
    }

    #[test]
    fn serde_argv_separator_round_trips_and_is_checked() {
        let cmd = Originate::application(test_endpoint(), Application::simple("park"))
            .with_argv_separator('~')
            .unwrap();
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""argv_separator":"~""#), "{json}");
        assert_eq!(serde_json::from_str::<Originate>(&json).unwrap(), cmd);

        let plain = serde_json::to_string(&Originate::application(
            test_endpoint(),
            Application::simple("park"),
        ))
        .unwrap();
        assert!(!plain.contains("argv_separator"), "{plain}");

        let refused = r#"{
            "endpoint": {"loopback": {"extension": "9199"}},
            "application": {"name": "park"},
            "argv_separator": "|"
        }"#;
        assert!(serde_json::from_str::<Originate>(refused).is_err());
    }

    /// `originate_function` reads `undef` in any case as an absent argument, so no spelling
    /// delivers it; a config naming one fails at load.
    #[test]
    fn serde_refuses_an_undef_positional() {
        for (field, value) in [
            ("context", "undef"),
            ("cid_name", "UNDEF"),
            ("cid_num", "Undef"),
        ] {
            let json = format!(
                r#"{{"endpoint": {{"loopback": {{"extension": "9199"}}}},
                "application": {{"name": "park"}}, "{field}": "{value}"}}"#
            );
            let msg = serde_json::from_str::<Originate>(&json)
                .expect_err(&json)
                .to_string();
            assert!(msg.contains(field), "does not name {field}: {msg}");
            assert!(
                value == "undef" || !msg.contains(value),
                "quoted its input: {msg}"
            );
        }
        let json = r#"{"endpoint": {"loopback": {"extension": "9199"}},
            "application": {"name": "park"}, "cid_name": "undefined"}"#;
        assert!(serde_json::from_str::<Originate>(json).is_ok());
    }

    /// `originate_function` NULLs an `undef` target and then asserts it is set, which aborts
    /// the switch.
    #[test]
    fn an_undef_target_is_refused() {
        for line in [
            "originate loopback/9199/test undef",
            "originate loopback/9199/test 'UNDEF'",
            "originate loopback/9199/test Undef inline",
            "originate ^^~loopback/9199/test~undef",
            "originate ^^~loopback/9199/test~UNDEF~inline",
        ] {
            assert_eq!(
                line.parse::<Originate>(),
                Err(OriginateError::UndefPositional("target")),
                "{line}"
            );
        }
        for target in [
            r#""extension": "undef""#,
            r#""extension": "UNDEF", "argv_separator": "~""#,
            r#""inline_applications": [{"name": "Undef"}]"#,
        ] {
            let json =
                format!(r#"{{"endpoint": {{"loopback": {{"extension": "9199"}}}}, {target}}}"#);
            let msg = serde_json::from_str::<Originate>(&json)
                .expect_err(&json)
                .to_string();
            assert!(msg.contains("target"), "does not name the target: {msg}");
        }
        assert!(Originate::extension(test_endpoint(), "undefined")
            .to_string()
            .parse::<Originate>()
            .is_ok());
    }

    /// The blank split reads its positionals by the same rules as a separator split: the
    /// third argument is strictly the dialplan, `undef` is absent, and an eighth is refused.
    #[test]
    fn blank_positionals_read_as_the_separator_split_does() {
        assert!(
            "originate loopback/9199/test &park() XML default a b 30 extra"
                .parse::<Originate>()
                .is_err()
        );

        let parsed: Originate = "originate loopback/9199/test 1000 ctx"
            .parse()
            .unwrap();
        assert_eq!(parsed.dialplan_type(), None);
        assert_eq!(parsed.dialplan_name(), Some("ctx"));
        assert_eq!(parsed.context_str(), None);

        let parsed: Originate = "originate loopback/9199/test &park() undef UNDEF Alice"
            .parse()
            .unwrap();
        assert_eq!(parsed.dialplan_type(), None);
        assert_eq!(parsed.context_str(), None);
        assert_eq!(parsed.caller_id_name(), Some("Alice"));

        let parsed: Originate = "originate loopback/9199/test &park() undef ctx"
            .parse()
            .unwrap();
        assert_eq!(parsed.dialplan_type(), None);
        assert_eq!(parsed.context_str(), Some("ctx"));
    }

    /// The blank split collapses a run of spaces and splits a bare space, so an empty or
    /// spaced positional is quoted to arrive as one argument.
    #[test]
    fn blank_split_quotes_empty_and_spaced_positionals() {
        let cases = [
            (
                Originate::extension(test_endpoint(), "1000")
                    .context("")
                    .cid_name("Alice"),
                "originate loopback/9199/test 1000 undef '' Alice",
            ),
            (
                Originate::extension(test_endpoint(), "1000")
                    .context("ctx with space")
                    .cid_name("Alice"),
                "originate loopback/9199/test 1000 undef 'ctx with space' Alice",
            ),
            (
                Originate::extension(test_endpoint(), "1000")
                    .context("test")
                    .cid_name("Alice")
                    .cid_num(""),
                "originate loopback/9199/test 1000 undef test Alice ''",
            ),
        ];
        for (cmd, wire) in cases {
            assert_eq!(cmd.to_string(), wire);
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire} failed to parse: {e}"));
            assert_eq!(parsed.context_str(), cmd.context_str(), "{wire}");
            assert_eq!(parsed.caller_id_name(), cmd.caller_id_name(), "{wire}");
            assert_eq!(parsed.caller_id_number(), cmd.caller_id_number(), "{wire}");
            assert_eq!(parsed.to_string(), wire);
        }
    }

    /// A lone quote on the blank split is wrapped and escaped, so the split closes the region
    /// it opens and its cleanup delivers the quote.
    #[test]
    fn blank_split_delivers_a_single_quote_in_any_argument() {
        let cases = [
            (
                Originate::application(test_endpoint(), Application::simple("park"))
                    .dialplan(DialplanType::Xml)
                    .unwrap()
                    .context("default")
                    .cid_name("it's"),
                r"originate loopback/9199/test &park() XML default 'it\'s'",
            ),
            (
                Originate::application(
                    test_endpoint(),
                    Application::new("set", Some(r"quoted=it's\n")),
                ),
                r"originate loopback/9199/test '&set(quoted=it\'s\\n)'",
            ),
            (
                Originate::inline(null_endpoint(), [Application::new("set", Some("a=it's,b"))])
                    .unwrap()
                    .dialplan(DialplanType::Inline)
                    .unwrap(),
                r"originate loopback/9199 'set:a=it\\\'s\\,b' inline",
            ),
            (
                Originate::application(
                    test_endpoint(),
                    Application::new("set", Some(r"lit=a\nb\\c\sd")),
                )
                .cid_name(r"x\ny")
                .cid_num(r"\t"),
                r"originate loopback/9199/test '&set(lit=a\\nb\\\\c\\sd)' undef undef 'x\\ny' '\\t'",
            ),
        ];
        for (cmd, wire) in cases {
            assert_eq!(cmd.to_string(), wire);
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire} failed to parse: {e}"));
            assert_eq!(parsed, cmd, "{wire}");
        }

        let parsed: Originate = r"originate loopback/9199/test &park() XML default it\'s"
            .parse()
            .unwrap();
        assert_eq!(parsed.caller_id_name(), Some("it's"));
    }

    /// The hunt's split cleans each action up, reading `\\`, `\'`, `\n`, `\s` and the rest and
    /// trimming a space at the action's end, so an argument escapes for it once.
    #[test]
    fn an_inline_argument_escapes_for_the_hunt_split() {
        let cases = [
            (
                Some("v= "),
                None,
                r"originate loopback/9199 'set:v=\\s,park' inline",
            ),
            (
                Some(r"v=a\nb"),
                None,
                r"originate loopback/9199 'set:v=a\\\\nb,park' inline",
            ),
            (
                Some("v=a\tb "),
                Some('~'),
                r"originate ^^~loopback/9199~set:v=a\\tb\\s,park~inline",
            ),
        ];
        for (args, sep, wire) in cases {
            let cmd = Originate::inline(
                null_endpoint(),
                [Application::new("set", args), Application::park()],
            )
            .unwrap();
            let cmd = match sep {
                Some(sep) => cmd
                    .with_argv_separator(sep)
                    .unwrap(),
                None => cmd,
            };
            assert_eq!(cmd.to_string(), wire);
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire:?} failed to parse: {e}"));
            assert_eq!(parsed.target(), cmd.target(), "{wire:?}");
        }
    }

    /// `originate_function` hands the target to the inline hunt only under the inline dialplan;
    /// under any other it transfers the action list as an extension.
    #[test]
    fn inline_applications_refuse_another_dialplan() {
        let inline = || Originate::inline(null_endpoint(), [Application::simple("park")]).unwrap();
        for refused in [
            inline().dialplan(DialplanType::Xml),
            inline().dialplan_raw(""),
            inline().dialplan_raw("nosuchdp"),
        ] {
            assert_eq!(refused, Err(OriginateError::InlineApplicationsWithDialplan));
        }
        assert!(inline()
            .dialplan(DialplanType::Inline)
            .is_ok());
        assert!(inline()
            .dialplan_raw("INLINE")
            .is_ok());
        let json = r#"{"endpoint": {"loopback": {"extension": "9199"}},
            "inline_applications": [{"name": "park"}], "dialplan": "xml"}"#;
        assert_eq!(
            serde_json::from_str::<Originate>(json)
                .unwrap_err()
                .to_string(),
            OriginateError::InlineApplicationsWithDialplan.to_string()
        );
    }

    /// `originate_function` runs a target opening `&` and more as an application and ends its
    /// arguments at the first `)`.
    #[test]
    fn a_target_originate_reads_as_something_else_is_refused() {
        assert_eq!(
            "originate loopback/9199/test &park(a)b)".parse::<Originate>(),
            Err(OriginateError::ParenthesisInApplication {
                application: "park".into()
            })
        );
        for target in [
            r#""application": {"name": "park", "args": "a)b"}"#,
            r#""application": {"name": "pa(rk"}"#,
            r#""extension": "&park()""#,
            r#""extension": "&x""#,
        ] {
            let json =
                format!(r#"{{"endpoint": {{"loopback": {{"extension": "9199"}}}}, {target}}}"#);
            let msg = serde_json::from_str::<Originate>(&json)
                .expect_err(&json)
                .to_string();
            assert!(!msg.contains("park") && !msg.contains("a)b"), "{msg}");
        }
        let json = r#"{"endpoint": {"loopback": {"extension": "9199"}}, "extension": "&"}"#;
        assert!(serde_json::from_str::<Originate>(json).is_ok());
    }

    /// An inline action list reads an argument up to its separator, not its first `)`, so the
    /// refusal names the form that delivers a tone stream's parenthesised spec.
    #[test]
    fn a_parenthesis_refusal_points_at_the_inline_action_list() {
        let json = r#"{"endpoint": {"loopback": {"extension": "9199"}},
            "application": {"name": "playback", "args": "tone_stream://%(500,0,800)"}}"#;
        let msg = serde_json::from_str::<Originate>(json)
            .expect_err(json)
            .to_string();
        assert!(msg.contains("Originate::inline"), "{msg}");

        let inline = r#"{"endpoint": {"loopback": {"extension": "9199"}},
            "inline_applications": [{"name": "playback", "args": "tone_stream://%(500,0,800)"}]}"#;
        let originate = serde_json::from_str::<Originate>(inline).expect(inline);
        assert_eq!(
            originate.to_string(),
            r"originate loopback/9199 'playback:tone_stream://%(500\\,0\\,800)' inline"
        );
    }

    #[test]
    fn an_application_under_the_inline_dialplan_and_a_lone_ampersand_round_trip() {
        let cases = [
            (
                Originate::application(test_endpoint(), Application::new("park", Some(":,")))
                    .dialplan(DialplanType::Inline)
                    .unwrap(),
                "originate loopback/9199/test &park(:,) inline",
            ),
            (
                Originate::extension(test_endpoint(), "&"),
                "originate loopback/9199/test &",
            ),
        ];
        for (cmd, wire) in cases {
            assert_eq!(cmd.to_string(), wire);
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire:?} failed to parse: {e}"));
            assert_eq!(parsed, cmd, "{wire:?}");
        }
    }

    /// `switch_api_execute` strips tab, vertical tab, CR, newline and space from the edges of
    /// the argument line, so a last positional ending in one is kept inside quotes.
    #[test]
    fn a_last_positional_ending_in_stripped_whitespace_arrives() {
        let cases = [
            (
                Originate::extension(test_endpoint(), "1000").cid_num("\t"),
                "originate loopback/9199/test 1000 undef undef undef '\t'",
            ),
            (
                Originate::extension(test_endpoint(), "1000").cid_num("x\u{b}"),
                "originate loopback/9199/test 1000 undef undef undef 'x\u{b}'",
            ),
            (
                Originate::extension(test_endpoint(), "1000")
                    .cid_num("\u{b}")
                    .with_argv_separator('~')
                    .unwrap(),
                "originate ^^~loopback/9199/test~1000~undef~undef~undef~''\u{b}''",
            ),
        ];
        for (cmd, wire) in cases {
            assert_eq!(cmd.to_string(), wire);
            let parsed: Originate = wire
                .parse()
                .unwrap_or_else(|e| panic!("{wire:?} failed to parse: {e}"));
            assert_eq!(parsed, cmd, "{wire:?}");
        }
    }

    /// `originate_function` hands any word in the dialplan slot to the transfer, which looks
    /// up a dialplan module by that name.
    #[test]
    fn any_dialplan_name_is_kept() {
        for (line, name) in [
            ("originate loopback/9199/test 1000 nosuchdp ctx", "nosuchdp"),
            (
                "originate ^^~loopback/9199/test~1000~nosuchdp~ctx",
                "nosuchdp",
            ),
            ("originate loopback/9199/test 1000 '' ctx", ""),
        ] {
            let parsed: Originate = line
                .parse()
                .unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(parsed.dialplan_type(), None, "{line}");
            assert_eq!(parsed.dialplan_name(), Some(name), "{line}");
            assert_eq!(parsed.context_str(), Some("ctx"), "{line}");
            assert_eq!(parsed.to_string(), line);
        }

        let typed: Originate = "originate loopback/9199/test 1000 xml ctx"
            .parse()
            .unwrap();
        assert_eq!(typed.dialplan_type(), Some(&DialplanType::Xml));
        assert_eq!(typed.dialplan_name(), Some("XML"));

        let named = Originate::extension(test_endpoint(), "1000")
            .dialplan_raw("nosuchdp")
            .unwrap();
        assert_eq!(
            named.to_string(),
            "originate loopback/9199/test 1000 nosuchdp"
        );
        assert_eq!(
            Originate::extension(test_endpoint(), "1000").dialplan_raw("INLINE"),
            Err(OriginateError::ExtensionWithInlineDialplan)
        );
        let json = serde_json::to_string(&named).unwrap();
        assert!(json.contains(r#""dialplan":"nosuchdp""#), "{json}");
        assert_eq!(serde_json::from_str::<Originate>(&json).unwrap(), named);
    }

    #[test]
    fn serde_dialplan_keeps_existing_configs_and_refuses_undef() {
        let config = |dialplan: &str| {
            format!(
                r#"{{"endpoint": {{"loopback": {{"extension": "9199"}}}},
                "extension": "1000", "dialplan": "{dialplan}"}}"#
            )
        };
        for (dialplan, typed) in [("xml", DialplanType::Xml), ("XML", DialplanType::Xml)] {
            let cmd: Originate = serde_json::from_str(&config(dialplan)).unwrap();
            assert_eq!(cmd.dialplan_type(), Some(&typed));
            assert!(serde_json::to_string(&cmd)
                .unwrap()
                .contains(r#""dialplan":"xml""#));
        }
        let msg = serde_json::from_str::<Originate>(&config("Undef"))
            .expect_err("undef dialplan")
            .to_string();
        assert!(msg.contains("dialplan"), "{msg}");
    }

    /// `switch_separate_string` takes `^^ ` as picking the blank split itself.
    #[test]
    fn a_blank_argv_separator_picks_the_blank_split() {
        let parsed: Originate = "originate ^^ loopback/9199/test &park() XML ctx"
            .parse()
            .unwrap();
        assert_eq!(parsed.argv_separator(), None);
        assert_eq!(parsed.endpoint(), &test_endpoint());
        assert_eq!(parsed.context_str(), Some("ctx"));
        assert_eq!(
            parsed.to_string(),
            "originate loopback/9199/test &park() XML ctx"
        );
    }

    /// The switch splits any dial string holding `:_:` into enterprise threads, quoted or not.
    #[test]
    fn an_enterprise_separator_in_a_variable_value_is_refused() {
        assert!("originate {k=x:_:y}loopback/9199/test &park()"
            .parse::<Originate>()
            .is_err());
    }
}
