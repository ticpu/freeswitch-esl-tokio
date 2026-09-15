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

#[cfg(all(test, feature = "serde"))]
mod proptests;

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
    AudioEndpoint, DialString, EndpointDisplay, EndpointFieldFault, ErrorEndpoint, GroupCall,
    GroupCallOrder, LoopbackEndpoint, ParseGroupCallOrderError, SofiaContact, SofiaEndpoint,
    SofiaGateway, UserEndpoint,
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

use crate::switch_passes::api_argument::{self, clean_argument, split_line};
use crate::switch_passes::separate::separate;
use crate::switch_passes::{trace, untrace};
use originate::{check_inline_delimiter, check_target_readable, DEFAULT_INLINE_DELIMITER};

/// Wrap a token in single quotes for originate command strings.
///
/// A token that is empty or carries a space, single quote, backslash, or a tab, vertical tab, CR
/// or newline, which the API strips from the edges of its argument line, is escaped and wrapped
/// as [`quote_for_uuid_setvar`] does, since that command splits on the same blank tokenizer;
/// any other token is returned as-is.
pub fn originate_quote(token: &str) -> String {
    api_argument::originate_quote(token)
}

/// Escape and single-quote a value for the argument string of `uuid_setvar`.
///
/// That command splits its arguments on spaces through `cleanup_separated_string`,
/// which honours `'` grouping and processes `\` escapes inside the quoted region,
/// so an unquoted value is silently truncated at its first space and a bare `'`
/// or `\` inside the quotes ends or eats a character. The inline originate
/// `{var=…}` block is a different carrier with different escaping.
pub fn quote_for_uuid_setvar(value: &str) -> String {
    api_argument::quote_for_uuid_setvar(value)
}

/// What the switch delivers of one token of the blank split: its quotes stripped and its
/// escapes read, the exact inverse of [`originate_quote`].
pub fn originate_unquote(token: &str) -> String {
    clean_argument(token, None)
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
    Ok(split_line(line, split_at)?
        .arguments
        .into_iter()
        .map(|argument| line[argument.raw].to_string())
        .collect())
}

/// Split an `m:<delim>:` prefix off an inline action list.
///
/// `inline_dialplan_hunt` reads exactly four bytes for this, so anything longer
/// or shorter is part of the first application rather than a prefix.
pub(crate) fn split_inline_prefix(s: &str) -> (Option<char>, &str) {
    let bytes = s.as_bytes();
    match bytes {
        [b'm', b':', delimiter, b':', ..] if delimiter.is_ascii() => {
            (Some(*delimiter as char), &s[4..])
        }
        _ => (None, s),
    }
}

/// `argv` in `inline_dialplan_hunt`: the most actions its split keeps.
const INLINE_ACTIONS: usize = 128;

/// The actions `inline_dialplan_hunt` splits an action list into, each through its cleanup.
fn split_inline_actions(s: &str, delimiter: char) -> Vec<String> {
    separate(&trace(s), delimiter, INLINE_ACTIONS)
        .tokens
        .iter()
        .map(|token| untrace(&token.text))
        .collect()
}

/// Parse the target argument of an originate command.
///
/// Determines whether the target is a dialplan extension or application(s), in the order
/// `originate_function` decides it:
///
/// - If string is `&` and more: parse as XML app → `Application`, whatever the dialplan
/// - If dialplan is `Inline`: parse as inline apps → `InlineApplications`
/// - Otherwise: bare string → `Extension`
pub fn parse_originate_target(
    s: &str,
    dialplan: Option<&DialplanType>,
) -> Result<OriginateTarget, OriginateError> {
    if let Some(rest) = s
        .strip_prefix('&')
        .filter(|rest| !rest.is_empty())
    {
        let rest = rest
            .strip_suffix(')')
            .ok_or_else(|| OriginateError::ParseError("missing closing paren".into()))?;
        let (name, args) = rest
            .split_once('(')
            .ok_or_else(|| OriginateError::ParseError("missing opening paren".into()))?;
        let args = if args.is_empty() { None } else { Some(args) };
        let target = OriginateTarget::Application(Application::new(name, args));
        check_target_readable(&target)?;
        Ok(target)
    } else if matches!(dialplan, Some(DialplanType::Inline)) {
        let (delimiter, s) = split_inline_prefix(s);
        let delimiter = delimiter.unwrap_or(DEFAULT_INLINE_DELIMITER);
        check_inline_delimiter(delimiter)?;
        let mut apps = Vec::new();
        for action in split_inline_actions(s, delimiter) {
            let part = action.trim_start_matches(' ');
            let (name, args) = match part.split_once(':') {
                Some((n, "")) => (n, None),
                Some((n, a)) => (n, Some(a)),
                None => (part, None),
            };
            apps.push(Application::new(name, args));
        }
        Ok(OriginateTarget::InlineApplications(apps))
    } else {
        Ok(OriginateTarget::Extension(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `originate_function` runs `&name(args)` before it reads the dialplan, and takes a lone
    /// `&` as an extension.
    #[test]
    fn parse_target_reads_an_application_before_the_dialplan() {
        for dialplan in [None, Some(&DialplanType::Inline), Some(&DialplanType::Xml)] {
            let target = parse_originate_target("&park(:,)", dialplan).unwrap();
            assert_eq!(
                target,
                OriginateTarget::Application(Application::new("park", Some(":,"))),
                "{dialplan:?}"
            );
        }
        assert_eq!(
            parse_originate_target("&", None).unwrap(),
            OriginateTarget::Extension("&".into())
        );
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
