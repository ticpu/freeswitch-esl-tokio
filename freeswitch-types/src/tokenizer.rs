//! Port of the switch's own string separator, `separate_string_char_delim` and
//! `cleanup_separated_string` in `switch_utils.c`, for every reader of text the
//! switch tokenizes that way.

use std::iter::Peekable;
use std::str::Chars;

/// Whether a matching close quote lies ahead, mirroring the C `strchr(ptr + 1, '\'')`.
fn has_closing_quote(rest: &Peekable<Chars<'_>>) -> bool {
    rest.clone()
        .any(|c| c == '\'')
}

/// The `\X` expansions `unescape_char` in `switch_utils.c` maps; any other `X` is `None`.
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
pub(crate) fn separate_string_char_delim(s: &str, delim: char) -> Vec<String> {
    let mut raw_tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut inside_quotes = false;
    let mut chars = s
        .chars()
        .peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Backslash and the char it escapes ride into the raw token verbatim;
            // cleanup_separated_string expands them. Skipping ahead only prevents a split.
            if let Some(&next) = chars.peek() {
                chars.next();
                current.push('\\');
                current.push(next);
            } else {
                current.push('\\');
            }
        } else if ch == '\'' {
            // Quote state affects the split point; cleanup strips the quote itself.
            if inside_quotes || has_closing_quote(&chars) {
                inside_quotes = !inside_quotes;
            }
            current.push('\'');
        } else if ch == delim && !inside_quotes {
            raw_tokens.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    raw_tokens.push(current);

    raw_tokens
        .into_iter()
        .map(|t| cleanup_separated_string(&t, delim))
        .collect()
}

/// Apply `cleanup_separated_string` logic to a single raw token.
///
/// - Strips leading spaces (only space, not other whitespace — mirrors the C `' '` check).
/// - Strips trailing spaces outside quotes (via `end` pointer tracking).
/// - Strips `'` quote characters (they are not included in output).
/// - Expands escape sequences.
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

#[cfg(test)]
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
