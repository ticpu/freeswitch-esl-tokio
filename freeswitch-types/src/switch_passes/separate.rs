//! Port of the switch's own string separators in `switch_utils.c`, for every reader
//! of text the switch tokenizes that way.

use std::ops::Range;

#[cfg(feature = "esl")]
use super::{byte_range, extent};
use super::{trace, untrace, Traced};

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
        .take_while(|&&(c, ..)| c == ' ')
        .count();
    &text[count..]
}

/// `strchr` for a quote, which stops at a NUL.
fn has_quote(rest: &[Traced]) -> bool {
    rest.iter()
        .take_while(|&&(c, ..)| c != '\0')
        .any(|&(c, ..)| c == '\'')
}

/// `separate_string_char_delim` before its cleanup, keeping at most `limit` tokens. A trailing
/// delimiter or an empty input yields no token; two adjacent delimiters yield an empty one. A NUL
/// ends the scan, unless a backslash before it steps over it.
pub(crate) fn char_delim(text: &[Traced], delim: char, limit: usize) -> Cut {
    let mut spans = Vec::new();
    let mut begin = None;
    let mut inside_quotes = false;
    #[cfg(feature = "esl")]
    let mut held_delimiter = false;
    let mut stop = text.len();
    let mut i = 0;
    while i < text.len() {
        if text[i].0 == '\0' {
            stop = i;
            break;
        }
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
        spans.push(start..stop);
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
/// toggles with no lookahead, and a run of spaces is one separator. A NUL ends the scan, unless a
/// backslash before it steps over it.
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
    let mut stop = text.len();
    let mut i = 0;
    while i < text.len() {
        let c = text[i].0;
        if c == '\0' {
            stop = i;
            break;
        }
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
        spans.push(begin..stop);
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

/// `cleanup_separated_string`, `delim` being `None` where the switch passes 0.
pub(crate) fn cleanup(raw: &[Traced], delim: Option<char>) -> Vec<Traced> {
    let (mut out, end) = cleanup_written(raw, delim);
    out.truncate(end.unwrap_or(0));
    out
}

/// Every char `cleanup_separated_string` writes from the first past the leading spaces, and where
/// it writes the terminator, `None` where it writes none.
fn cleanup_written(raw: &[Traced], delim: Option<char>) -> (Vec<Traced>, Option<usize>) {
    let s = skip_spaces(raw);
    let mut out = Vec::with_capacity(s.len());
    let mut end = None;
    let mut inside_quotes = false;
    let mut i = 0;
    while i < s.len() {
        let (c, start, _) = s[i];
        let escaped = match (c, s.get(i + 1)) {
            ('\\', Some(&(next, _, next_end))) => {
                unescape(next, delim).map(|e| (e, start, next_end))
            }
            _ => None,
        };
        if let Some(e) = escaped {
            out.push(e);
            end = Some(out.len());
            i += 2;
            continue;
        }
        if c == '\'' && (inside_quotes || has_quote(&s[i + 1..])) {
            inside_quotes = !inside_quotes;
            if inside_quotes {
                end = Some(out.len());
            }
        } else {
            out.push(s[i]);
            if c != ' ' || inside_quotes {
                end = Some(out.len());
            }
        }
        i += 1;
    }
    (out, end)
}

/// A NUL-terminated buffer the switch's splits rewrite in place, a NUL held as `'\0'`.
#[cfg(feature = "esl")]
pub(crate) struct CBuffer(Vec<Traced>);

/// What [`CBuffer::separate`] cut.
#[cfg(feature = "esl")]
pub(crate) struct Split {
    pub(crate) tokens: Vec<SplitToken>,
    /// A quote kept a delimiter from splitting.
    pub(crate) held_delimiter: bool,
    /// A `^^` head named a non-ASCII separator, which the switch takes as a byte; the split ran
    /// on the delimiter given.
    pub(crate) unreadable_head: bool,
}

/// One token of a [`Split`], as indices into its buffer.
#[cfg(feature = "esl")]
pub(crate) struct SplitToken {
    /// The token as the split cut it, before its cleanup.
    pub(crate) raw: Range<usize>,
    /// Where the cleaned token starts.
    pub(crate) start: usize,
}

#[cfg(feature = "esl")]
impl CBuffer {
    /// `text` and its terminator, and a second NUL for a trailing backslash to step onto.
    pub(crate) fn new(text: &[Traced]) -> Self {
        let end = extent(text).end;
        let mut chars = text.to_vec();
        chars.extend([('\0', end, end), ('\0', end, end)]);
        Self(chars)
    }

    pub(crate) fn at(&self, index: usize) -> char {
        self.0
            .get(index)
            .map_or('\0', |&(c, ..)| c)
    }

    pub(crate) fn terminate(&mut self, index: usize) {
        if let Some(c) = self
            .0
            .get_mut(index)
        {
            c.0 = '\0';
        }
    }

    /// The string at `index`, up to its terminator.
    pub(crate) fn c_str(&self, index: usize) -> &[Traced] {
        let rest = self
            .0
            .get(index..)
            .unwrap_or_default();
        let len = rest
            .iter()
            .position(|&(c, ..)| c == '\0')
            .unwrap_or(rest.len());
        &rest[..len]
    }

    pub(crate) fn c_str_mut(&mut self, index: usize) -> &mut [Traced] {
        let len = self
            .c_str(index)
            .len();
        &mut self.0[index..index + len]
    }

    pub(crate) fn into_chars(self) -> Vec<Traced> {
        self.0
    }

    /// `switch_separate_string` on the string at `index`, in place.
    pub(crate) fn separate(&mut self, index: usize, delim: char, limit: usize) -> Split {
        let (mut buf, mut delim, mut unreadable_head) = (index, delim, false);
        if self.at(buf) == '^' && self.at(buf + 1) == '^' && self.at(buf + 2) != '\0' {
            if !self
                .at(buf + 2)
                .is_ascii()
            {
                unreadable_head = true;
            } else if self.at(buf + 3) != '\0' {
                delim = self.at(buf + 2);
                buf += 3;
            }
        }
        let text = &self.0[buf.min(
            self.0
                .len(),
        )..];
        let cut = match delim {
            ' ' => blank_delim(text, limit),
            delim => char_delim(text, delim, limit),
        };
        for span in &cut.spans {
            if self.at(buf + span.end) == delim {
                self.terminate(buf + span.end);
            }
        }
        let cleanup_delim = (delim != ' ').then_some(delim);
        let tokens = cut
            .spans
            .iter()
            .map(|span| SplitToken {
                raw: buf + span.start..buf + span.end,
                start: self.cleanup(buf + span.start, cleanup_delim),
            })
            .collect();
        Split {
            tokens,
            held_delimiter: cut.held_delimiter,
            unreadable_head,
        }
    }

    /// `cleanup_separated_string` on the string at `index`, written back: where the result starts.
    fn cleanup(&mut self, index: usize, delim: Option<char>) -> usize {
        let raw = self
            .c_str(index)
            .to_vec();
        let start = index + (raw.len() - skip_spaces(&raw).len());
        let (written, end) = cleanup_written(&raw, delim);
        self.0[start..start + written.len()].copy_from_slice(&written);
        if let Some(end) = end {
            self.terminate(start + end);
        }
        start
    }
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

/// The delimiter a leading `^^X` picks and the text after it, as `switch_separate_string`
/// reads one: `X` then at least one byte.
///
/// The switch takes `X` as one byte, so a non-ASCII `X` splits on the first byte of its
/// UTF-8 form. No char delimiter mirrors that, and such a prefix picks nothing here.
#[cfg(feature = "esl")]
pub(crate) fn delimiter_override(text: &[Traced]) -> (Option<char>, &[Traced]) {
    match text {
        [('^', ..), ('^', ..), (picked, ..), rest @ ..]
            if picked.is_ascii() && !rest.is_empty() =>
        {
            (Some(*picked), rest)
        }
        _ => (None, text),
    }
}

/// `switch_separate_string`: a `^^X` prefix [`delimiter_override`] accepts picks `X` over
/// `delim`, a space splits blank and anything else by char.
#[cfg(feature = "esl")]
pub(crate) fn separate(text: &[Traced], delim: char, limit: usize) -> Separated {
    let (picked, body) = delimiter_override(text);
    let skipped = text.len() - body.len();
    let mut separated = separate_on(body, picked.unwrap_or(delim), limit);
    for token in &mut separated.tokens {
        token.raw = token
            .raw
            .start
            + skipped
            ..token
                .raw
                .end
                + skipped;
    }
    separated
}

/// [`separate`] on `delim` with no `^^X` prefix read, for text not at the head of a
/// switch argument.
#[cfg(feature = "esl")]
pub(crate) fn separate_on(body: &[Traced], delim: char, limit: usize) -> Separated {
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
                raw: span,
            })
            .collect(),
        held_delimiter: cut.held_delimiter,
        open_quote: cut.open_quote,
    }
}

/// A split that must leave its text whole cut it, or a quote held a delimiter.
#[cfg(feature = "esl")]
pub(crate) struct ArgvCut;

/// The one token `originate`'s split on `delim` leaves of `text`, or `None` for an empty text.
/// On a space that split is [`separate`], where a quote may hold a space but not stay open.
#[cfg(feature = "esl")]
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
    for (i, &(c, ..)) in text
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
            .map(|&(c, ..)| c)
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

/// The raw tokens `separate_string_char_delim` cuts from `text`, traced from `s`, before
/// any cleanup.
#[cfg(feature = "esl")]
pub(crate) fn char_delim_spans<'s>(s: &'s str, text: &[Traced], delim: char) -> Vec<&'s str> {
    char_delim(text, delim, usize::MAX)
        .spans
        .into_iter()
        .map(|span| &s[byte_range(text, extent(text), span)])
        .collect()
}

/// The raw tokens `separate_string_blank_delim` cuts on spaces from `text`, traced from
/// `s`, before any cleanup, and whether a quote was still open at the end.
#[cfg(feature = "esl")]
pub(crate) fn blank_delim_spans<'s>(s: &'s str, text: &[Traced]) -> (Vec<&'s str>, bool) {
    let cut = blank_delim(text, usize::MAX);
    let spans = cut
        .spans
        .into_iter()
        .map(|span| &s[byte_range(text, extent(text), span)])
        .collect();
    (spans, cut.open_quote)
}

#[cfg(all(test, feature = "esl"))]
mod c_oracle;

#[cfg(all(test, feature = "esl"))]
mod tiling {
    use super::*;
    use crate::test_text::TILING_INPUTS;

    #[test]
    fn tokens_meet_only_at_their_delimiters() {
        for input in TILING_INPUTS {
            let text = trace(input);
            for delim in [' ', ',', '|', '='] {
                let ranges: Vec<_> = separate(&text, delim, usize::MAX)
                    .tokens
                    .into_iter()
                    .map(|token| byte_range(&text, extent(&text), token.raw))
                    .collect();
                let is_delimiter = |gap: &str| match delim {
                    ' ' => {
                        !gap.is_empty()
                            && gap
                                .chars()
                                .all(|c| c == ' ')
                    }
                    _ => gap.len() == 1 && gap.starts_with(delim),
                };
                let context = format!("{input:?} on {delim:?}: {ranges:?}");
                let mut at = 0;
                for (k, range) in ranges
                    .iter()
                    .enumerate()
                {
                    let gap = &input[at..range.start];
                    assert!(
                        if k == 0 {
                            gap.is_empty()
                        } else {
                            is_delimiter(gap)
                        },
                        "{context}"
                    );
                    at = range.end;
                }
                let rest = &input[at..];
                assert!(rest.is_empty() || is_delimiter(rest), "{context}");
            }
        }
    }
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
