//! Port of the split `switch_api_execute` and `originate_function` run over an API command's
//! argument line, and the escapes that keep a text one argument of it.

use std::borrow::Cow;
use std::fmt::{self, Write as _};
use std::ops::Range;

use super::separate::{
    cleanup, escape_inside_token, separate, separate_on, takes_a_backslash, Token,
};
use super::{byte_range, extent, trace, untrace, Traced};
use crate::commands::originate::OriginateError;

#[cfg(test)]
mod tests;

/// What `switch_strip_whitespace` strips from both edges of an API command's argument line.
pub(crate) const STRIPPED_WHITESPACE: [char; 5] = ['\t', '\n', '\u{b}', '\r', ' '];

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

/// Space, controls, non-ASCII, `\`, `'` and lowercase `n r t s`, which break a split on `sep` or
/// the escapes its cleanup reads.
pub(crate) fn breaks_a_split(sep: char) -> bool {
    !sep.is_ascii_graphic() || matches!(sep, '\\' | '\'' | 'n' | 'r' | 't' | 's')
}

/// Not [`breaks_a_split`]; as policy, not what reads as dial-string grammar, quoting or a word.
pub(crate) fn usable_argv_separator(sep: char) -> bool {
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
                c if takes_a_backslash(c, Some(self.sep)) => {
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

/// `text` escaped as one argument of the split on `sep`: an empty text is written `''`, and on
/// blanks a text opening `^^` follows `''`, since a line opening `^^` names its own separator.
pub(crate) fn escape_argument(text: &str, sep: char) -> Option<Cow<'_, str>> {
    if text.is_empty() {
        return Some(Cow::Borrowed("''"));
    }
    let guarded = sep == ' ' && text.starts_with("^^");
    let plain = !guarded
        && !text.starts_with(STRIPPED_WHITESPACE)
        && !text.ends_with(STRIPPED_WHITESPACE)
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

/// A split that must leave its text whole cut it, or a quote held a delimiter.
pub(crate) struct ArgvCut;

/// The one token `originate`'s split on `delim` leaves of `text`, or `None` for an empty text.
/// On a space that split is [`separate`], where a quote may hold a space but not stay open.
pub(crate) fn sole_argument(text: &[Traced], delim: char) -> Result<Option<Token>, ArgvCut> {
    let (separated, quote_cuts) = match delim {
        ' ' => {
            let separated = separate(text, ' ', usize::MAX);
            let open = separated.open_quote;
            (separated, open)
        }
        delim => {
            let separated = separate_on(text, delim, usize::MAX);
            let held = separated.held_delimiter;
            (separated, held)
        }
    };
    if quote_cuts
        || separated
            .tokens
            .len()
            > 1
    {
        return Err(ArgvCut);
    }
    Ok(separated
        .tokens
        .into_iter()
        .next())
}

/// `value` escaped and single-quoted for the blank split's cleanup.
pub(crate) fn quote_for_uuid_setvar(value: &str) -> String {
    format!("'{}'", escape_inside_token(value, None))
}

/// `token` quoted as [`quote_for_uuid_setvar`] quotes it when empty or carrying what the blank
/// split, its cleanup or the line strip reads, else as it stands.
pub(crate) fn originate_quote(token: &str) -> String {
    if token.is_empty() || token.contains(STRIPPED_WHITESPACE) || token.contains(['\'', '\\']) {
        quote_for_uuid_setvar(token)
    } else {
        token.to_string()
    }
}

/// What the switch's cleanup after splitting on `sep`, or on blanks, leaves of `token`.
pub(crate) fn clean_argument(token: &str, sep: Option<char>) -> String {
    untrace(&cleanup(&trace(token), sep))
}

/// One argument of [`split_line`]: the bytes of the line it came from, and what the split's
/// cleanup leaves of it.
pub(crate) struct Argument {
    pub(crate) raw: Range<usize>,
    pub(crate) text: String,
}

/// What [`split_line`] cut.
pub(crate) struct ArgumentLine {
    pub(crate) arguments: Vec<Argument>,
    /// The delimiter the split ran on, a `^^` head's pick included.
    pub(crate) delimiter: char,
}

/// `originate`'s split of `line` on `split_at` or the delimiter a `^^` head picks. A blank split's
/// raw argument starts past its leading spaces, and a quote it leaves open is refused.
pub(crate) fn split_line(line: &str, split_at: char) -> Result<ArgumentLine, OriginateError> {
    let text = trace(line);
    let separated = separate(&text, split_at, usize::MAX);
    let blank = separated.delimiter == ' ';
    let arguments: Vec<Argument> = separated
        .tokens
        .into_iter()
        .map(|token| {
            let raw = byte_range(&text, extent(&text), token.raw);
            let leading = match blank {
                true => {
                    line[raw.clone()].len()
                        - line[raw.clone()]
                            .trim_start_matches(' ')
                            .len()
                }
                false => 0,
            };
            Argument {
                raw: raw.start + leading..raw.end,
                text: untrace(&token.text),
            }
        })
        .collect();
    if blank && separated.open_quote {
        let last = arguments
            .last()
            .map_or("", |argument| {
                &line[argument
                    .raw
                    .clone()]
            });
        return Err(OriginateError::UnclosedQuote(last.to_string()));
    }
    Ok(ArgumentLine {
        arguments,
        delimiter: separated.delimiter,
    })
}
