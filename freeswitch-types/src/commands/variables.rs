//! Channel variable scope and ordered key-value storage for originate commands.
//!
//! Line numbers in this module index FreeSWITCH `v1.11.1`
//! (`c2c59645f6911a76589e5008c4d73349ded44b65`).

use indexmap::IndexMap;
use std::borrow::Cow;
use std::fmt;
use std::fmt::Write as _;
use std::str::FromStr;

use super::flattened::pipeline::{names_a_variable, ENTERPRISE_DELIM};
use super::originate::OriginateError;
use crate::tokenizer::{sole_argument, trace, untrace, ArgvCut, Token, Traced};
use crate::version::FreeswitchVersion;

/// Scope for channel variables in an originate command.
///
/// - `Enterprise` (`<>`) -- applies across all threads (`:_:` separated)
/// - `Default` (`{}`) -- applies to all channels in this originate
/// - `Channel` (`[]`) -- applies only to one specific channel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[non_exhaustive]
pub enum VariablesType {
    /// `<>` scope -- applies across all `:_:` separated threads.
    Enterprise,
    /// `{}` scope -- applies to all channels in this originate.
    Default,
    /// `[]` scope -- applies to one specific channel.
    Channel,
}

impl VariablesType {
    pub(super) fn delimiters(self) -> (char, char) {
        match self {
            Self::Enterprise => ('<', '>'),
            Self::Default => ('{', '}'),
            Self::Channel => ('[', ']'),
        }
    }

    /// A `[]` block rides through the `|` and `,` leg splits before the block
    /// parse, and both consume escapes.
    fn leg_split_passes(self) -> u32 {
        match self {
            Self::Enterprise | Self::Default => 0,
            Self::Channel => LEG_SPLIT_PASSES,
        }
    }
}

/// `switch_ivr_originate` cuts a thread into groups on `|` and a group into legs on `,`, each
/// through `cleanup_separated_string`.
const LEG_SPLIT_PASSES: u32 = 2;

/// Text escaped for the passes that read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EscapedField {
    /// A pair's value in a block of `scope`, whose pairs split on commas when `commas_separate`.
    Value {
        scope: VariablesType,
        commas_separate: bool,
    },
    /// A leg's text after its blocks, which the leg splits read and the endpoint module parses.
    Endpoint,
    /// A pair's key, which meets the value's passes and ends the `=` split.
    Key {
        scope: VariablesType,
        commas_separate: bool,
    },
}

impl EscapedField {
    fn escapes_comma(self) -> bool {
        match self {
            Self::Value {
                commas_separate, ..
            }
            | Self::Key {
                commas_separate, ..
            } => commas_separate,
            Self::Endpoint => true,
        }
    }

    fn escapes_pipe(self) -> bool {
        match self {
            Self::Value { scope, .. } | Self::Key { scope, .. } => scope == VariablesType::Channel,
            Self::Endpoint => true,
        }
    }
}

/// Ordered set of channel variables with FreeSWITCH escaping.
///
/// A comma is escaped with `\,`, a backslash and a single quote with as many
/// backslashes as the [`DialStringTarget`]'s passes consume, a space at either
/// edge of a value as a `\s` escaped for the same passes, and a value with other
/// spaces is wrapped in single quotes. This form round-trips through [`FromStr`];
/// what the switch itself decodes depends on which command carries the block
/// and which parser revision reads it, documented in `docs/dial-string-format.md`.
///
/// A key meets the same passes and is escaped the same way, with `\=` for the `=` split and an
/// empty `''` ahead of one opening `^^`.
///
/// A value naming a variable (`${…}`) is left to the switch, which expands it or
/// drops it at install unless `origination_nested_vars` is true.
///
/// # Serde format
///
/// [`Default`](VariablesType::Default) scope with the comma separator
/// serializes as a flat JSON map: `{"key": "value", ...}`. Anything else
/// serializes as `{"scope": "enterprise", "vars": {"key": "value"}}`, carrying
/// a `"separator"` field only when [`with_separator`](Variables::with_separator)
/// chose one. Deserialization accepts both formats; a flat map implies
/// `Default` scope and the comma. A `separator` that cannot delimit the block,
/// or that a key or value already contains, is refused at load.
// qual:allow(srp, god_struct) reason: "public builder; accessors read disjoint fields"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variables {
    vars_type: VariablesType,
    inner: IndexMap<String, String>,
    /// Set by [`with_separator`](Variables::with_separator). Parsing a `^^`
    /// block does not populate it: reading one back and writing it out
    /// canonicalises to a comma, and the other form is asked for explicitly.
    separator: Option<char>,
}

/// Which command carries a dial string, and therefore how deeply its variable
/// values must be escaped.
///
/// FreeSWITCH escape-processes a bracket block a different number of times
/// depending on the command it arrived on, and the correct escaping of a value
/// containing a single quote differs between them with no form that satisfies
/// both. Naming the carrier is the only way a renderer can be right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum DialStringCarrier {
    /// `api originate` / `bgapi originate`, whose argument list is split before
    /// the block itself is parsed. The default for [`Display`](fmt::Display).
    EslApi,
    /// A dialplan application such as `bridge`, including via `sendmsg execute`,
    /// which receives its argument whole.
    Dialplan,
}

impl DialStringCarrier {
    /// Dialplan variable expansion deletes an escaped quote outright, where every
    /// other pass unescapes it, and drops the first `$` of a `$$`.
    fn expands(self) -> bool {
        matches!(self, Self::Dialplan)
    }
}

/// Which revision of the switch's bracket-block parser a dial string is rendered
/// for.
///
/// How many escape-consuming passes a block meets is the switch's to change, and
/// every escaped quote and backslash depends on it. It covers `{}`, `<>` and `[]`
/// escaping only: the inline action list and the quote pre-scan ahead of a `[]`
/// block are other parsers. Which revisions a FreeSWITCH version is known to run
/// is documented in `docs/dial-string-format.md`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum BlockParse {
    /// Both of the block's own splits, on the separator and on `=`, run the
    /// escape-consuming cleanup.
    #[default]
    PairSplitCleans,
}

impl BlockParse {
    const ALL: &'static [Self] = &[Self::PairSplitCleans];

    fn as_str(self) -> &'static str {
        match self {
            Self::PairSplitCleans => "pair_split_cleans",
        }
    }

    fn cleanup_passes(self) -> u32 {
        match self {
            Self::PairSplitCleans => 2,
        }
    }

    /// The revision a FreeSWITCH release is vouched to run, or why it cannot be.
    ///
    /// A release is vouched for only inside a range whose block parse
    /// (`switch_event.c:1737`, `switch_utils.c:2804`) was checked at every tag and
    /// whose line passed the live escaping suite. Anything else, a development
    /// build included, is refused; name a revision explicitly for it instead.
    pub fn for_version(version: &FreeswitchVersion) -> Result<Self, UnvouchedVersion> {
        let version = *version;
        let (first, last) = VOUCHED_PAIR_SPLIT_CLEANS;
        if version.is_dev() {
            Err(UnvouchedVersion::Dev { version })
        } else if version < first {
            Err(UnvouchedVersion::OlderThanVouched { version })
        } else if version > last {
            Err(UnvouchedVersion::NewerThanVouched { version })
        } else {
            Ok(Self::PairSplitCleans)
        }
    }
}

const VOUCHED_PAIR_SPLIT_CLEANS: (FreeswitchVersion, FreeswitchVersion) = (
    FreeswitchVersion::new(1, 10, 0),
    FreeswitchVersion::new(1, 10, 12),
);

/// A FreeSWITCH version [`BlockParse::for_version`] cannot vouch for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnvouchedVersion {
    /// A development build, whose version stays the same across commits.
    Dev {
        /// The version refused.
        version: FreeswitchVersion,
    },
    /// A release newer than the vouched range.
    NewerThanVouched {
        /// The version refused.
        version: FreeswitchVersion,
    },
    /// A release older than the vouched range.
    OlderThanVouched {
        /// The version refused.
        version: FreeswitchVersion,
    },
}

impl UnvouchedVersion {
    /// The version refused.
    pub fn version(&self) -> FreeswitchVersion {
        match *self {
            Self::Dev { version }
            | Self::NewerThanVouched { version }
            | Self::OlderThanVouched { version } => version,
        }
    }
}

impl fmt::Display for UnvouchedVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (first, last) = VOUCHED_PAIR_SPLIT_CLEANS;
        let why = match self {
            Self::Dev { .. } => "is a development build",
            Self::NewerThanVouched { .. } => "is newer than any vouched release",
            Self::OlderThanVouched { .. } => "is older than any vouched release",
        };
        write!(
            f,
            "FreeSWITCH {} {why}; releases {first} to {last} are vouched for, name a BlockParse explicitly",
            self.version()
        )
    }
}

impl std::error::Error for UnvouchedVersion {}

impl fmt::Display for BlockParse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BlockParse {
    type Err = ParseBlockParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|revision| {
                revision
                    .as_str()
                    .eq_ignore_ascii_case(s)
            })
            .ok_or_else(|| ParseBlockParseError(s.to_string()))
    }
}

/// A [`BlockParse`] name this crate does not know. The rejected name is the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseBlockParseError(pub String);

impl fmt::Display for ParseBlockParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown block-parse revision, expected one of:")?;
        for revision in BlockParse::ALL {
            write!(f, " {revision}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseBlockParseError {}

/// Where a dial string is headed: the command carrying it, the block-parser
/// revision of the switch reading it, and the split cutting that command's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialStringTarget {
    carrier: DialStringCarrier,
    block_parse: BlockParse,
    argument: ArgumentPass,
}

/// The pass cutting a dial string out of its command's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgumentPass {
    /// The carrier's own pass.
    Blank,
    /// `originate` after a leading `^^<sep>`.
    Char(char),
    /// Escaped for at the rendered argument's edge, so the renders inside it skip the pass.
    Consumed,
}

/// Why a [`DialStringTarget`] takes no argument separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidArgvSeparator {
    /// The carrier hands the dial string over whole, so no argument split reads it.
    WrongCarrier(DialStringCarrier),
    /// A separator [`DialStringTarget::with_argv_separator`] refuses.
    Unusable(char),
}

impl fmt::Display for InvalidArgvSeparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongCarrier(carrier) => {
                write!(f, "the {carrier:?} carrier takes no argument separator")
            }
            Self::Unusable(sep) => write!(f, "{sep:?} cannot separate originate's arguments"),
        }
    }
}

impl std::error::Error for InvalidArgvSeparator {}

/// Space, controls, non-ASCII, `\`, `'` and lowercase `n r t s`, which break a split on `sep` or
/// the escapes its cleanup reads.
fn breaks_a_split(sep: char) -> bool {
    !sep.is_ascii_graphic() || matches!(sep, '\\' | '\'' | 'n' | 'r' | 't' | 's')
}

/// Not [`breaks_a_split`]; as policy, not what reads as dial-string grammar, quoting or a word.
fn usable_argv_separator(sep: char) -> bool {
    !breaks_a_split(sep)
        && !sep.is_ascii_alphanumeric()
        && !matches!(
            sep,
            '^' | '"' | ',' | '|' | '[' | ']' | '{' | '}' | '<' | '>' | '=' | ':'
        )
}

/// Escapes for one split on `sep` and its cleanup: `\`, `'` and `sep` take a backslash, newline,
/// CR and tab their letter, and a space reads `\s` at either edge, or everywhere when `sep` is one.
/// A vertical tab, which no escape names, is kept at either edge by an empty `''` beside it.
struct ArgumentEscape<W> {
    out: W,
    sep: char,
    started: bool,
    spaces: usize,
    vertical_tab_last: bool,
}

impl<W: fmt::Write> fmt::Write for ArgumentEscape<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if c == ' ' && self.started && self.sep != ' ' {
                self.spaces += 1;
                continue;
            }
            for _ in 0..std::mem::take(&mut self.spaces) {
                self.out
                    .write_char(' ')?;
            }
            if c == '\u{b}' && !self.started {
                self.out
                    .write_str("''")?;
            }
            self.started = true;
            self.vertical_tab_last = c == '\u{b}';
            match c {
                ' ' => self
                    .out
                    .write_str(r"\s")?,
                '\n' => self
                    .out
                    .write_str(r"\n")?,
                '\r' => self
                    .out
                    .write_str(r"\r")?,
                '\t' => self
                    .out
                    .write_str(r"\t")?,
                c if c == '\\' || c == '\'' || c == self.sep => {
                    self.out
                        .write_char('\\')?;
                    self.out
                        .write_char(c)?;
                }
                c => self
                    .out
                    .write_char(c)?,
            }
        }
        Ok(())
    }
}

/// Write `inner` as one argument of the split on `sep`.
pub(crate) fn write_escaped(
    out: impl fmt::Write,
    sep: char,
    inner: impl fmt::Display,
) -> fmt::Result {
    let mut escape = ArgumentEscape {
        out,
        sep,
        started: false,
        spaces: 0,
        vertical_tab_last: false,
    };
    write!(escape, "{inner}")?;
    let Some(kept) = escape
        .spaces
        .checked_sub(1)
    else {
        if escape.vertical_tab_last {
            escape
                .out
                .write_str("''")?;
        }
        return Ok(());
    };
    for _ in 0..kept {
        escape
            .out
            .write_char(' ')?;
    }
    escape
        .out
        .write_str(r"\s")
}

impl DialStringTarget {
    /// Target `carrier` at the default [`BlockParse`].
    pub fn new(carrier: DialStringCarrier) -> Self {
        Self {
            carrier,
            block_parse: BlockParse::default(),
            argument: ArgumentPass::Blank,
        }
    }

    /// Split `originate`'s arguments on `sep`, as a line opening with `^^<sep>` asks the switch.
    ///
    /// The dial string is one argument of that split: rendered, it is escaped once for the
    /// split's cleanup; parsed, that cleanup runs first and a second argument is refused.
    ///
    /// Refused: separators the switch splits or unescapes wrongly (space, `\`, `'`, lowercase
    /// `n r t s`, controls, non-ASCII) and, as policy, `^ " , | [ ] { } < > = :`, which read as
    /// dial-string grammar or quoting, and every other letter or digit, which reads as part of a word.
    /// A `'` separator pairs with the next one as quotes.
    pub fn with_argv_separator(mut self, sep: char) -> Result<Self, InvalidArgvSeparator> {
        match self.carrier {
            DialStringCarrier::EslApi => {}
            DialStringCarrier::Dialplan => {
                return Err(InvalidArgvSeparator::WrongCarrier(self.carrier))
            }
        }
        if !usable_argv_separator(sep) {
            return Err(InvalidArgvSeparator::Unusable(sep));
        }
        self.argument = ArgumentPass::Char(sep);
        Ok(self)
    }

    /// The separator cutting `originate`'s arguments in place of the blank split.
    pub fn argv_separator(&self) -> Option<char> {
        match self.argument {
            ArgumentPass::Char(sep) => Some(sep),
            ArgumentPass::Blank | ArgumentPass::Consumed => None,
        }
    }

    /// `text` escaped as one argument of `originate`'s split, on blanks or on the
    /// [`argv_separator`](Self::argv_separator), or `None` at the dialplan carrier, which splits none.
    ///
    /// An empty text is written `''`, which the split keeps as an argument. On blanks a text
    /// opening `^^` is written after `''`, since a line opening `^^` names its own separator.
    pub fn escape_argument<'a>(&self, text: &'a str) -> Option<Cow<'a, str>> {
        let sep = self.split_delimiter()?;
        if text.is_empty() {
            return Some(Cow::Borrowed("''"));
        }
        let guarded = sep == ' ' && text.starts_with("^^");
        let plain = !guarded
            && !text.starts_with([' ', '\u{b}'])
            && !text.ends_with([' ', '\u{b}'])
            && !text.contains(['\\', '\'', '\n', '\r', '\t', sep]);
        if plain {
            return Some(Cow::Borrowed(text));
        }
        let mut escaped = String::with_capacity(text.len() + 8);
        if guarded {
            escaped.push_str("''");
        }
        // Writing to a String cannot fail.
        write_escaped(&mut escaped, sep, text).ok()?;
        Some(Cow::Owned(escaped))
    }

    /// `render` at this target, escaped once at its edge for an argv separator's split.
    pub(crate) fn write_argument(
        self,
        f: &mut fmt::Formatter<'_>,
        render: impl Fn(&mut fmt::Formatter<'_>, Self) -> fmt::Result,
    ) -> fmt::Result {
        match self.argv_separator() {
            Some(sep) => write_escaped(
                f,
                sep,
                RenderedAt {
                    target: self.inner(),
                    render: &render,
                },
            ),
            None => render(f, self),
        }
    }

    /// A separator [`with_argv_separator`](Self::with_argv_separator) refuses as policy, for
    /// reading captures the switch accepts under it.
    #[cfg(test)]
    pub(crate) fn with_unchecked_argv_separator(mut self, sep: char) -> Self {
        self.argument = ArgumentPass::Char(sep);
        self
    }

    /// The delimiter of `originate`'s argument split still ahead of this target, if any.
    fn split_delimiter(self) -> Option<char> {
        match (self.carrier, self.argument) {
            (DialStringCarrier::EslApi, ArgumentPass::Blank) => Some(' '),
            (_, ArgumentPass::Char(sep)) => Some(sep),
            (DialStringCarrier::Dialplan, ArgumentPass::Blank) | (_, ArgumentPass::Consumed) => {
                None
            }
        }
    }

    /// The one argument `originate`'s split leaves of `text`, or `None` where no split reads it.
    pub(crate) fn split_argument(self, text: &[Traced]) -> Option<Result<Option<Token>, ArgvCut>> {
        self.split_delimiter()
            .map(|delim| sole_argument(text, delim))
    }

    /// This target inside an argument its split already read.
    pub(crate) fn inner(mut self) -> Self {
        if self
            .split_delimiter()
            .is_some()
        {
            self.argument = ArgumentPass::Consumed;
        }
        self
    }

    /// What this target's argument split leaves of `s`, and the target reading that.
    pub(crate) fn read_argument(self, s: &str) -> Result<(Cow<'_, str>, Self), OriginateError> {
        let Some(token) = self.split_argument(&trace(s)) else {
            return Ok((Cow::Borrowed(s), self));
        };
        let token = token.map_err(|ArgvCut| {
            OriginateError::ParseError("originate's argument split cuts the dial string".into())
        })?;
        let argument = token.map_or_else(String::new, |token| untrace(&token.text));
        Ok((Cow::Owned(argument), self.inner()))
    }

    /// Target a switch running `block_parse`.
    pub fn with_block_parse(mut self, block_parse: BlockParse) -> Self {
        self.block_parse = block_parse;
        self
    }

    /// The command carrying the dial string.
    pub fn carrier(&self) -> DialStringCarrier {
        self.carrier
    }

    /// The block-parser revision rendered for.
    pub fn block_parse(&self) -> BlockParse {
        self.block_parse
    }

    fn argument_passes(self) -> u32 {
        match self.argument {
            ArgumentPass::Blank | ArgumentPass::Char(_) => 1,
            ArgumentPass::Consumed => 0,
        }
    }

    fn passes(self, field: EscapedField) -> u32 {
        self.argument_passes()
            + match field {
                EscapedField::Value { scope, .. } | EscapedField::Key { scope, .. } => {
                    self.block_parse
                        .cleanup_passes()
                        + scope.leg_split_passes()
                }
                EscapedField::Endpoint => LEG_SPLIT_PASSES,
            }
    }

    /// Every pass halves a run of backslashes, so a literal one needs 2^passes.
    fn backslash_escape(self, field: EscapedField) -> String {
        "\\".repeat(1 << self.passes(field))
    }

    /// The last pass trims the text's edges, so an edge space must read `\s` entering it.
    fn space_escape(self, field: EscapedField) -> String {
        format!("{}s", "\\".repeat(1 << (self.passes(field) - 1)))
    }

    /// A quote must still read `\'` entering the last pass, or bare after a
    /// carrier pass that deletes `\'`.
    fn quote_escape(self, field: EscapedField) -> String {
        self.quote_bare_after(self.passes(field))
    }

    /// A quote reading bare once `passes` passes have run.
    fn quote_bare_after(self, passes: u32) -> String {
        let consumed_by_carrier = if self
            .carrier
            .expands()
        {
            2
        } else {
            1
        };
        let run = (1usize << passes) - consumed_by_carrier;
        format!("{}'", "\\".repeat(run))
    }

    /// An empty `''` reaching bare the scan `switch_ivr_originate` runs over a `[]` block after
    /// the `|` leg split, which protects a comma only when the byte before it is no backslash.
    fn channel_comma_guard(self) -> String {
        self.quote_bare_after(self.argument_passes() + 1)
            .repeat(2)
    }

    /// An empty `''` reaching the `=` split bare ahead of a key opening `^^`, which would otherwise
    /// name that split's separator, or the block's when the key is first.
    fn caret_guard(self, field: EscapedField) -> String {
        self.quote_bare_after(self.passes(field) - 1)
            .repeat(2)
    }
}

struct RenderedAt<'r, R> {
    target: DialStringTarget,
    render: &'r R,
}

impl<R> fmt::Display for RenderedAt<'_, R>
where
    R: Fn(&mut fmt::Formatter<'_>, DialStringTarget) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.render)(f, self.target)
    }
}

impl From<DialStringCarrier> for DialStringTarget {
    fn from(carrier: DialStringCarrier) -> Self {
        Self::new(carrier)
    }
}

/// Why `open` and `close` in `text` move the end the switch counts its way to.
pub(super) fn unbalanced(text: &str, (open, close): (char, char)) -> Option<String> {
    let mut depth = 0i32;
    for ch in text.chars() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth < 0 {
                return Some(format!(
                    "closes a '{open}' it never opened, ending the block early"
                ));
            }
        }
    }
    (depth != 0).then(|| format!("opens a '{open}' it never closes, swallowing the block's end"))
}

/// Reject a key no escaping carries: empty, `:_:`, a quote in channel scope or an unbalanced
/// bracket. The key is the offending text, so no error quotes it.
fn check_key(key: &str, vars_type: VariablesType) -> Result<(), OriginateError> {
    let fault = if key.is_empty() {
        Some("is empty, which the switch installs under no name".to_owned())
    } else if key.contains(ENTERPRISE_DELIM) {
        Some(format!(
            "carries the enterprise separator {ENTERPRISE_DELIM}, on which the switch splits \
             the dial string into threads whatever quoting or escaping surrounds it"
        ))
    } else if vars_type == VariablesType::Channel && key.contains('\'') {
        Some(
            "carries a single quote in channel scope, which the switch pairs with the next \
             quote in the dial string before it parses the block"
                .to_owned(),
        )
    } else {
        unbalanced(key, vars_type.delimiters())
    };
    fault.map_or(Ok(()), |fault| {
        Err(OriginateError::ParseError(format!(
            "a variable name {fault}"
        )))
    })
}

/// Reject a pair no escaping carries: a key [`check_key`] refuses, or a value carrying `:_:`, a
/// quote in channel scope, nothing or an unbalanced bracket. Each error names why the switch loses it.
fn check_representable(
    key: &str,
    value: &str,
    vars_type: VariablesType,
) -> Result<(), OriginateError> {
    check_key(key, vars_type)?;
    if value.contains(ENTERPRISE_DELIM) {
        return Err(OriginateError::ParseError(format!(
            "variable {key} carries the enterprise separator {ENTERPRISE_DELIM}, on which \
             the switch splits the dial string into threads whatever quoting or escaping \
             surrounds it"
        )));
    }
    if vars_type == VariablesType::Channel && value.contains('\'') {
        return Err(OriginateError::ParseError(format!(
            "variable {key} carries a single quote in channel scope: the switch \
             pairs it with the next quote in the dial string before it parses \
             the block, whatever escaping precedes either. Use default scope, \
             or keep the quote out of the value"
        )));
    }
    if value.is_empty() {
        return Err(OriginateError::ParseError(format!(
            "variable {key} has an empty value: the switch discards such a pair \
             without logging it, so it cannot be told from an absent variable on \
             the wire. Give it a value or remove it -- and if its presence was \
             itself the signal, that signal needs a home outside the dial string"
        )));
    }

    unbalanced(value, vars_type.delimiters()).map_or(Ok(()), |fault| {
        Err(OriginateError::ParseError(format!(
            "variable {key} {fault}"
        )))
    })
}

/// Reject a separator that cannot delimit the block it was chosen for.
///
/// Beyond what [`breaks_a_split`], either bracket moves the end the switch counts its way to,
/// `=` splits the pair instead, `^` leaves the `^^` prefix reading as its own separator, and
/// `|` in a `[]` block is read by the leg split before the block is parsed. Dialplan expansion
/// reads `$` then `{` across a pair boundary as a reference, so neither separates.
fn check_separator(sep: char, vars_type: VariablesType) -> Result<(), OriginateError> {
    let (open, close) = vars_type.delimiters();
    if breaks_a_split(sep)
        || sep == open
        || sep == close
        || matches!(sep, '=' | '^' | '$' | '{')
        || (sep == '|' && vars_type == VariablesType::Channel)
    {
        return Err(OriginateError::ParseError(format!(
            "invalid ^^ separator: '{sep}'"
        )));
    }
    Ok(())
}

/// Escape a value for the wire, escaping the comma only when `commas_separate`
/// says it is this block's separator; a `^^` block separates on something else
/// and refuses a value carrying it, so a comma there is ordinary text. A `[]`
/// block also escapes the pipe, which the leg split would otherwise read.
fn escape_value(
    value: &str,
    target: impl Into<DialStringTarget>,
    commas_separate: bool,
    vars_type: VariablesType,
) -> String {
    escape_text(
        value,
        target.into(),
        EscapedField::Value {
            scope: vars_type,
            commas_separate,
        },
    )
}

/// Escape `text` for every pass `field` meets at `target`.
pub(crate) fn escape_text(text: &str, target: DialStringTarget, field: EscapedField) -> String {
    let value = text;
    // The backslash goes first, or the ones the other rules introduce get
    // escaped in turn.
    let escaped = value
        .replace('\\', &target.backslash_escape(field))
        .replace('\'', &target.quote_escape(field));
    let dollars = protects_dollars(value, target);
    let escaped = if dollars {
        escaped.replace('$', "\\$")
    } else {
        escaped
    };
    // `\,` and `\|` keep one backslash: a pass consumes `\x` only before a
    // quote, a backslash, a named escape or that pass's own delimiter.
    let escaped = if field.escapes_comma() {
        escaped.replace(',', "\\,")
    } else {
        escaped
    };
    let escaped = if field.escapes_pipe() {
        escaped.replace('|', "\\|")
    } else {
        escaped
    };
    let escaped = match field {
        EscapedField::Key { .. } => escaped.replace('=', "\\="),
        EscapedField::Value { .. } | EscapedField::Endpoint => escaped,
    };
    let escaped = match field {
        EscapedField::Value {
            scope: VariablesType::Channel,
            commas_separate,
        } => guard_channel_commas(
            escaped,
            commas_separate && value.ends_with('\\'),
            target,
            commas_separate,
        ),
        EscapedField::Key {
            scope: VariablesType::Channel,
            commas_separate,
        } => guard_channel_commas(escaped, false, target, commas_separate),
        EscapedField::Value { .. } | EscapedField::Key { .. } | EscapedField::Endpoint => escaped,
    };
    let space = target.space_escape(field);
    let escaped = match escaped.strip_prefix(' ') {
        Some(rest) => format!("{space}{rest}"),
        None => escaped,
    };
    let escaped = match escaped.strip_suffix(' ') {
        Some(rest) => format!("{rest}{space}"),
        None => escaped,
    };
    let escaped = match field {
        EscapedField::Key { .. } if value.starts_with("^^") => {
            format!("{}{escaped}", target.caret_guard(field))
        }
        EscapedField::Value { .. } | EscapedField::Key { .. } | EscapedField::Endpoint => escaped,
    };
    let escaped = if dollars {
        format!("\\'{escaped}")
    } else {
        escaped
    };
    if escaped.contains(' ') {
        format!("'{}'", escaped)
    } else {
        escaped
    }
}

/// Put the guard between a backslash and the comma after it in a `[]` field: the separator after
/// a value ending in one (`separator_follows`), or a literal comma in a `^^` block. Channel scope
/// refuses a quote.
fn guard_channel_commas(
    escaped: String,
    separator_follows: bool,
    target: DialStringTarget,
    commas_separate: bool,
) -> String {
    let guard = target.channel_comma_guard();
    if !commas_separate {
        let backslash = target.backslash_escape(EscapedField::Value {
            scope: VariablesType::Channel,
            commas_separate,
        });
        escaped.replace(&format!("{backslash},"), &format!("{backslash}{guard},"))
    } else if separator_follows {
        escaped + &guard
    } else {
        escaped
    }
}

/// Expansion drops the first `$` of a `$$` opening no reference and substitutes a reference.
/// `\$` keeps it only while expansion runs, which a leading `\'` guarantees and expansion then
/// deletes. Text naming a variable is left to the switch, whichever field carries it.
fn protects_dollars(value: &str, target: DialStringTarget) -> bool {
    target
        .carrier()
        .expands()
        && value.contains("$$")
        && !names_a_variable(value)
}

/// Inverts [`escape_value`].
fn unescape_value(
    value: &str,
    target: DialStringTarget,
    commas_separate: bool,
    vars_type: VariablesType,
) -> String {
    unescape_field(
        value,
        target,
        EscapedField::Value {
            scope: vars_type,
            commas_separate,
        },
    )
}

/// Inverts [`escape_text`] for a key or value, undoing each substitution in the reverse order it
/// was applied so an escape introduced by a later rule is not read as input to an earlier one.
fn unescape_field(value: &str, target: DialStringTarget, field: EscapedField) -> String {
    let (vars_type, commas_separate, key) = match field {
        EscapedField::Value {
            scope,
            commas_separate,
        } => (scope, commas_separate, false),
        EscapedField::Key {
            scope,
            commas_separate,
        } => (scope, commas_separate, true),
        EscapedField::Endpoint => (VariablesType::Default, true, false),
    };
    let s = value
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(value);
    // Every other escape at this carrier opens with at least two backslashes.
    let (dollars, s) = match s.strip_prefix("\\'") {
        Some(rest)
            if target
                .carrier()
                .expands() =>
        {
            (true, rest)
        }
        _ => (false, s),
    };
    let caret_guard = target.caret_guard(field);
    let s = match s.strip_prefix(caret_guard.as_str()) {
        Some(rest) if key && rest.starts_with("^^") => rest,
        _ => s,
    };
    let space = target.space_escape(field);
    let (lead, s) = match s.strip_prefix(space.as_str()) {
        Some(rest) => (" ", rest),
        None => ("", s),
    };
    // A literal backslash before a final `s` also ends in the escape's run, but whole levels deep.
    let run = s
        .strip_suffix('s')
        .map_or(0, |body| {
            body.len()
                - body
                    .trim_end_matches('\\')
                    .len()
        });
    let (trail, s) = if run % (1 << target.passes(field)) == space.len() - 1 {
        (" ", &s[..s.len() - space.len()])
    } else {
        ("", s)
    };
    let guard = target.channel_comma_guard();
    let s: Cow<'_, str> = match (vars_type, commas_separate) {
        (VariablesType::Channel, true) if !key => Cow::Borrowed(
            s.strip_suffix(guard.as_str())
                .unwrap_or(s),
        ),
        (VariablesType::Channel, false) => Cow::Owned(s.replace(&format!("{guard},"), ",")),
        _ => Cow::Borrowed(s),
    };

    let s = if vars_type == VariablesType::Channel {
        s.replace("\\|", "|")
    } else {
        s.to_string()
    };
    let s = if key { s.replace("\\=", "=") } else { s };
    let s = if commas_separate {
        s.replace("\\,", ",")
    } else {
        s
    };
    let s = if dollars { s.replace("\\$", "$") } else { s };
    let s = s
        .replace(&target.quote_escape(field), "'")
        .replace(&target.backslash_escape(field), "\\");
    format!("{lead}{s}{trail}")
}

impl Variables {
    /// Create an empty variable set with the given scope.
    pub fn new(vars_type: VariablesType) -> Self {
        Self {
            vars_type,
            inner: IndexMap::new(),
            separator: None,
        }
    }

    /// Create from an existing set of key-value pairs.
    pub fn with_vars(
        vars_type: VariablesType,
        vars: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            vars_type,
            inner: vars
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            separator: None,
        }
    }

    /// Separate the pairs with `sep` instead of a comma, emitting the block in
    /// FreeSWITCH's `^^<sep>` form.
    ///
    /// A value expanded from `${...}` in a dialplan is substituted before the
    /// block is parsed, so no escaping can be inserted into it; choosing a
    /// separator none of the values contain is the only way such a value can
    /// carry a comma. The separator is given rather than derived, so a block
    /// renders the same way whatever its values happen to be that call.
    ///
    /// Fails if `sep` cannot delimit this block, or if a key or value already
    /// present contains it. A pair inserted afterwards is not checked: one carrying
    /// `sep` splits into a pair nobody wrote, silently, until
    /// [`insert`](Self::insert) becomes fallible (`docs/next-major.md`).
    ///
    /// Refused: space, controls, non-ASCII, `\`, `'` and lowercase `n r t s`, which break the
    /// switch's split or its escapes; either of the block's brackets, `=` and `^`; `$` and `{`,
    /// which dialplan expansion reads as a reference across a pair boundary; `|` in a `[]` block.
    pub fn with_separator(mut self, sep: char) -> Result<Self, OriginateError> {
        check_separator(sep, self.vars_type)?;
        if self
            .inner
            .keys()
            .any(|k| k.contains(sep))
        {
            return Err(OriginateError::ParseError(format!(
                "a variable name contains the chosen '{sep}' separator"
            )));
        }
        if let Some((key, _)) = self
            .inner
            .iter()
            .find(|(_, v)| v.contains(sep))
        {
            return Err(OriginateError::ParseError(format!(
                "variable {key} contains the chosen '{sep}' separator"
            )));
        }
        self.separator = Some(sep);
        Ok(self)
    }

    /// The `^^` separator this block renders with, if one was chosen.
    pub fn separator(&self) -> Option<char> {
        self.separator
    }

    /// Insert or overwrite a variable.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner
            .insert(key.into(), value.into());
    }

    /// Remove a variable by name, returning its value if it existed.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.inner
            .shift_remove(key)
    }

    /// Look up a variable by name.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner
            .get(key)
            .map(|s| s.as_str())
    }

    /// Whether the set contains no variables.
    pub fn is_empty(&self) -> bool {
        self.inner
            .is_empty()
    }

    /// Number of variables.
    pub fn len(&self) -> usize {
        self.inner
            .len()
    }

    /// Variable scope (Enterprise, Default, or Channel).
    pub fn scope(&self) -> VariablesType {
        self.vars_type
    }

    /// Change the variable scope.
    pub fn set_scope(&mut self, scope: VariablesType) {
        self.vars_type = scope;
    }

    /// Iterate over key-value pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Mutable iterator over key-value pairs in insertion order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut String)> {
        self.inner
            .iter_mut()
            .map(|(k, v)| (k.as_str(), v))
    }

    /// Mutable iterator over values in insertion order.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.inner
            .values_mut()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Variables {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.vars_type == VariablesType::Default
            && self
                .separator
                .is_none()
        {
            self.inner
                .serialize(serializer)
        } else {
            use serde::ser::SerializeStruct;
            let fields = 2 + usize::from(
                self.separator
                    .is_some(),
            );
            let mut s = serializer.serialize_struct("Variables", fields)?;
            s.serialize_field("scope", &self.vars_type)?;
            s.serialize_field("vars", &self.inner)?;
            if let Some(sep) = self.separator {
                s.serialize_field("separator", &sep)?;
            }
            s.end()
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Variables {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum VariablesRepr {
            Scoped {
                scope: VariablesType,
                vars: IndexMap<String, String>,
                #[serde(default)]
                separator: Option<char>,
            },
            Flat(IndexMap<String, String>),
        }

        let (vars_type, inner, separator) = match VariablesRepr::deserialize(deserializer)? {
            VariablesRepr::Scoped {
                scope,
                vars,
                separator,
            } => (scope, vars, separator),
            VariablesRepr::Flat(map) => (VariablesType::Default, map, None),
        };
        // A config naming a value the wire cannot carry fails at load rather
        // than on the call it was loaded for.
        for (key, value) in &inner {
            check_representable(key, value, vars_type).map_err(serde::de::Error::custom)?;
        }
        let vars = Self {
            vars_type,
            inner,
            separator: None,
        };
        match separator {
            Some(sep) => vars
                .with_separator(sep)
                .map_err(serde::de::Error::custom),
            None => Ok(vars),
        }
    }
}

/// Renders a [`Variables`] for one target. Returned by
/// [`Variables::display_for`].
#[derive(Debug, Clone, Copy)]
pub struct VariablesDisplay<'a> {
    vars: &'a Variables,
    target: DialStringTarget,
}

impl fmt::Display for VariablesDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.target
            .write_argument(f, |f, target| {
                self.vars
                    .write_for(f, target)
            })
    }
}

impl Variables {
    /// Render for a named carrier or [`DialStringTarget`], rather than the
    /// [`DialStringCarrier::EslApi`] default that [`Display`](fmt::Display) uses.
    pub fn display_for(&self, target: impl Into<DialStringTarget>) -> VariablesDisplay<'_> {
        VariablesDisplay {
            vars: self,
            target: target.into(),
        }
    }

    pub(super) fn write_for(
        &self,
        f: &mut fmt::Formatter<'_>,
        target: DialStringTarget,
    ) -> fmt::Result {
        let (open, close) = self
            .vars_type
            .delimiters();
        f.write_fmt(format_args!("{}", open))?;
        if let Some(sep) = self.separator {
            write!(f, "^^{sep}")?;
        }
        // A chosen separator carries the values that a comma would have needed
        // escaping for, so only the comma form escapes them.
        let commas_separate = self
            .separator
            .is_none();
        let sep = self
            .separator
            .unwrap_or(',');
        for (i, (key, value)) in self
            .inner
            .iter()
            .enumerate()
        {
            if i > 0 {
                write!(f, "{sep}")?;
            }
            let key = escape_text(
                key,
                target,
                EscapedField::Key {
                    scope: self.vars_type,
                    commas_separate,
                },
            );
            let value = escape_value(value, target, commas_separate, self.vars_type);
            write!(f, "{}={}", key, value)?;
        }
        f.write_fmt(format_args!("{}", close))
    }
}

impl fmt::Display for Variables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_for(f, DialStringCarrier::EslApi.into())
    }
}

impl FromStr for Variables {
    type Err = OriginateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_for(s, DialStringCarrier::EslApi)
    }
}

impl Variables {
    /// Parse a block written for a named carrier or [`DialStringTarget`],
    /// mirroring [`display_for`](Self::display_for). [`FromStr`] uses the same
    /// [`DialStringCarrier::EslApi`] default as [`Display`](fmt::Display), so
    /// the two round-trip.
    pub fn parse_for(s: &str, target: impl Into<DialStringTarget>) -> Result<Self, OriginateError> {
        let (argument, target) = target
            .into()
            .read_argument(s)?;
        let s = argument.trim_matches(' ');
        if s.len() < 2 {
            return Err(OriginateError::ParseError(
                "variable block too short".into(),
            ));
        }

        let (vars_type, inner_str) = match (s.as_bytes()[0], s.as_bytes()[s.len() - 1]) {
            (b'{', b'}') => (VariablesType::Default, &s[1..s.len() - 1]),
            (b'<', b'>') => (VariablesType::Enterprise, &s[1..s.len() - 1]),
            (b'[', b']') => (VariablesType::Channel, &s[1..s.len() - 1]),
            (open, close) => {
                return Err(OriginateError::ParseError(format!(
                    "unknown variable delimiters: {:?}..{:?}",
                    open as char, close as char
                )));
            }
        };

        let (sep, var_str) = match inner_str.strip_prefix("^^") {
            Some(rest) => {
                let sep = rest
                    .chars()
                    .next()
                    .ok_or_else(|| {
                        OriginateError::ParseError("^^ without separator character".into())
                    })?;
                check_separator(sep, vars_type)?;
                (sep, &rest[sep.len_utf8()..])
            }
            None => (',', inner_str),
        };
        let commas_separate = sep == ',';

        let mut inner = IndexMap::new();
        if !var_str.is_empty() {
            for (i, part) in split_unescaped(var_str, sep)
                .into_iter()
                .enumerate()
            {
                let at = unescaped_at(part, '=')
                    .next()
                    .ok_or_else(|| {
                        OriginateError::ParseError(format!("missing = in variable {i}"))
                    })?;
                let key = unescape_field(
                    &part[..at],
                    target,
                    EscapedField::Key {
                        scope: vars_type,
                        commas_separate,
                    },
                );
                let key = key.as_str();
                let value = unescape_value(&part[at + 1..], target, commas_separate, vars_type);
                check_representable(key, &value, vars_type)?;
                if !commas_separate && (key.contains(sep) || value.contains(sep)) {
                    return Err(OriginateError::ParseError(format!(
                        "variable {i} contains the block's ^^ separator"
                    )));
                }
                inner.insert(key.to_string(), value);
            }
        }

        Ok(Self {
            vars_type,
            inner,
            separator: None,
        })
    }
}

/// Split on separators that are not escaped by a backslash.
///
/// A separator preceded by an odd number of backslashes is escaped (e.g. `\,`).
/// One preceded by an even number is a real split point (e.g. `\\,` is an
/// escaped backslash followed by the delimiter).
fn split_unescaped(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for i in unescaped_at(s, sep) {
        parts.push(&s[start..i]);
        start = i + sep.len_utf8();
    }
    parts.push(&s[start..]);
    parts
}

/// The byte offset of every `sep` behind an even run of backslashes.
fn unescaped_at(s: &str, sep: char) -> impl Iterator<Item = usize> + '_ {
    s.char_indices()
        .filter(move |&(i, ch)| {
            ch == sep
                && s.as_bytes()[..i]
                    .iter()
                    .rev()
                    .take_while(|&&b| b == b'\\')
                    .count()
                    % 2
                    == 0
        })
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measured on a live switch: an empty value is discarded under every
    /// encoding, silently, so nothing can be emitted for one and the refusal
    /// has to name a remedy the caller could not guess.
    #[test]
    fn an_empty_value_is_refused_at_the_boundary() {
        let err = "{k=,after=sentinel}"
            .parse::<Variables>()
            .unwrap_err()
            .to_string();
        assert!(err.contains('k'), "error does not name the variable: {err}");
        assert!(
            err.contains("remove it"),
            "error does not name a remedy: {err}"
        );
        // Prescribing removal alone is wrong for a caller whose *presence* test
        // is the signal: dropping the key silently reverses that decision.
        assert!(
            err.contains("presence"),
            "error prescribes a fix without allowing that presence meant something: {err}"
        );
    }

    /// The switch finds a block's end by counting bracket depth and honours no
    /// escape while doing so, so a value closing a bracket it never opened ends
    /// the block early and the rest becomes dial-string text.
    #[test]
    fn an_unbalanced_bracket_is_refused_while_a_balanced_one_is_not() {
        assert!("{k=oops}extra}"
            .parse::<Variables>()
            .is_err());

        let balanced: Variables = "{k=${some_var}}"
            .parse()
            .expect("a balanced ${...} is ordinary and must parse");
        assert_eq!(balanced.get("k"), Some("${some_var}"));
    }

    /// A chosen separator is what carries a comma through a value that was
    /// expanded before the block was parsed, so the comma must survive as
    /// ordinary text rather than picking up an escape.
    #[test]
    fn chosen_separator_leaves_commas_alone() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("codecs", "PCMA,PCMU,G729");
        let vars = vars
            .with_separator(':')
            .unwrap();

        assert_eq!(vars.to_string(), "{^^:codecs=PCMA,PCMU,G729}");
        assert_eq!(vars.separator(), Some(':'));
    }

    #[test]
    fn separator_that_cannot_delimit_the_block_is_refused() {
        let vars = Variables::new(VariablesType::Channel);
        for sep in ['[', ']', '=', '^', 'é', '§'] {
            assert!(
                vars.clone()
                    .with_separator(sep)
                    .is_err(),
                "accepted {sep:?}"
            );
        }
    }

    /// `separate_string_char_delim` skips the byte after a backslash, a quote pairs with the next
    /// one, a space or control is trimmed or cuts the argument, `n r t s` name escapes, and dialplan
    /// expansion reads `$` then `{` across a pair boundary as a reference.
    #[test]
    fn a_separator_breaking_the_switch_split_is_refused_everywhere() {
        for sep in [
            '\\', '\'', ' ', '\t', '\n', '\u{b}', '\0', '\u{7f}', 'n', 'r', 't', 's', '$', '{',
        ] {
            for scope in [
                VariablesType::Default,
                VariablesType::Enterprise,
                VariablesType::Channel,
            ] {
                let mut vars = Variables::new(scope);
                vars.insert("a", "1");
                assert!(
                    vars.with_separator(sep)
                        .is_err(),
                    "builder accepted {sep:?} in {scope:?}"
                );
            }
            assert!(
                Variables::parse_for(
                    &format!("{{^^{sep}a=1{sep}b=2}}"),
                    DialStringCarrier::Dialplan
                )
                .is_err(),
                "parser accepted {sep:?}"
            );
        }
        for sep in ['~', ';', '!', '#', 'N', '0', '"', ','] {
            let mut vars = Variables::new(VariablesType::Default);
            vars.insert("a", "1");
            assert!(
                vars.with_separator(sep)
                    .is_ok(),
                "refused {sep:?}"
            );
        }
    }

    /// The parser has to refuse what the builder refuses, `^` included:
    /// accepting a block no render of this crate can reproduce hands the caller
    /// a value that changes when it is written back out.
    #[test]
    fn the_parser_refuses_every_separator_the_builder_does() {
        for sep in ['[', ']', '=', '^', 'é', '§'] {
            assert!(
                format!("[^^{sep}a=1{sep}b=2]")
                    .parse::<Variables>()
                    .is_err(),
                "parser accepted {sep:?}"
            );
        }
    }

    /// Refusing here is the whole point: a value carrying the separator would
    /// split into a pair that was never written, and the switch reports nothing.
    #[test]
    fn separator_already_present_in_a_value_is_refused() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("uri", "sip:bob@example.com");
        let err = vars
            .with_separator(':')
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("uri"),
            "error does not name the variable: {err}"
        );
    }

    /// Measured against a live switch, with two awkward values in one block —
    /// a block carrying only one is more forgiving and hides the failure.
    /// Sweeping the backslash count, each carrier succeeds at counts the other
    /// fails, so these forms are the wire contract and not a preference.
    #[test]
    fn escaping_pins_the_measured_wire_forms() {
        let cases = [
            (DialStringCarrier::Dialplan, "it's", r"it\\\\\\'s"),
            (DialStringCarrier::EslApi, "it's", r"it\\\\\\\'s"),
            (DialStringCarrier::Dialplan, "l'a'b", r"l\\\\\\'a\\\\\\'b"),
            (DialStringCarrier::EslApi, "l'a'b", r"l\\\\\\\'a\\\\\\\'b"),
            (DialStringCarrier::Dialplan, r"a\nb", r"a\\\\\\\\nb"),
            (DialStringCarrier::EslApi, r"a\nb", r"a\\\\\\\\nb"),
            (DialStringCarrier::Dialplan, "a,b", r"a\,b"),
            (DialStringCarrier::EslApi, "a,b", r"a\,b"),
            (DialStringCarrier::Dialplan, "a|b", "a|b"),
            (DialStringCarrier::EslApi, "a|b", "a|b"),
            (DialStringCarrier::Dialplan, "pa$$word", r"\'pa\$\$word"),
            (DialStringCarrier::Dialplan, "a$b", "a$b"),
            (DialStringCarrier::EslApi, "pa$$word", "pa$$word"),
        ];
        for (carrier, value, want) in cases {
            assert_eq!(
                escape_value(value, carrier, true, VariablesType::Default),
                want,
                "{value:?} for {carrier:?}"
            );
        }
    }

    /// A `[]` block rides through the leg splits before it is parsed: two more
    /// backslash-consuming passes, and a pipe the first of them would read.
    #[test]
    fn channel_scope_escapes_for_the_leg_splits() {
        let cases = [
            (
                DialStringCarrier::Dialplan,
                r"a\nb",
                r"a\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\nb",
            ),
            (
                DialStringCarrier::EslApi,
                r"a\nb",
                r"a\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\nb",
            ),
            (DialStringCarrier::Dialplan, "a|b", r"a\|b"),
            (DialStringCarrier::EslApi, "a|b", r"a\|b"),
            (DialStringCarrier::EslApi, "a,b", r"a\,b"),
        ];
        for (carrier, value, want) in cases {
            assert_eq!(
                escape_value(value, carrier, true, VariablesType::Channel),
                want,
                "{value:?} for {carrier:?}"
            );
        }
    }

    /// A `^^` block reaches the switch's tokenizer as often as the comma form,
    /// so every rule above still holds there; only the comma stops being the
    /// separator and so stops being escaped.
    #[test]
    fn a_chosen_separator_changes_only_the_comma() {
        let cases = [
            (DialStringCarrier::Dialplan, "it's", r"it\\\\\\'s"),
            (DialStringCarrier::EslApi, "it's", r"it\\\\\\\'s"),
            (DialStringCarrier::Dialplan, "l'a'b", r"l\\\\\\'a\\\\\\'b"),
            (DialStringCarrier::EslApi, "l'a'b", r"l\\\\\\\'a\\\\\\\'b"),
            (DialStringCarrier::Dialplan, r"a\nb", r"a\\\\\\\\nb"),
            (DialStringCarrier::EslApi, r"a\nb", r"a\\\\\\\\nb"),
            (DialStringCarrier::Dialplan, "a,b", "a,b"),
            (DialStringCarrier::EslApi, "a,b", "a,b"),
        ];
        for (carrier, value, want) in cases {
            assert_eq!(
                escape_value(value, carrier, false, VariablesType::Default),
                want,
                "{value:?} for {carrier:?}"
            );
        }
    }

    /// The crate exists to drive `api originate`, so a block rendered with no
    /// carrier named is rendered for that one.
    #[test]
    fn display_defaults_to_the_api_carrier() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "it's");
        assert_eq!(vars.to_string(), r"{k=it\\\\\\\'s}");
        assert_eq!(
            vars.display_for(DialStringCarrier::Dialplan)
                .to_string(),
            r"{k=it\\\\\\'s}"
        );
    }

    #[test]
    fn round_trips_at_either_carrier() {
        for carrier in [DialStringCarrier::EslApi, DialStringCarrier::Dialplan] {
            for value in ["it's", r"a\,b", r"C:\path", "a,b", "with space"] {
                let mut vars = Variables::new(VariablesType::Default);
                vars.insert("k", value);
                vars.insert("after", "sentinel");
                let rendered = vars
                    .display_for(carrier)
                    .to_string();

                let back = Variables::parse_for(&rendered, carrier).unwrap_or_else(|e| {
                    panic!("{value:?} for {carrier:?} rendered {rendered}: {e}")
                });
                assert_eq!(back.get("k"), Some(value), "rendered {rendered}");
                assert_eq!(back.get("after"), Some("sentinel"), "rendered {rendered}");
            }
        }
    }

    /// A chosen separator changes which character needs escaping, not whether
    /// the block is escape-processed: the switch runs the same tokenizer over
    /// it, so a value still comes back through the same undoing.
    #[test]
    fn separated_block_round_trips_at_either_carrier() {
        for carrier in [DialStringCarrier::EslApi, DialStringCarrier::Dialplan] {
            for value in ["it's", "a,b", r"C:\path", r"a\nb", "with space"] {
                let mut vars = Variables::new(VariablesType::Default);
                vars.insert("k", value);
                vars.insert("after", "sentinel");
                let vars = vars
                    .with_separator('~')
                    .expect("'~' appears in none of these values");
                let rendered = vars
                    .display_for(carrier)
                    .to_string();

                let back = Variables::parse_for(&rendered, carrier).unwrap_or_else(|e| {
                    panic!("{value:?} for {carrier:?} rendered {rendered}: {e}")
                });
                assert_eq!(back.get("k"), Some(value), "rendered {rendered}");
                assert_eq!(back.get("after"), Some("sentinel"), "rendered {rendered}");
            }
        }
    }

    /// `split_unescaped_commas` reads a comma behind an even number of
    /// backslashes as a real separator, so a value ending in a backslash has to
    /// be written with its own backslash escaped or the writer contradicts the
    /// reader and the block no longer parses.
    #[test]
    fn value_with_backslash_round_trips() {
        for value in [r"a\,b", r"C:\path", r"trailing\", r"\\", r"a\nb"] {
            let mut vars = Variables::new(VariablesType::Default);
            vars.insert("k", value);
            vars.insert("after", "sentinel");
            let rendered = vars.to_string();

            let back: Variables = rendered
                .parse()
                .unwrap_or_else(|e| {
                    panic!("{value:?} rendered {rendered} and failed to parse: {e}")
                });
            assert_eq!(back.get("k"), Some(value), "rendered {rendered}");
            assert_eq!(
                back.get("after"),
                Some("sentinel"),
                "value {value:?} ate the next variable: {rendered}"
            );
        }
    }

    /// A variable block holds dialled numbers and passthrough header values, so a
    /// malformed part is reported by position.
    #[test]
    fn missing_equals_error_omits_the_fragment() {
        let msg = "{origination_caller_id_number=15551234567,15550009999}"
            .parse::<Variables>()
            .unwrap_err()
            .to_string();
        assert!(
            !msg.contains("15550009999"),
            "error quoted its input: {msg}"
        );
        assert!(
            msg.contains("variable 1"),
            "error does not name the part: {msg}"
        );
    }

    #[test]
    fn unknown_delimiters_error_omits_the_block() {
        let msg = "(origination_caller_id_number=15551234567)"
            .parse::<Variables>()
            .unwrap_err()
            .to_string();
        assert!(
            !msg.contains("15551234567"),
            "error quoted its input: {msg}"
        );
    }

    #[test]
    fn variables_standard_chars() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "this_value");
        vars.insert("second", "2");
        assert_eq!(vars.to_string(), "{test_key=this_value,second=2}");
    }

    #[test]
    fn variables_comma_escaped() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "this,is,a,value");
        let result = vars.to_string();
        assert!(result.contains("\\,"));
    }

    #[test]
    fn variables_spaces_quoted() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "this is a value");
        let result = vars.to_string();
        assert_eq!(
            result
                .matches('\'')
                .count(),
            2
        );
    }

    #[test]
    fn variables_single_quote_escaped() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("test_key", "let's_this_be_a_value");
        let result = vars.to_string();
        assert!(result.contains("\\'"));
    }

    #[test]
    fn variables_enterprise_delimiters() {
        let mut vars = Variables::new(VariablesType::Enterprise);
        vars.insert("k", "v");
        let result = vars.to_string();
        assert!(result.starts_with('<'));
        assert!(result.ends_with('>'));
    }

    #[test]
    fn variables_channel_delimiters() {
        let mut vars = Variables::new(VariablesType::Channel);
        vars.insert("k", "v");
        let result = vars.to_string();
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn variables_default_delimiters() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "v");
        let result = vars.to_string();
        assert!(result.starts_with('{'));
        assert!(result.ends_with('}'));
    }

    #[test]
    fn variables_parse_round_trip() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("origination_caller_id_number", "9005551212");
        vars.insert("sip_h_Call-Info", "<url>;meta=123,<uri>");
        let s = vars.to_string();
        let parsed: Variables = s
            .parse()
            .unwrap();
        assert_eq!(
            parsed.get("origination_caller_id_number"),
            Some("9005551212")
        );
        assert_eq!(parsed.get("sip_h_Call-Info"), Some("<url>;meta=123,<uri>"));
    }

    #[test]
    fn split_unescaped_basic() {
        assert_eq!(split_unescaped("a,b,c", ','), vec!["a", "b", "c"]);
        assert_eq!(split_unescaped("a~b~c", '~'), vec!["a", "b", "c"]);
    }

    #[test]
    fn split_unescaped_escaped() {
        assert_eq!(split_unescaped(r"a\,b,c", ','), vec![r"a\,b", "c"]);
    }

    #[test]
    fn split_unescaped_double_backslash() {
        // \\, = escaped backslash + comma delimiter
        assert_eq!(split_unescaped(r"a\\,b", ','), vec![r"a\\", "b"]);
    }

    #[test]
    fn split_unescaped_triple_backslash() {
        // \\\, = escaped backslash + escaped comma (no split)
        assert_eq!(split_unescaped(r"a\\\,b", ','), vec![r"a\\\,b"]);
    }

    #[test]
    fn variables_caret_caret_separator() {
        let vars: Variables =
            "[^^:sip_invite_domain=pbx.example.com:presence_id=1211@pbx.example.com]"
                .parse()
                .unwrap();
        assert_eq!(vars.scope(), VariablesType::Channel);
        assert_eq!(vars.get("sip_invite_domain"), Some("pbx.example.com"));
        assert_eq!(vars.get("presence_id"), Some("1211@pbx.example.com"));
    }

    #[test]
    fn variables_caret_caret_display_uses_canonical_comma() {
        let vars: Variables = "[^^:a=1:b=2]"
            .parse()
            .unwrap();
        assert_eq!(vars.to_string(), "[a=1,b=2]");
    }

    #[test]
    fn variables_caret_caret_default_scope() {
        let vars: Variables = "{^^|x=1|y=2}"
            .parse()
            .unwrap();
        assert_eq!(vars.scope(), VariablesType::Default);
        assert_eq!(vars.get("x"), Some("1"));
        assert_eq!(vars.get("y"), Some("2"));
    }

    #[test]
    fn variables_caret_caret_enterprise_scope() {
        let vars: Variables = "<^^;a=1;b=2>"
            .parse()
            .unwrap();
        assert_eq!(vars.scope(), VariablesType::Enterprise);
        assert_eq!(vars.get("a"), Some("1"));
    }

    /// The switch consumes a backslash only before a quote, another backslash,
    /// the separator in force, or a character it has an escape for -- so in a
    /// block separated on something else, `\,` is two literal characters.
    #[test]
    fn variables_caret_caret_keeps_an_escaped_comma_literal() {
        let vars: Variables = r"[^^:key=val\,ue:other=x]"
            .parse()
            .unwrap();
        assert_eq!(vars.get("key"), Some(r"val\,ue"));
    }

    #[test]
    fn variables_caret_caret_values_with_commas() {
        let vars: Variables = "{^^|sip_h_X-Call-Info=<urn:foo>;purpose=bar,<urn:baz>|other=val}"
            .parse()
            .unwrap();
        assert_eq!(
            vars.get("sip_h_X-Call-Info"),
            Some("<urn:foo>;purpose=bar,<urn:baz>")
        );
        assert_eq!(vars.get("other"), Some("val"));
    }

    #[test]
    fn variables_caret_caret_empty_vars() {
        let vars: Variables = "[^^:]"
            .parse()
            .unwrap();
        assert!(vars.is_empty());
        assert_eq!(vars.scope(), VariablesType::Channel);
    }

    #[test]
    fn variables_caret_caret_missing_separator() {
        assert!("[^^]"
            .parse::<Variables>()
            .is_err());
    }

    #[test]
    fn variables_caret_caret_closing_bracket_as_sep() {
        assert!("[^^]]"
            .parse::<Variables>()
            .is_err());
    }

    #[test]
    fn variables_caret_caret_equals_as_sep() {
        assert!("[^^=a=1]"
            .parse::<Variables>()
            .is_err());
    }

    #[test]
    fn variables_from_str_empty_block() {
        let result = "{}".parse::<Variables>();
        assert!(
            result.is_ok(),
            "empty variable block should parse successfully"
        );
        let vars = result.unwrap();
        assert!(
            vars.is_empty(),
            "parsed empty block should have no variables"
        );
    }

    #[test]
    fn variables_from_str_empty_channel_block() {
        let result = "[]".parse::<Variables>();
        assert!(result.is_ok());
        let vars = result.unwrap();
        assert!(vars.is_empty());
        assert_eq!(vars.scope(), VariablesType::Channel);
    }

    #[test]
    fn variables_from_str_empty_enterprise_block() {
        let result = "<>".parse::<Variables>();
        assert!(result.is_ok());
        let vars = result.unwrap();
        assert!(vars.is_empty());
        assert_eq!(vars.scope(), VariablesType::Enterprise);
    }

    #[test]
    fn serde_variables_type() {
        let json = serde_json::to_string(&VariablesType::Enterprise).unwrap();
        assert_eq!(json, "\"enterprise\"");
        let parsed: VariablesType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, VariablesType::Enterprise);
    }

    #[test]
    fn serde_variables_flat_default() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("key1", "val1");
        vars.insert("key2", "val2");
        let json = serde_json::to_string(&vars).unwrap();
        let parsed: Variables = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.scope(), VariablesType::Default);
        assert_eq!(parsed.get("key1"), Some("val1"));
        assert_eq!(parsed.get("key2"), Some("val2"));
    }

    #[test]
    fn serde_variables_scoped_enterprise() {
        let mut vars = Variables::new(VariablesType::Enterprise);
        vars.insert("key1", "val1");
        let json = serde_json::to_string(&vars).unwrap();
        assert!(json.contains("\"enterprise\""));
        let parsed: Variables = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.scope(), VariablesType::Enterprise);
        assert_eq!(parsed.get("key1"), Some("val1"));
    }

    #[test]
    fn serde_variables_flat_map_deserializes_as_default() {
        let json = r#"{"key1":"val1","key2":"val2"}"#;
        let vars: Variables = serde_json::from_str(json).unwrap();
        assert_eq!(vars.scope(), VariablesType::Default);
        assert_eq!(vars.get("key1"), Some("val1"));
        assert_eq!(vars.get("key2"), Some("val2"));
    }

    #[test]
    fn serde_variables_scoped_deserializes() {
        let json = r#"{"scope":"channel","vars":{"k":"v"}}"#;
        let vars: Variables = serde_json::from_str(json).unwrap();
        assert_eq!(vars.scope(), VariablesType::Channel);
        assert_eq!(vars.get("k"), Some("v"));
    }

    /// A block whose separator is dropped by a round trip renders as a comma
    /// block, which splits the very value the separator was chosen to carry.
    #[test]
    fn serde_variables_separator_round_trips() {
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("codecs", "PCMA,PCMU");
        let vars = vars
            .with_separator('|')
            .unwrap();

        let json = serde_json::to_string(&vars).unwrap();
        let parsed: Variables = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.separator(), Some('|'));
        assert_eq!(parsed, vars);
        assert_eq!(parsed.to_string(), "{^^|codecs=PCMA,PCMU}");
    }

    /// The default separator has no key, so a config written before the key
    /// existed serializes back byte-identical.
    #[test]
    fn serde_variables_default_separator_is_absent() {
        let mut flat = Variables::new(VariablesType::Default);
        flat.insert("k", "v");
        assert_eq!(serde_json::to_string(&flat).unwrap(), r#"{"k":"v"}"#);

        let mut scoped = Variables::new(VariablesType::Channel);
        scoped.insert("k", "v");
        assert_eq!(
            serde_json::to_string(&scoped).unwrap(),
            r#"{"scope":"channel","vars":{"k":"v"}}"#
        );
    }

    #[test]
    fn serde_variables_separator_deserializes_from_config() {
        let json = r#"{"scope":"channel","vars":{"codecs":"PCMA,PCMU"},"separator":":"}"#;
        let vars: Variables = serde_json::from_str(json).unwrap();
        assert_eq!(vars.scope(), VariablesType::Channel);
        assert_eq!(vars.separator(), Some(':'));
        assert_eq!(vars.to_string(), "[^^:codecs=PCMA,PCMU]");
    }

    /// The leg split reads a `|` before a `[]` block is parsed, so it can
    /// separate nothing there; in `{}` it is ordinary.
    #[test]
    fn pipe_separator_is_refused_in_channel_scope_only() {
        let mut vars = Variables::new(VariablesType::Channel);
        vars.insert("k", "v");
        assert!(vars
            .clone()
            .with_separator('|')
            .is_err());
        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", "v");
        assert!(vars
            .with_separator('|')
            .is_ok());
    }

    /// The builder's two refusals have to hold at the config boundary too, or a
    /// YAML file produces a block the same crate would not build.
    #[test]
    fn serde_variables_unusable_separator_is_refused() {
        let undelimitable = r#"{"scope":"default","vars":{"k":"v"},"separator":"="}"#;
        assert!(serde_json::from_str::<Variables>(undelimitable).is_err());

        let in_a_value =
            r#"{"scope":"default","vars":{"uri":"sip:bob@example.com"},"separator":":"}"#;
        let err = serde_json::from_str::<Variables>(in_a_value)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("uri"),
            "error does not name the variable: {err}"
        );
    }

    /// Measured: `[p1=it's,p2=don't,p3=x]` reaches the channel as
    /// `p1=its,p2=dont` with no `p2`, at every escaping depth, because the scan
    /// ahead of the peer split pairs quotes across values. The same value in
    /// default scope is ordinary.
    #[test]
    fn a_quote_in_channel_scope_is_refused_at_every_boundary() {
        let err = r"[cid=it\\\\\\\'s]"
            .parse::<Variables>()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("cid") && err.contains("channel scope"),
            "error does not name the variable and scope: {err}"
        );

        let err = serde_json::from_str::<Variables>(r#"{"scope":"channel","vars":{"cid":"it's"}}"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("cid") && err.contains("channel scope"),
            "error does not name the variable and scope: {err}"
        );

        assert!(r"{cid=it\\\\\\\'s}"
            .parse::<Variables>()
            .is_ok());
        assert!(
            serde_json::from_str::<Variables>(r#"{"scope":"default","vars":{"cid":"it's"}}"#)
                .is_ok()
        );
    }

    /// Every revision a block can be rendered for; a new variant joins this list.
    const REVISIONS: &[BlockParse] = &[BlockParse::PairSplitCleans];

    #[test]
    fn a_bare_carrier_is_a_target_at_the_default_revision() {
        let target = DialStringTarget::from(DialStringCarrier::Dialplan);
        assert_eq!(target.carrier(), DialStringCarrier::Dialplan);
        assert_eq!(target.block_parse(), BlockParse::default());
        assert_eq!(BlockParse::default(), BlockParse::PairSplitCleans);
        assert_eq!(
            DialStringTarget::new(DialStringCarrier::EslApi)
                .with_block_parse(BlockParse::PairSplitCleans)
                .block_parse(),
            BlockParse::PairSplitCleans
        );
    }

    /// An enterprise block is parsed at the same depth as a default one; nothing
    /// else pins its forms.
    #[test]
    fn enterprise_scope_escapes_like_default_scope() {
        let cases = [
            (DialStringCarrier::Dialplan, "it's", r"it\\\\\\'s"),
            (DialStringCarrier::EslApi, "it's", r"it\\\\\\\'s"),
            (DialStringCarrier::EslApi, r"a\nb", r"a\\\\\\\\nb"),
            (DialStringCarrier::EslApi, "a,b", r"a\,b"),
        ];
        for (carrier, value, want) in cases {
            let target =
                DialStringTarget::new(carrier).with_block_parse(BlockParse::PairSplitCleans);
            assert_eq!(
                escape_value(value, target, true, VariablesType::Enterprise),
                want,
                "{value:?} for {carrier:?}"
            );
        }
    }

    /// Measured: the `=` split trims both edges of a value, quoted or not, so an edge space
    /// must still read `\s` entering it.
    #[test]
    fn an_edge_space_is_escaped_for_the_pass_that_trims_it() {
        let api = DialStringTarget::new(DialStringCarrier::EslApi);
        let dialplan = DialStringTarget::new(DialStringCarrier::Dialplan);
        let cases = [
            (api, VariablesType::Default, " a b ", r"'\\\\sa b\\\\s'"),
            (
                dialplan,
                VariablesType::Default,
                " a b ",
                r"'\\\\sa b\\\\s'",
            ),
            (api, VariablesType::Enterprise, " a", r"\\\\sa"),
            (api, VariablesType::Default, "a  ", r"'a \\\\s'"),
            (api, VariablesType::Default, " ", r"\\\\s"),
            (api, VariablesType::Default, "  ", r"\\\\s\\\\s"),
            (api, VariablesType::Default, r"a\ ", r"a\\\\\\\\\\\\s"),
            (
                api,
                VariablesType::Channel,
                " a ",
                &format!("{0}sa{0}s", "\\".repeat(16)),
            ),
        ];
        for (target, scope, value, want) in cases {
            assert_eq!(
                escape_value(value, target, true, scope),
                want,
                "{value:?} in {scope:?} at {target:?}"
            );
        }

        let mut vars = Variables::new(VariablesType::Default);
        vars.insert("k", " a b ");
        assert_eq!(
            vars.display_for(tilde())
                .to_string(),
            r"{k=\'\\\\sa b\\\\s\'}"
        );
    }

    /// The scan ahead of the leg split protects a `[]` comma only when the byte before it is no
    /// backslash, so a value ending in one is followed by an empty `''` reaching that scan bare.
    #[test]
    fn a_channel_value_ending_in_a_backslash_guards_the_next_comma() {
        let run = "\\".repeat(32);
        for (target, guard) in [
            (
                DialStringTarget::new(DialStringCarrier::EslApi),
                r"\\\'\\\'",
            ),
            (
                DialStringTarget::new(DialStringCarrier::Dialplan),
                r"\\'\\'",
            ),
        ] {
            assert_eq!(
                escape_value(r"a\", target, true, VariablesType::Channel),
                format!("a{run}{guard}"),
                "{target:?}"
            );
            assert_eq!(
                escape_value(r"a\", target, false, VariablesType::Channel),
                format!("a{run}"),
                "{target:?}"
            );
            assert_eq!(
                escape_value(r"a\,b", target, false, VariablesType::Channel),
                format!("a{run}{guard},b"),
                "{target:?}"
            );
        }
        assert_eq!(
            escape_value(
                r"a\",
                DialStringCarrier::EslApi,
                true,
                VariablesType::Default
            ),
            r"a\\\\\\\\"
        );
    }

    #[test]
    fn round_trips_at_every_target() {
        const EDGES: [&str; 6] = [" lead and trail ", " a", "a  ", " ", r"a\ ", r"a\s"];
        let cases = [
            (
                VariablesType::Default,
                &["it's", r"C:\path", "a,b", r"a\nb", "with space", "x~y"][..],
            ),
            (
                VariablesType::Enterprise,
                &["it's", r"C:\path", "a,b", "with space", "x~y"][..],
            ),
            (
                VariablesType::Channel,
                &[r"C:\path", "a,b", "a|b", "with space", "x~y"][..],
            ),
        ];
        let cases = cases.map(|(scope, values)| {
            let values: Vec<&str> = values
                .iter()
                .chain(&EDGES)
                .copied()
                .collect();
            (scope, values)
        });
        for &block_parse in REVISIONS {
            for target in [
                DialStringTarget::new(DialStringCarrier::EslApi),
                DialStringTarget::new(DialStringCarrier::Dialplan),
                tilde(),
            ] {
                let target = target.with_block_parse(block_parse);
                for (scope, values) in &cases {
                    for &value in values {
                        let mut vars = Variables::new(*scope);
                        vars.insert("k", value);
                        vars.insert("after", "sentinel");
                        let rendered = vars
                            .display_for(target)
                            .to_string();

                        let back = Variables::parse_for(&rendered, target).unwrap_or_else(|e| {
                            panic!("{value:?} for {target:?} rendered {rendered}: {e}")
                        });
                        assert_eq!(back.get("k"), Some(value), "rendered {rendered}");
                        assert_eq!(back.get("after"), Some("sentinel"), "rendered {rendered}");
                    }
                }
            }
        }
    }

    /// What the switch cannot deliver is decided before any block parse runs, so
    /// no revision makes one of these representable.
    #[test]
    fn refusals_do_not_depend_on_the_revision() {
        for &block_parse in REVISIONS {
            for carrier in [DialStringCarrier::EslApi, DialStringCarrier::Dialplan] {
                let target = DialStringTarget::new(carrier).with_block_parse(block_parse);
                for block in [
                    r"[cid=it\\\\\\\'s]",
                    "{k=,after=sentinel}",
                    "{k=oops}extra}",
                    "{^^=a=1=b=2}",
                    "[^^|a=1|b=2]",
                ] {
                    assert!(
                        Variables::parse_for(block, target).is_err(),
                        "{block} accepted at {target:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn block_parse_reads_config_spellings_and_writes_the_canonical_one() {
        for spelling in [
            "pair_split_cleans",
            "PAIR_SPLIT_CLEANS",
            "Pair_Split_Cleans",
        ] {
            assert_eq!(
                spelling
                    .parse::<BlockParse>()
                    .ok(),
                Some(BlockParse::PairSplitCleans),
                "{spelling}"
            );
        }
        assert_eq!(BlockParse::PairSplitCleans.to_string(), "pair_split_cleans");

        let err = "pair_split_whatever"
            .parse::<BlockParse>()
            .unwrap_err()
            .to_string();
        assert!(!err.contains("whatever"), "error quoted its input: {err}");
    }

    #[test]
    fn serde_block_parse_uses_the_config_spelling() {
        assert_eq!(
            serde_json::to_string(&BlockParse::PairSplitCleans).unwrap(),
            r#""pair_split_cleans""#
        );
        let parsed: BlockParse = serde_json::from_str(r#""pair_split_cleans""#).unwrap();
        assert_eq!(parsed, BlockParse::PairSplitCleans);
    }

    #[test]
    fn for_version_maps_a_vouched_release() {
        for version in [
            FreeswitchVersion::new(1, 10, 0),
            FreeswitchVersion::new(1, 10, 7),
            FreeswitchVersion::new(1, 10, 12),
        ] {
            assert_eq!(
                BlockParse::for_version(&version).ok(),
                Some(BlockParse::PairSplitCleans),
                "{version}"
            );
        }
    }

    /// A dev build reports one version across every commit, so even one below
    /// the vouched range's last release is refused.
    #[test]
    fn for_version_refuses_what_it_cannot_vouch_for() {
        for version in [
            FreeswitchVersion::new(1, 10, 12).dev(),
            FreeswitchVersion::new(1, 10, 5).dev(),
        ] {
            let err = BlockParse::for_version(&version).expect_err(&version.to_string());
            assert!(
                matches!(err, UnvouchedVersion::Dev { .. }),
                "{version}: {err:?}"
            );
            assert_eq!(err.version(), version);
        }
        for version in [
            FreeswitchVersion::new(1, 10, 13),
            FreeswitchVersion::new(1, 11, 1),
        ] {
            let err = BlockParse::for_version(&version).expect_err(&version.to_string());
            assert!(
                matches!(err, UnvouchedVersion::NewerThanVouched { .. }),
                "{version}: {err:?}"
            );
        }
        let version = FreeswitchVersion::new(1, 8, 7);
        let err = BlockParse::for_version(&version).expect_err(&version.to_string());
        assert!(
            matches!(err, UnvouchedVersion::OlderThanVouched { .. }),
            "{version}: {err:?}"
        );
    }

    fn tilde() -> DialStringTarget {
        DialStringTarget::new(DialStringCarrier::EslApi)
            .with_argv_separator('~')
            .expect("'~' separates originate's arguments")
    }

    #[test]
    fn an_argv_separator_is_part_of_the_target() {
        let blank = DialStringTarget::new(DialStringCarrier::EslApi);
        assert_eq!(blank.argv_separator(), None);
        assert_eq!(tilde().argv_separator(), Some('~'));
        assert_eq!(tilde().carrier(), DialStringCarrier::EslApi);
        assert_eq!(
            tilde()
                .with_block_parse(BlockParse::PairSplitCleans)
                .argv_separator(),
            Some('~')
        );
        assert_ne!(tilde(), blank);
    }

    #[test]
    fn the_dialplan_carrier_takes_no_argv_separator() {
        assert_eq!(
            DialStringTarget::new(DialStringCarrier::Dialplan).with_argv_separator('~'),
            Err(InvalidArgvSeparator::WrongCarrier(
                DialStringCarrier::Dialplan
            ))
        );
    }

    /// Space, `\`, `'`, lowercase `n r t s`, controls and non-ASCII break the switch's split
    /// or its escapes; the rest read as grammar, quoting, an escape letter or a word.
    #[test]
    fn unusable_argv_separators_are_refused() {
        for sep in [
            ' ', '\\', '\'', 'é', '\n', '\r', '\t', '\0', '\u{b}', 'n', 'r', 't', 's', 'N', 'R',
            'T', 'S', 'a', '0', '^', '"', ',', '|', '[', ']', '{', '}', '<', '>', '=', ':',
        ] {
            let err = DialStringTarget::new(DialStringCarrier::EslApi)
                .with_argv_separator(sep)
                .expect_err(&format!("accepted {sep:?}"));
            assert_eq!(err, InvalidArgvSeparator::Unusable(sep));
            assert!(!err
                .to_string()
                .is_empty());
        }
        for sep in ['~', ';', '!', '#'] {
            assert!(
                DialStringTarget::new(DialStringCarrier::EslApi)
                    .with_argv_separator(sep)
                    .is_ok(),
                "refused {sep:?}"
            );
        }
    }

    #[test]
    fn escape_argument_is_none_only_at_the_dialplan_carrier() {
        assert_eq!(
            DialStringTarget::new(DialStringCarrier::Dialplan).escape_argument("a b"),
            None
        );
        for (text, want) in [
            ("a b", r"a\sb"),
            (" a  b ", r"\sa\s\sb\s"),
            ("x~y", "x~y"),
            ("it's", r"it\'s"),
            (r"a\b", r"a\\b"),
            ("a\nb\tc\rd", r"a\nb\tc\rd"),
            ("", "''"),
            ("^^~a b", r"''^^~a\sb"),
        ] {
            assert_eq!(
                DialStringTarget::new(DialStringCarrier::EslApi)
                    .escape_argument(text)
                    .as_deref(),
                Some(want),
                "{text:?}"
            );
        }
        assert!(matches!(
            tilde().escape_argument("loopback/9199/test"),
            Some(std::borrow::Cow::Borrowed("loopback/9199/test"))
        ));
    }

    /// The last case is a capture originated under `^^~`.
    #[test]
    fn escape_argument_escapes_for_the_separator_split() {
        for (text, want) in [
            ("a b", "a b"),
            ("x~y", r"x\~y"),
            ("it's o'k", r"it\'s o\'k"),
            (r"a\b", r"a\\b"),
            (r"a\~b", r"a\\\~b"),
            (r"end\", r"end\\"),
            ("a\nb", r"a\nb"),
            ("a\rb", r"a\rb"),
            ("a\tb", r"a\tb"),
            (
                r"[presence_id=fp-argv-leg2@pbx.example.com,v=a b~c\d,sentinel=s]loopback/9199/test,[presence_id=fp-argv-leg2@pbx.example.com,sentinel=s2]loopback/9199/test",
                r"[presence_id=fp-argv-leg2@pbx.example.com,v=a b\~c\\d,sentinel=s]loopback/9199/test,[presence_id=fp-argv-leg2@pbx.example.com,sentinel=s2]loopback/9199/test",
            ),
        ] {
            assert_eq!(
                tilde()
                    .escape_argument(text)
                    .as_deref(),
                Some(want),
                "{text:?}"
            );
        }
    }

    #[test]
    fn escape_argument_keeps_edge_spaces() {
        for (text, want) in [
            (" a ", r"\sa\s"),
            ("a  ", r"a \s"),
            ("  a", r"\s a"),
            (" ", r"\s"),
            ("  ", r"\s\s"),
            ("", "''"),
            ("^^~", "^^\\~"),
        ] {
            assert_eq!(
                tilde()
                    .escape_argument(text)
                    .as_deref(),
                Some(want),
                "{text:?}"
            );
        }
    }

    /// `switch_api_execute` strips a vertical tab from the edges of the argument line before
    /// `originate` splits it, and no escape letter names one.
    #[test]
    fn a_vertical_tab_at_an_edge_survives_the_line_strip() {
        use crate::tokenizer::{separate, trace, untrace};

        for text in ["\u{b}", "\u{b}a", "a\u{b}", "\u{b} \u{b}"] {
            for target in [DialStringTarget::new(DialStringCarrier::EslApi), tilde()] {
                let escaped = target
                    .escape_argument(text)
                    .expect("an API target escapes");
                let (prefix, sep) = match target.argv_separator() {
                    Some(sep) => (format!("^^{sep}"), sep),
                    None => (String::new(), ' '),
                };
                for (line, want) in [
                    (format!("{prefix}{escaped}{sep}y"), vec![text, "y"]),
                    (format!("{prefix}x{sep}{escaped}"), vec!["x", text]),
                ] {
                    let stripped = line.trim_matches(['\r', '\n', '\t', ' ', '\u{b}']);
                    let tokens: Vec<String> = separate(&trace(stripped), ' ', usize::MAX)
                        .tokens
                        .iter()
                        .map(|token| untrace(&token.text))
                        .collect();
                    assert_eq!(tokens, want, "{line:?}");
                }
            }
        }
    }

    /// An empty argument vanished from either split and a text opening `^^` renamed the blank
    /// split's separator when it opened the line.
    #[test]
    fn an_empty_or_caret_led_text_stays_one_argument_in_any_position() {
        use crate::tokenizer::{separate, trace, untrace};

        for text in ["", "^^", "^^~a", "^^ y"] {
            for target in [DialStringTarget::new(DialStringCarrier::EslApi), tilde()] {
                let escaped = target
                    .escape_argument(text)
                    .expect("an API target escapes");
                let (prefix, sep) = match target.argv_separator() {
                    Some(sep) => (format!("^^{sep}"), sep),
                    None => (String::new(), ' '),
                };
                for (line, want) in [
                    (format!("{prefix}{escaped}{sep}y"), vec![text, "y"]),
                    (
                        format!("{prefix}x{sep}{escaped}{sep}y"),
                        vec!["x", text, "y"],
                    ),
                    (format!("{prefix}x{sep}{escaped}"), vec!["x", text]),
                ] {
                    let tokens: Vec<String> = separate(&trace(&line), ' ', usize::MAX)
                        .tokens
                        .iter()
                        .map(|token| untrace(&token.text))
                        .collect();
                    assert_eq!(tokens, want, "{line:?}");
                }
            }
        }
    }

    #[test]
    fn escape_argument_is_undone_by_the_split_cleanup() {
        use crate::tokenizer::{cleanup, trace, untrace};

        for text in [
            "a b",
            " a b ",
            r"it's \'q\' ''",
            r#"a"b\"c"#,
            r"\\\~~\n\s",
            "tab\tcr\rnl\n",
            "x~y~",
            "~",
            r"trailing\",
            "é ü",
        ] {
            let escaped = tilde()
                .escape_argument(text)
                .expect("a separator target escapes");
            assert_eq!(
                untrace(&cleanup(&trace(&escaped), Some('~'))),
                text,
                "escaped {escaped:?}"
            );
        }
    }

    #[test]
    fn escape_argument_is_undone_by_the_blank_split() {
        let api = DialStringTarget::new(DialStringCarrier::EslApi);
        for text in [
            "a b",
            "  a  b  ",
            r"it's \'q\' ''",
            r#"a"b\"c"#,
            r"\\\~~\n\s",
            "tab\tcr\rnl\n",
            r"trailing\",
            "é ü",
            "'",
            "",
        ] {
            let escaped = api
                .escape_argument(text)
                .expect("the API carrier escapes");
            let (argument, _) = api
                .read_argument(&escaped)
                .unwrap_or_else(|e| panic!("{escaped:?}: {e}"));
            assert_eq!(argument, text, "escaped {escaped:?}");
        }
    }

    /// Inside the argument a value meets the same passes as at the blank split, so the
    /// argument escape is the whole difference between the two renders.
    #[test]
    fn a_block_at_an_argv_separator_is_escaped_once_at_its_edge() {
        let cases = [
            (
                VariablesType::Default,
                "it's",
                r"{k=it\\\\\\\'s}".to_owned(),
            ),
            (
                VariablesType::Default,
                r"a\nb",
                r"{k=a\\\\\\\\nb}".to_owned(),
            ),
            (VariablesType::Default, "x~y", r"{k=x\~y}".to_owned()),
            (VariablesType::Default, "a,b", r"{k=a\\,b}".to_owned()),
            (VariablesType::Default, "a b", r"{k=\'a b\'}".to_owned()),
            (VariablesType::Channel, "a|b", r"[k=a\\|b]".to_owned()),
            (
                VariablesType::Channel,
                r"a\nb",
                format!("[k=a{}nb]", "\\".repeat(32)),
            ),
        ];
        for (scope, value, want) in cases {
            let mut vars = Variables::new(scope);
            vars.insert("k", value);
            assert_eq!(
                vars.display_for(tilde())
                    .to_string(),
                want,
                "{value:?} in {scope:?}"
            );
        }
    }

    #[test]
    fn a_block_cut_by_its_argv_separator_is_refused() {
        for block in ["{k=a}~{j=b}", "{k=a~j=b}", "{k='a~b'}"] {
            let err = Variables::parse_for(block, tilde()).expect_err(block);
            assert!(!err
                .to_string()
                .contains("k="));
        }
        assert_eq!(
            Variables::parse_for("{k=a}~", tilde())
                .expect("a trailing separator adds no argument")
                .get("k"),
            Some("a")
        );
    }

    #[test]
    fn a_block_cut_by_the_blank_split_is_refused() {
        for block in ["{k=a b}", r"{k=x\\'y}", "{k=a} {j=b}"] {
            let err = Variables::parse_for(block, DialStringCarrier::EslApi).expect_err(block);
            assert!(!err
                .to_string()
                .contains("k="));
        }
        for (block, want) in [
            ("{k='a b'}", "a b"),
            (r"{k=\\\\sa\\\\s}", " a "),
            (" {k=v} ", "v"),
        ] {
            assert_eq!(
                Variables::parse_for(block, DialStringCarrier::EslApi)
                    .unwrap_or_else(|e| panic!("{block}: {e}"))
                    .get("k"),
                Some(want),
                "{block}"
            );
        }
        assert!(Variables::parse_for("{k=a b}", DialStringCarrier::Dialplan).is_ok());
    }

    /// `switch_ivr_originate` takes the enterprise path on any `:_:` in the dial string, and
    /// that split honours no quote or escape.
    #[test]
    fn a_value_carrying_the_enterprise_separator_is_refused() {
        for block in ["{k=x:_:y}", "<k=x:_:y>", "[k=x:_:y]", "{k='a :_: b'}"] {
            let err = Variables::parse_for(block, DialStringCarrier::EslApi).expect_err(block);
            assert!(!err
                .to_string()
                .contains("x:_:y"));
        }
        assert!(serde_json::from_str::<Variables>(r#"{"k":"x:_:y"}"#).is_err());
    }

    /// The `=` split skips a byte after a backslash and its cleanup reads `\=`, and a pair
    /// opening `^^` names that split's separator unless a quote pair the cleanup strips leads it.
    #[test]
    fn a_key_the_switch_splits_arrives_escaped_and_round_trips() {
        use crate::commands::flattened::pipeline::{self, PairEffect};

        let keys = [
            "a=b", "a,b", "a b", "^^ab", "^^", "x^^", " edge ", r"C:\p", "it's", "pa$$", "a|b",
            r"a\=b", r"end\",
        ];
        for target in [
            DialStringTarget::new(DialStringCarrier::EslApi),
            DialStringTarget::new(DialStringCarrier::Dialplan),
            tilde(),
        ] {
            for scope in [
                VariablesType::Default,
                VariablesType::Enterprise,
                VariablesType::Channel,
            ] {
                for key in keys {
                    if scope == VariablesType::Channel && key.contains('\'') {
                        continue;
                    }
                    let mut vars = Variables::new(scope);
                    vars.insert(key, "v");
                    vars.insert("after", "sentinel");
                    let rendered = vars
                        .display_for(target)
                        .to_string();
                    let list = pipeline::read(&format!("{rendered}null/x"), target)
                        .unwrap_or_else(|e| panic!("{rendered:?} at {target:?}: {e:?}"));
                    let installed: Vec<(String, PairEffect)> = list
                        .blocks
                        .iter()
                        .chain(&list.threads[0].blocks)
                        .chain(&list.threads[0].groups[0][0].blocks)
                        .flat_map(|block| &block.pairs)
                        .map(|pair| {
                            (
                                pair.key
                                    .clone(),
                                pair.effect
                                    .clone(),
                            )
                        })
                        .collect();
                    assert_eq!(
                        installed,
                        [
                            (key.to_owned(), PairEffect::Set("v".to_owned())),
                            ("after".to_owned(), PairEffect::Set("sentinel".to_owned()))
                        ],
                        "{key:?} in {scope:?} at {target:?}: {rendered:?}"
                    );
                    let back = Variables::parse_for(&rendered, target)
                        .unwrap_or_else(|e| panic!("{rendered:?} at {target:?}: {e}"));
                    assert_eq!(back, vars, "{rendered:?} at {target:?}");
                }
            }
        }
    }

    /// Each key carries `SECRET`, which no refusal may quote.
    #[test]
    fn a_key_no_escaping_delivers_is_refused_at_parse_and_config_load() {
        for block in [
            "{SECRET:_:x=v}",
            "{SECRET}=v}",
            "{=v,after=sentinel}",
            r"[SECRET\\\\\\\'s=v]",
        ] {
            let msg = Variables::parse_for(block, DialStringCarrier::Dialplan)
                .expect_err(block)
                .to_string();
            assert!(!msg.contains("SECRET"), "{block}: {msg}");
        }
        for json in [
            r#"{"SECRET:_:x":"v"}"#,
            r#"{"SECRET}":"v"}"#,
            r#"{"":"v"}"#,
            r#"{"scope":"channel","vars":{"SECRET's":"v"}}"#,
            r#"{"scope":"channel","vars":{"SECRET]":"v"}}"#,
        ] {
            let msg = serde_json::from_str::<Variables>(json)
                .expect_err(json)
                .to_string();
            assert!(!msg.contains("SECRET"), "{json}: {msg}");
        }
        assert!(serde_json::from_str::<Variables>(r#"{"a=b, c":"v"}"#).is_ok());
    }

    /// `switch_event_base_add_header` reads a name carrying `[` as an array index and installs the
    /// value under the text before it, in every scope.
    #[test]
    fn a_key_carrying_an_array_index_is_refused_at_parse_and_config_load() {
        for block in [
            "{SECRET[1]=v,after=sentinel}",
            "<SECRET[x=v>",
            "[SECRET[1]=v]",
            r"{^^;SECRET[0]=v;after=a,b}",
        ] {
            let msg = Variables::parse_for(block, DialStringCarrier::Dialplan)
                .expect_err(block)
                .to_string();
            assert!(!msg.contains("SECRET"), "{block}: {msg}");
        }
        for json in [
            r#"{"SECRET[1]":"v"}"#,
            r#"{"scope":"channel","vars":{"SECRET[0]":"v"}}"#,
        ] {
            let msg = serde_json::from_str::<Variables>(json)
                .expect_err(json)
                .to_string();
            assert!(!msg.contains("SECRET"), "{json}: {msg}");
        }
        assert!(serde_json::from_str::<Variables>(r#"{"SECRET]":"v"}"#).is_ok());
    }

    /// A block's variables land in one `EF_UNIQ_HEADERS` event, whose add deletes every header of
    /// the name by `strcasecmp`, so two names differing only in case install as one.
    #[test]
    fn two_keys_differing_only_in_case_are_refused_at_parse_and_config_load() {
        for block in [
            "{Secret=1,SECRET=2}",
            "<secret=1,after=a,SECRET=2>",
            "[SeCrEt=1,secret=2]",
            "{^^;secret=1;SECRET=a,b}",
        ] {
            let msg = Variables::parse_for(block, DialStringCarrier::EslApi)
                .expect_err(block)
                .to_string();
            assert!(
                !msg.to_ascii_lowercase()
                    .contains("secret"),
                "{block}: {msg}"
            );
        }
        for json in [
            r#"{"Secret":"1","SECRET":"2"}"#,
            r#"{"scope":"enterprise","vars":{"secret":"1","Secret":"2"}}"#,
        ] {
            let msg = serde_json::from_str::<Variables>(json)
                .expect_err(json)
                .to_string();
            assert!(
                !msg.to_ascii_lowercase()
                    .contains("secret"),
                "{json}: {msg}"
            );
        }
        let same = Variables::parse_for("{k=1,k=2}", DialStringCarrier::EslApi)
            .expect("a repeated name is the last one written, on the switch and here");
        assert_eq!(same.get("k"), Some("2"));
    }

    /// The caller has to decide what to name instead, so the refusal says what
    /// would have been accepted.
    #[test]
    fn an_unvouched_version_names_the_vouched_range() {
        let msg = BlockParse::for_version(&FreeswitchVersion::new(1, 11, 1))
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("1.10.0") && msg.contains("1.10.12"),
            "does not name the vouched range: {msg}"
        );
    }
}
