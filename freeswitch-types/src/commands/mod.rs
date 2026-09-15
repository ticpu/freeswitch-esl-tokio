//! Command string builders for [`api()`] and [`bgapi()`].
//!
//! [`api()`]: https://docs.rs/freeswitch-esl-tokio/latest/freeswitch_esl_tokio/connection/struct.EslClient.html#method.api
//! [`bgapi()`]: https://docs.rs/freeswitch-esl-tokio/latest/freeswitch_esl_tokio/connection/struct.EslClient.html#method.bgapi
//!
//! Each builder implements [`Display`](std::fmt::Display), producing the argument
//! string for the corresponding FreeSWITCH API command.  The builders perform
//! escaping and validation so callers don't need to worry about wire-format
//! details.

pub mod bridge;
pub mod channel;
pub mod conference;
pub mod endpoint;
pub mod execute_on;
pub mod flattened;
pub mod originate;
pub mod variables;

pub use bridge::{BridgeDialString, BridgeDialStringDisplay};
pub use channel::{
    UuidAnswer, UuidBridge, UuidDeflect, UuidGetVar, UuidHold, UuidKill, UuidSendDtmf, UuidSetVar,
    UuidTransfer,
};
pub use conference::{
    ConferenceDtmf, ConferenceHold, ConferenceMute, HoldAction, MuteAction, ParseHoldActionError,
    ParseMuteActionError,
};
pub use endpoint::{
    AudioEndpoint, DialString, EndpointDisplay, ErrorEndpoint, GroupCall, GroupCallOrder,
    LoopbackEndpoint, ParseGroupCallOrderError, SofiaContact, SofiaEndpoint, SofiaGateway,
    UserEndpoint,
};
pub use execute_on::ExecuteOn;
pub use flattened::{
    CauseReading, ErrorLeg, FlattenedDialString, FlattenedDialStringDisplay,
    FlattenedDialStringError, FlattenedGroup, FlattenedLeg, FlattenedThread, LegTarget, LegWarning,
    ListWarning, UnparsedLeg,
};
pub use originate::{
    Application, DialplanType, Endpoint, Originate, OriginateDisplay, OriginateError,
    OriginateTarget, ParseDialplanTypeError, Variables, VariablesType,
};
pub use variables::{
    BlockParse, DialStringCarrier, DialStringTarget, InvalidArgvSeparator, ParseBlockParseError,
    UnvouchedVersion, VariablesDisplay,
};

use crate::tokenizer::{blank_delim_spans, char_delim_spans, delimiter_override, trace};
use originate::DEFAULT_INLINE_DELIMITER;

/// Find the index of the closing bracket matching the opener at position 0.
///
/// Tracks nesting depth so that inner pairs of the same bracket type are
/// skipped. Returns `None` if the string never reaches depth 0.
pub(crate) fn find_matching_bracket(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Wrap a token in single quotes for originate command strings.
///
/// A token carrying a single quote is wrapped as [`quote_for_uuid_setvar`] does, which the
/// same blank split reads back; one carrying only spaces is wrapped in `'...'`; any other is
/// returned as-is.
pub fn originate_quote(token: &str) -> String {
    if token.contains('\'') {
        quote_for_uuid_setvar(token)
    } else if token.contains(' ') {
        format!("'{token}'")
    } else {
        token.to_string()
    }
}

/// Escape and single-quote a value for the argument string of `uuid_setvar`.
///
/// That command splits its arguments on spaces through `cleanup_separated_string`,
/// which honours `'` grouping and processes `\` escapes inside the quoted region,
/// so an unquoted value is silently truncated at its first space and a bare `'`
/// or `\` inside the quotes ends or eats a character. The inline originate
/// `{var=…}` block is a different carrier with different escaping.
pub fn quote_for_uuid_setvar(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Strip single-quote wrapping added by [`originate_quote`].
///
/// If the token starts and ends with `'`, the outer quotes are removed
/// and `\'` sequences are unescaped back to `'`.
pub fn originate_unquote(token: &str) -> String {
    match token
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
    {
        Some(inner) => inner.replace("\\'", "'"),
        None => token.to_string(),
    }
}

/// Split a command line the way the `originate` API splits its arguments.
///
/// `split_at` is the default a leading `^^X` overrides, as `switch_separate_string`
/// in `switch_utils.c` reads one: an ASCII `X` followed by at least one byte, the
/// prefix itself dropped. The switch takes a non-ASCII `X` as its first UTF-8 byte,
/// which no char delimiter mirrors, so such a line splits on `split_at` whole.
///
/// On a space this is `separate_string_blank_delim`, on any other delimiter
/// `separate_string_char_delim`. Tokens keep their quotes and escapes:
/// [`originate_unquote`] and the variable-block parser consume those.
///
/// A quote left open is an error. The switch runs the rest of the line into one
/// argument instead, a shape nothing this crate renders produces.
pub fn originate_split(line: &str, split_at: char) -> Result<Vec<String>, OriginateError> {
    let text = trace(line);
    let (picked, body) = delimiter_override(&text);
    let split_at = picked.unwrap_or(split_at);
    if split_at != ' ' {
        return Ok(char_delim_spans(line, body, split_at)
            .into_iter()
            .map(str::to_string)
            .collect());
    }
    let (spans, open_quote) = blank_delim_spans(line, body);
    if open_quote {
        let last = spans
            .last()
            .copied()
            .unwrap_or_default();
        return Err(OriginateError::UnclosedQuote(last.to_string()));
    }
    Ok(spans
        .into_iter()
        .map(|t| {
            t.trim_start_matches(' ')
                .to_string()
        })
        .collect())
}

/// Split an `m:<delim>:` prefix off an inline action list.
///
/// `inline_dialplan_hunt` reads exactly four bytes for this, so anything longer
/// or shorter is part of the first application rather than a prefix.
pub(crate) fn split_inline_prefix(s: &str) -> (Option<char>, &str) {
    let bytes = s.as_bytes();
    match bytes {
        [b'm', b':', delimiter, b':', ..] if delimiter.is_ascii() && *delimiter != b':' => {
            (Some(*delimiter as char), &s[4..])
        }
        _ => (None, s),
    }
}

/// Split an inline action list on unescaped separators, unescaping the escaped
/// ones as `cleanup_separated_string` would.
fn split_inline_actions(s: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                // Only the separator is unescaped here, matching a cleanup
                // whose delim is this one; anything else keeps its backslash.
                Some(next) if next == delimiter => current.push(next),
                Some(next) => {
                    current.push(ch);
                    current.push(next);
                }
                None => current.push(ch),
            }
        } else if ch == delimiter {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    parts.push(current);

    parts
}

/// Parse the target argument of an originate command.
///
/// Determines whether the target is a dialplan extension or application(s):
///
/// - If dialplan is `Inline`: parse as inline apps → `InlineApplications`
/// - If string starts with `&`: parse as XML app → `Application`
/// - Otherwise: bare string → `Extension`
pub fn parse_originate_target(
    s: &str,
    dialplan: Option<&DialplanType>,
) -> Result<OriginateTarget, OriginateError> {
    if matches!(dialplan, Some(DialplanType::Inline)) {
        let (delimiter, s) = split_inline_prefix(s);
        let delimiter = delimiter.unwrap_or(DEFAULT_INLINE_DELIMITER);
        let mut apps = Vec::new();
        for part in split_inline_actions(s, delimiter) {
            let (name, args) = match part.split_once(':') {
                Some((n, "")) => (n, None),
                Some((n, a)) => (n, Some(a)),
                None => (part.as_str(), None),
            };
            apps.push(Application::new(name, args));
        }
        Ok(OriginateTarget::InlineApplications(apps))
    } else if let Some(rest) = s.strip_prefix('&') {
        let rest = rest
            .strip_suffix(')')
            .ok_or_else(|| OriginateError::ParseError("missing closing paren".into()))?;
        let (name, args) = rest
            .split_once('(')
            .ok_or_else(|| OriginateError::ParseError("missing opening paren".into()))?;
        let args = if args.is_empty() { None } else { Some(args) };
        Ok(OriginateTarget::Application(Application::new(name, args)))
    } else {
        Ok(OriginateTarget::Extension(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_matching_bracket_simple() {
        assert_eq!(find_matching_bracket("{abc}", '{', '}'), Some(4));
    }

    #[test]
    fn find_matching_bracket_nested() {
        assert_eq!(find_matching_bracket("{a={b}}", '{', '}'), Some(6));
    }

    #[test]
    fn find_matching_bracket_unclosed() {
        assert_eq!(find_matching_bracket("{a={b}", '{', '}'), None);
    }

    #[test]
    fn find_matching_bracket_angle() {
        assert_eq!(find_matching_bracket("<a=<b>>rest", '<', '>'), Some(6));
    }

    #[test]
    fn split_with_quotes_ignores_spaces_inside() {
        let result =
            originate_split("originate {test='variable with quote'}sofia/test 123", ' ').unwrap();
        assert_eq!(result[0], "originate");
        assert_eq!(result[1], "{test='variable with quote'}sofia/test");
        assert_eq!(result[2], "123");
    }

    #[test]
    fn split_missing_quote_returns_error() {
        let result = originate_split(
            "originate {test='variable with missing quote}sofia/test 123",
            ' ',
        );
        assert!(result.is_err());
    }

    #[test]
    fn split_string_starting_ending_with_quote() {
        let result = originate_split("'this is test'", ' ').unwrap();
        assert_eq!(result[0], "'this is test'");
    }

    #[test]
    fn split_comma_separated() {
        let result = originate_split("item1,item2", ',').unwrap();
        assert_eq!(result[0], "item1");
        assert_eq!(result[1], "item2");
    }

    #[test]
    fn split_with_escaped_quotes() {
        let result = originate_split(
            "originate {test='variable with quote'}sofia/test let\\'s add a quote",
            ' ',
        )
        .unwrap();
        assert_eq!(result[0], "originate");
        assert_eq!(result[1], "{test='variable with quote'}sofia/test");
        assert_eq!(result[2], "let\\'s");
        assert_eq!(result[3], "add");
        assert_eq!(result[4], "a");
        assert_eq!(result[5], "quote");
    }

    #[test]
    fn quote_without_spaces_returns_as_is() {
        assert_eq!(originate_quote("&park()"), "&park()");
    }

    #[test]
    fn quote_with_spaces_wraps_in_single_quotes() {
        assert_eq!(
            originate_quote("&socket(127.0.0.1:8040 async full)"),
            "'&socket(127.0.0.1:8040 async full)'"
        );
    }

    #[test]
    fn quote_with_single_quote_and_spaces_escapes_quote() {
        assert_eq!(
            originate_quote("&playback(it's a test file)"),
            "'&playback(it\\'s a test file)'"
        );
    }

    /// A bare quote opens a region the blank split never closes, and a backslash inside the
    /// wrapping is consumed by that split's cleanup.
    #[test]
    fn quote_wraps_any_token_carrying_a_single_quote() {
        assert_eq!(originate_quote("it's"), r"'it\'s'");
        assert_eq!(originate_quote(r"a\b it's"), r"'a\\b it\'s'");
        assert_eq!(originate_quote("a b"), "'a b'");
        assert_eq!(originate_quote(r"a\b c"), r"'a\b c'");
        assert_eq!(originate_quote(r"a\,b"), r"a\,b");
    }

    #[test]
    fn unquote_non_quoted_returns_as_is() {
        assert_eq!(originate_unquote("&park()"), "&park()");
    }

    #[test]
    fn unquote_strips_outer_quotes() {
        assert_eq!(
            originate_unquote("'&socket(127.0.0.1:8040 async full)'"),
            "&socket(127.0.0.1:8040 async full)"
        );
    }

    #[test]
    fn unquote_unescapes_inner_quotes() {
        assert_eq!(
            originate_unquote("'&playback(it\\'s a test file)'"),
            "&playback(it's a test file)"
        );
    }

    #[test]
    fn quote_unquote_round_trip() {
        let original = "&socket(127.0.0.1:8040 async full)";
        assert_eq!(originate_unquote(&originate_quote(original)), original);
    }

    #[test]
    fn quote_unquote_round_trip_with_inner_quote() {
        let original = "&playback(it's a test file)";
        assert_eq!(originate_unquote(&originate_quote(original)), original);
    }

    #[test]
    fn split_multiple_consecutive_spaces() {
        let result = originate_split("originate  sofia/test  123", ' ').unwrap();
        // Multiple consecutive spaces produce empty tokens that are trimmed/skipped
        assert_eq!(result[0], "originate");
        assert_eq!(result[1], "sofia/test");
        assert_eq!(result[2], "123");
    }

    #[test]
    fn split_leading_trailing_spaces() {
        let result = originate_split("  originate sofia/test  ", ' ').unwrap();
        assert_eq!(result[0], "originate");
        assert_eq!(result[1], "sofia/test");
    }

    /// Measured on a live switch: `\\'` escapes the backslash, so the quote opens
    /// a region the rest of the line never closes and `originate` answers usage.
    #[test]
    fn split_quote_after_escaped_backslash_opens_a_region() {
        assert!(matches!(
            originate_split(r"originate {v=x\\'y z}loopback/9199/test &park()", ' '),
            Err(OriginateError::UnclosedQuote(_))
        ));
    }

    /// Measured on a live switch: the backslash escapes the space, one argument.
    #[test]
    fn split_escaped_space_does_not_split() {
        assert_eq!(
            originate_split(r"originate {v=a\ b}loopback/9199/test &park()", ' ').unwrap(),
            ["originate", r"{v=a\ b}loopback/9199/test", "&park()"]
        );
    }

    /// Only a space separates, and only a space is trimmed.
    #[test]
    fn split_keeps_a_tab() {
        assert_eq!(
            originate_split("originate x\t y", ' ').unwrap(),
            ["originate", "x\t", "y"]
        );
    }

    /// A comma split toggles on a quote only when another quote follows it, and
    /// keeps the empty token between two separators.
    #[test]
    fn split_on_comma_follows_the_char_delimiter_rules() {
        assert_eq!(originate_split("a'b,c", ',').unwrap(), ["a'b", "c"]);
        assert_eq!(originate_split("a,,b", ',').unwrap(), ["a", "", "b"]);
    }

    #[test]
    fn split_honours_a_leading_argument_separator() {
        assert_eq!(
            originate_split(r"^^~{v=a b}loopback/9199/test~&park()", ' ').unwrap(),
            ["{v=a b}loopback/9199/test", "&park()"]
        );
        assert_eq!(originate_split("^^~a b~c", ',').unwrap(), ["a b", "c"]);
        assert_eq!(originate_split("^^~é", ' ').unwrap(), ["é"]);
    }

    #[test]
    fn split_takes_no_override_without_a_byte_after_it() {
        assert_eq!(originate_split("^^~", ' ').unwrap(), ["^^~"]);
        assert_eq!(originate_split("^^~", ',').unwrap(), ["^^~"]);
    }

    /// The switch splits on the first byte of a non-ASCII separator, which no
    /// char delimiter can mirror, so the default split runs over the whole line.
    #[test]
    fn split_takes_no_override_on_a_non_ascii_separator() {
        assert_eq!(originate_split("^^éaéb c", ' ').unwrap(), ["^^éaéb", "c"]);
        assert_eq!(originate_split("^^éaé,b", ',').unwrap(), ["^^éaé", "b"]);
    }

    #[test]
    fn parse_target_bare_extension() {
        let target = parse_originate_target("123", None).unwrap();
        assert!(matches!(target, OriginateTarget::Extension(ref e) if e == "123"));
    }

    #[test]
    fn parse_target_xml_no_args() {
        let target = parse_originate_target("&conference()", None).unwrap();
        if let OriginateTarget::Application(app) = target {
            assert_eq!(app.name(), "conference");
            assert!(app
                .args()
                .is_none());
        } else {
            panic!("expected Application");
        }
    }

    #[test]
    fn parse_target_xml_with_args() {
        let target = parse_originate_target("&conference(1)", None).unwrap();
        if let OriginateTarget::Application(app) = target {
            assert_eq!(app.name(), "conference");
            assert_eq!(app.args(), Some("1"));
        } else {
            panic!("expected Application");
        }
    }

    #[test]
    fn parse_target_two_inline_apps() {
        let target = parse_originate_target(
            "conference:1,hangup:NORMAL_CLEARING",
            Some(&DialplanType::Inline),
        )
        .unwrap();
        if let OriginateTarget::InlineApplications(apps) = target {
            assert_eq!(apps.len(), 2);
            assert_eq!(apps[0].name(), "conference");
            assert_eq!(apps[0].args(), Some("1"));
            assert_eq!(apps[1].name(), "hangup");
            assert_eq!(apps[1].args(), Some("NORMAL_CLEARING"));
        } else {
            panic!("expected InlineApplications");
        }
    }

    #[test]
    fn parse_target_inline_bare_name() {
        let target = parse_originate_target("hangup", Some(&DialplanType::Inline)).unwrap();
        if let OriginateTarget::InlineApplications(apps) = target {
            assert_eq!(apps.len(), 1);
            assert_eq!(apps[0].name(), "hangup");
            assert!(apps[0]
                .args()
                .is_none());
        } else {
            panic!("expected InlineApplications");
        }
    }

    #[test]
    fn parse_target_inline_mixed_bare_and_args() {
        let target =
            parse_originate_target("park,hangup:NORMAL_CLEARING", Some(&DialplanType::Inline))
                .unwrap();
        if let OriginateTarget::InlineApplications(apps) = target {
            assert_eq!(apps.len(), 2);
            assert_eq!(apps[0].name(), "park");
            assert!(apps[0]
                .args()
                .is_none());
            assert_eq!(apps[1].name(), "hangup");
            assert_eq!(apps[1].args(), Some("NORMAL_CLEARING"));
        } else {
            panic!("expected InlineApplications");
        }
    }

    #[test]
    fn setvar_quoting_escapes_for_the_setvar_tokenizer() {
        let cases: &[(&str, &str)] = &[
            ("PCMU,PCMA", "'PCMU,PCMA'"),
            ("mode-set=0; octet-align=1", "'mode-set=0; octet-align=1'"),
            ("a'b", "'a\\'b'"),
            ("a\\b", "'a\\\\b'"),
        ];
        for (value, expected) in cases {
            assert_eq!(&quote_for_uuid_setvar(value), expected);
        }
    }

    #[test]
    fn parse_target_inline_trailing_colon_collapses_to_none() {
        let target = parse_originate_target("park:", Some(&DialplanType::Inline)).unwrap();
        if let OriginateTarget::InlineApplications(apps) = target {
            assert!(apps[0]
                .args()
                .is_none());
        } else {
            panic!("expected InlineApplications");
        }
    }
}
