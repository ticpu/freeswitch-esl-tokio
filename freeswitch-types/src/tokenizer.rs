//! Port of the switch's own string separators in `switch_utils.c`, for every reader
//! of text the switch tokenizes that way.
//!
//! Every pass runs over traced text: each char paired with the byte offset in the
//! original input it came from, so a cleaned token still points back at its source.

use std::ops::Range;

/// A char and its byte offset in the original input.
pub(crate) type Traced = (char, usize);

pub(crate) fn trace(s: &str) -> Vec<Traced> {
    s.char_indices()
        .map(|(at, c)| (c, at))
        .collect()
}

pub(crate) fn untrace(text: &[Traced]) -> String {
    text.iter()
        .map(|&(c, _)| c)
        .collect()
}

/// The byte range in the original input a raw token covers.
#[cfg(feature = "esl")]
pub(crate) fn byte_range(text: &[Traced], span: Range<usize>) -> Range<usize> {
    let end_of_text = text
        .last()
        .map_or(0, |&(c, at)| at + c.len_utf8());
    let start = text
        .get(span.start)
        .map_or(end_of_text, |&(_, at)| at);
    match text[span].last() {
        Some(&(c, at)) => start..at + c.len_utf8(),
        None => start..start,
    }
}

/// Raw token spans one split cuts, as index ranges into the text it split.
pub(crate) struct Cut {
    pub(crate) spans: Vec<Range<usize>>,
    /// A quote kept a delimiter from splitting.
    #[cfg(feature = "esl")]
    pub(crate) held_delimiter: bool,
    /// A quote was still open at the end of the text.
    #[cfg(feature = "esl")]
    pub(crate) open_quote: bool,
}

/// The text after its leading spaces; only a space counts.
pub(crate) fn skip_spaces(text: &[Traced]) -> &[Traced] {
    let count = text
        .iter()
        .take_while(|&&(c, _)| c == ' ')
        .count();
    &text[count..]
}

fn has_quote(rest: &[Traced]) -> bool {
    rest.iter()
        .any(|&(c, _)| c == '\'')
}

/// `separate_string_char_delim` before its cleanup, keeping at most `limit` tokens. A trailing
/// delimiter or an empty input yields no token; two adjacent delimiters yield an empty one.
pub(crate) fn char_delim(text: &[Traced], delim: char, limit: usize) -> Cut {
    let mut spans = Vec::new();
    let mut begin = None;
    let mut inside_quotes = false;
    #[cfg(feature = "esl")]
    let mut held_delimiter = false;
    let mut i = 0;
    while i < text.len() {
        let start = match begin {
            Some(start) => start,
            None if spans.len() + 1 >= limit => {
                begin = Some(i);
                break;
            }
            None => *begin.insert(i),
        };
        let c = text[i].0;
        if c == '\\' {
            i += 1;
        } else if c == '\'' && (inside_quotes || has_quote(&text[i + 1..])) {
            inside_quotes = !inside_quotes;
        } else if c == delim && !inside_quotes {
            spans.push(start..i);
            begin = None;
        } else if c == delim {
            #[cfg(feature = "esl")]
            {
                held_delimiter = true;
            }
        }
        i += 1;
    }
    if let Some(start) = begin {
        spans.push(start..text.len());
    }
    Cut {
        spans,
        #[cfg(feature = "esl")]
        held_delimiter,
        #[cfg(feature = "esl")]
        open_quote: inside_quotes,
    }
}

/// `separate_string_blank_delim` before its cleanup, keeping at most `limit` tokens. A quote
/// toggles with no lookahead, and a run of spaces is one separator.
#[cfg(feature = "esl")]
pub(crate) fn blank_delim(text: &[Traced], limit: usize) -> Cut {
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
    let mut held_delimiter = false;
    let mut i = 0;
    while i < text.len() {
        let c = text[i].0;
        match state {
            State::Start if spans.len() + 1 >= limit => {
                begin = i;
                state = State::FindDelim;
                break;
            }
            State::Start => {
                begin = i;
                state = State::SkipInitialSpace;
            }
            State::SkipInitialSpace | State::SkipEndingSpace if c == ' ' => i += 1,
            State::SkipInitialSpace => state = State::FindDelim,
            State::SkipEndingSpace => state = State::Start,
            State::FindDelim => {
                if c == '\\' {
                    i += 1;
                } else if c == '\'' {
                    inside_quotes = !inside_quotes;
                } else if c == ' ' && !inside_quotes {
                    spans.push(begin..i);
                    state = State::SkipEndingSpace;
                } else if c == ' ' {
                    held_delimiter = true;
                }
                i += 1;
            }
        }
    }
    if matches!(state, State::SkipInitialSpace | State::FindDelim) {
        spans.push(begin..text.len());
    }
    Cut {
        spans,
        held_delimiter,
        open_quote: inside_quotes,
    }
}

/// The char `\next` stands for in `cleanup_separated_string`, or `None` when the
/// backslash is kept.
fn unescape(next: char, delim: Option<char>) -> Option<char> {
    if Some(next) == delim || matches!(next, '\'' | '"' | '\\') {
        return Some(next);
    }
    match next {
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        's' => Some(' '),
        _ => None,
    }
}

/// `cleanup_separated_string`, `delim` being `None` where the switch passes 0. Each kept char
/// keeps its trace.
pub(crate) fn cleanup(raw: &[Traced], delim: Option<char>) -> Vec<Traced> {
    let s = skip_spaces(raw);
    let mut out = Vec::with_capacity(s.len());
    let mut end = 0;
    let mut inside_quotes = false;
    let mut i = 0;
    while i < s.len() {
        let (c, at) = s[i];
        let escaped = match (c, s.get(i + 1)) {
            ('\\', Some(&(next, _))) => unescape(next, delim),
            _ => None,
        };
        if let Some(e) = escaped {
            out.push((e, at));
            end = out.len();
            i += 2;
            continue;
        }
        if c == '\'' && (inside_quotes || has_quote(&s[i + 1..])) {
            inside_quotes = !inside_quotes;
            if inside_quotes {
                end = out.len();
            }
        } else {
            out.push((c, at));
            if c != ' ' || inside_quotes {
                end = out.len();
            }
        }
        i += 1;
    }
    out.truncate(end);
    out
}

/// One token of [`separate`]: its raw span in the text given, and its cleaned text.
#[cfg(feature = "esl")]
pub(crate) struct Token {
    pub(crate) raw: Range<usize>,
    pub(crate) text: Vec<Traced>,
}

#[cfg(feature = "esl")]
pub(crate) struct Separated {
    pub(crate) tokens: Vec<Token>,
    pub(crate) held_delimiter: bool,
    pub(crate) open_quote: bool,
}

/// `switch_separate_string`: a `^^X` prefix with at least one char after `X` picks
/// `X` as the delimiter, a space splits blank and anything else by char.
#[cfg(feature = "esl")]
pub(crate) fn separate(text: &[Traced], delim: char, limit: usize) -> Separated {
    let (skipped, delim) = match text {
        [('^', _), ('^', _), (picked, _), _, ..] => (3, *picked),
        _ => (0, delim),
    };
    let body = &text[skipped..];
    let (cut, cleanup_delim) = match (limit, delim) {
        (0, _) => (
            Cut {
                spans: Vec::new(),
                held_delimiter: false,
                open_quote: false,
            },
            None,
        ),
        (_, ' ') => (blank_delim(body, limit), None),
        (_, delim) => (char_delim(body, delim, limit), Some(delim)),
    };
    Separated {
        tokens: cut
            .spans
            .into_iter()
            .map(|span| Token {
                text: cleanup(&body[span.clone()], cleanup_delim),
                raw: span.start + skipped..span.end + skipped,
            })
            .collect(),
        held_delimiter: cut.held_delimiter,
        open_quote: cut.open_quote,
    }
}

/// `switch_find_end_paren`: the index of the close matching an opener that follows
/// any leading spaces. No escape is honoured.
#[cfg(feature = "esl")]
pub(crate) fn find_end_paren(text: &[Traced], open: char, close: char) -> Option<usize> {
    let skip = text.len() - skip_spaces(text).len();
    if text
        .get(skip)?
        .0
        != open
    {
        return None;
    }
    let mut depth = 1usize;
    for (i, &(c, _)) in text
        .iter()
        .enumerate()
        .skip(skip + 1)
    {
        if c == open && open != close {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Where `needle` first occurs in `text`, as `strstr` finds it.
#[cfg(feature = "esl")]
pub(crate) fn find(text: &[Traced], needle: &str) -> Option<usize> {
    let needle: Vec<char> = needle
        .chars()
        .collect();
    let last = text
        .len()
        .checked_sub(needle.len())?;
    (0..=last).find(|&i| {
        text[i..i + needle.len()]
            .iter()
            .map(|&(c, _)| c)
            .eq(needle
                .iter()
                .copied())
    })
}

/// `switch_separate_string_string`: a plain substring split with no quote or escape
/// handling, keeping at most `limit` tokens.
#[cfg(feature = "esl")]
pub(crate) fn separate_string_string(
    text: &[Traced],
    delim: &str,
    limit: usize,
) -> Vec<Range<usize>> {
    let width = delim
        .chars()
        .count();
    let mut spans = Vec::new();
    let mut start = 0;
    while spans.len() + 1 < limit {
        let Some(at) = find(&text[start..], delim) else {
            break;
        };
        spans.push(start..start + at);
        start += at + width;
    }
    spans.push(start..text.len());
    spans
}

/// `separate_string_char_delim` on a non-space `delim`, each token through [`cleanup`].
#[cfg(feature = "sdp")]
pub(crate) fn separate_string_char_delim(s: &str, delim: char) -> Vec<String> {
    let text = trace(s);
    char_delim(&text, delim, usize::MAX)
        .spans
        .into_iter()
        .map(|span| untrace(&cleanup(&text[span], Some(delim))))
        .collect()
}

/// The raw tokens `separate_string_char_delim` cuts, before any cleanup.
#[cfg(feature = "esl")]
pub(crate) fn char_delim_spans(s: &str, delim: char) -> Vec<&str> {
    let text = trace(s);
    char_delim(&text, delim, usize::MAX)
        .spans
        .into_iter()
        .map(|span| &s[byte_range(&text, span)])
        .collect()
}

/// The raw tokens `separate_string_blank_delim` cuts on spaces, before any cleanup,
/// and whether a quote was still open at the end.
#[cfg(feature = "esl")]
pub(crate) fn blank_delim_spans(s: &str) -> (Vec<&str>, bool) {
    let text = trace(s);
    let cut = blank_delim(&text, usize::MAX);
    let spans = cut
        .spans
        .into_iter()
        .map(|span| &s[byte_range(&text, span)])
        .collect();
    (spans, cut.open_quote)
}

#[cfg(all(test, feature = "sdp"))]
mod tests {
    use super::*;

    #[test]
    fn odd_quote_count_before_comma_still_splits() {
        // A quote toggles only with a partner ahead, so a third, unpaired one
        // leaves the comma free to split.
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
