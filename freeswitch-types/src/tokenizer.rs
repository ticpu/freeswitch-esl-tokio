//! Port of the switch's own string separator, `separate_string_char_delim` and
//! `cleanup_separated_string` in `switch_utils.c`, for every reader of text the
//! switch tokenizes that way.

#[cfg(feature = "sdp")]
use std::iter::Peekable;
#[cfg(feature = "sdp")]
use std::str::Chars;

/// Whether a matching close quote lies ahead, mirroring the C `strchr(ptr + 1, '\'')`.
#[cfg(feature = "sdp")]
fn has_closing_quote(rest: &Peekable<Chars<'_>>) -> bool {
    rest.clone()
        .any(|c| c == '\'')
}

/// The `\X` expansions `unescape_char` in `switch_utils.c` maps; any other `X` is `None`.
#[cfg(feature = "sdp")]
fn unescape_char(escaped: char) -> Option<char> {
    match escaped {
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        's' => Some(' '),
        _ => None,
    }
}

/// Split on a non-space `delim` and apply `cleanup_separated_string` to each token.
///
/// Faithfully ports `separate_string_char_delim` + `cleanup_separated_string` from
/// `switch_utils.c`. For the split step, `\` before `delim` prevents splitting and `'`
/// quote-toggling keeps the current token intact through a `delim` inside quotes.
/// Then for each token, leading spaces are stripped, trailing spaces (outside quotes)
/// are dropped, `'` is toggled (and stripped from output), and escape sequences are
/// expanded: `\'`→`'`, `\"`→`"`, `\<delim>`→`<delim>`, `\\`→`\`, `\n`→LF, `\r`→CR,
/// `\t`→TAB, `\s`→space; any other `\X` passes through as `\X`.
#[cfg(feature = "sdp")]
pub(crate) fn separate_string_char_delim(s: &str, delim: char) -> Vec<String> {
    char_delim_spans(s, delim)
        .into_iter()
        .map(|t| cleanup_separated_string(t, delim))
        .collect()
}

/// The raw tokens `separate_string_char_delim` cuts, before any cleanup.
///
/// A token starts only at a character, so a trailing delimiter and an empty input
/// yield no token, while two adjacent delimiters yield an empty one.
pub(crate) fn char_delim_spans(s: &str, delim: char) -> Vec<&str> {
    let mut spans = Vec::new();
    let mut begin = None;
    let mut inside_quotes = false;
    let mut chars = s.char_indices();
    while let Some((i, ch)) = chars.next() {
        let start = *begin.get_or_insert(i);
        if ch == '\\' {
            chars.next();
        } else if ch == '\'' && (inside_quotes || s[i + 1..].contains('\'')) {
            inside_quotes = !inside_quotes;
        } else if ch == delim && !inside_quotes {
            spans.push(&s[start..i]);
            begin = None;
        }
    }
    if let Some(start) = begin {
        spans.push(&s[start..]);
    }
    spans
}

/// The raw tokens `separate_string_blank_delim` cuts on spaces, before any cleanup,
/// and whether a quote was still open at the end.
///
/// Unlike the char delimiter, a quote toggles with no lookahead, and a run of
/// spaces is one separator.
#[cfg(feature = "esl")]
pub(crate) fn blank_delim_spans(s: &str) -> (Vec<&str>, bool) {
    enum State {
        Start,
        SkipInitialSpace,
        FindDelim,
        SkipEndingSpace,
    }

    let mut spans = Vec::new();
    let mut state = State::Start;
    let mut begin = 0;
    let mut inside_quotes = false;
    let mut chars = s.char_indices();
    let mut current = chars.next();
    while let Some((i, ch)) = current {
        match state {
            // START and the space skips hand the character they stop on to the
            // next state rather than consuming it.
            State::Start => {
                begin = i;
                state = State::SkipInitialSpace;
                continue;
            }
            State::SkipInitialSpace if ch != ' ' => {
                state = State::FindDelim;
                continue;
            }
            State::SkipEndingSpace if ch != ' ' => {
                state = State::Start;
                continue;
            }
            State::SkipInitialSpace | State::SkipEndingSpace => {}
            State::FindDelim => {
                if ch == '\\' {
                    chars.next();
                } else if ch == '\'' {
                    inside_quotes = !inside_quotes;
                } else if ch == ' ' && !inside_quotes {
                    spans.push(&s[begin..i]);
                    state = State::SkipEndingSpace;
                }
            }
        }
        current = chars.next();
    }
    if matches!(state, State::SkipInitialSpace | State::FindDelim) {
        spans.push(&s[begin..]);
    }
    (spans, inside_quotes)
}

/// Apply `cleanup_separated_string` logic to a single raw token.
///
/// - Strips leading spaces (only space, not other whitespace — mirrors the C `' '` check).
/// - Strips trailing spaces outside quotes (via `end` pointer tracking).
/// - Strips `'` quote characters (they are not included in output).
/// - Expands escape sequences.
#[cfg(feature = "sdp")]
fn cleanup_separated_string(raw: &str, delim: char) -> String {
    let mut out = String::new();
    // `end_len` tracks the length of `out` at the last non-trailing-space position.
    let mut end_len: usize = 0;
    let mut inside_quotes = false;

    // Skip leading spaces (C: `for (ptr = str; *ptr == ' '; ++ptr)`).
    let s = raw.trim_start_matches(' ');

    let mut chars = s
        .chars()
        .peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let expanded = chars
                .peek()
                .and_then(|&next| {
                    if next == '\'' || next == '"' || next == delim || next == '\\' {
                        Some(next)
                    } else {
                        unescape_char(next)
                    }
                });
            match expanded {
                Some(e) => {
                    chars.next();
                    out.push(e);
                    end_len = out.len();
                }
                // Unrecognized escape: upstream reprocesses the next char, so a
                // following space still trims.
                None => {
                    out.push('\\');
                    end_len = out.len();
                }
            }
        } else if ch == '\'' {
            if inside_quotes || has_closing_quote(&chars) {
                inside_quotes = !inside_quotes;
                // Quote char is NOT output; only update end_len when entering quotes.
                if inside_quotes {
                    end_len = out.len();
                }
            } else {
                // No matching close quote: output the quote literally.
                out.push('\'');
                end_len = out.len();
            }
        } else {
            out.push(ch);
            // Update end tracker when the char is not a trailing space.
            if ch != ' ' || inside_quotes {
                end_len = out.len();
            }
        }
    }

    // Truncate to end_len to strip trailing spaces.
    out.truncate(end_len);
    out
}

#[cfg(all(test, feature = "sdp"))]
mod tests {
    use super::*;

    #[test]
    fn odd_quote_count_before_comma_still_splits() {
        // Each ' toggles only when a closing ' is ahead (C: strchr(ptr+1, '\'')),
        // never by scanning what's already been consumed. Three quotes then a
        // comma must still split into two raw tokens; the first token's content
        // (an unterminated quote survives as a literal char, same as the C
        // cleanup) is a separate, orthogonal name-validation concern.
        let tokens = separate_string_char_delim("a'b'c'd,PCMA", ',');
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1], "PCMA");
    }

    #[test]
    fn escape_of_delimiter_follows_the_delimiter() {
        assert_eq!(
            separate_string_char_delim(r"a\|b|c\,d", '|'),
            vec!["a|b".to_string(), r"c\,d".to_string()]
        );
    }
}
