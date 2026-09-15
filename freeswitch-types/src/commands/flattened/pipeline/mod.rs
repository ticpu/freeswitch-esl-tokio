//! Port of the passes a dial string meets between a carrier and the channel:
//! the carrier's own pass, `switch_ivr_originate`'s thread and leg splits, and
//! `switch_event_create_brackets` for every block.

use std::ops::Range;

use super::CauseReading;
use crate::channel::HangupCause;
use crate::commands::variables::{BlockParse, DialStringCarrier, DialStringTarget};
use crate::tokenizer::{
    byte_range, find, find_end_paren, separate, separate_string_string, skip_spaces, trace,
    untrace, Traced,
};

#[cfg(test)]
mod tests;

/// `MAX_PEERS` in `switch_ivr_originate.c`: the most threads, groups or legs a
/// split keeps.
pub(crate) const MAX_PEERS: usize = 128;

/// `SWITCH_ENT_ORIGINATE_DELIM`.
const ENTERPRISE_DELIM: &str = ":_:";
/// What `switch_ivr_originate` turns a `[]` block's comma into before the leg split.
const QUOTED_ESC_COMMA: char = '\u{1}';
const UNQUOTED_ESC_COMMA: char = '\u{2}';
/// `var_array` in `switch_event_create_brackets`.
const BLOCK_PAIRS: usize = 1024;

/// What installing one pair of a block does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PairEffect {
    /// A value is set.
    Set(String),
    /// The pair has no value by the `=` split, so nothing is installed.
    Ignored,
    /// An empty value is installed, which deletes an earlier value.
    Cleared,
}

/// One pair, keyed by the name the switch installs it under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pair {
    pub(crate) key: String,
    pub(crate) effect: PairEffect,
}

/// One `<>`, `{}` or `[]` block as the switch parsed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) open: char,
    pub(crate) separator: char,
    pub(crate) pairs: Vec<Pair>,
}

/// One leg: its blocks, the endpoint text after them, and where it sits in the
/// input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Leg {
    pub(crate) raw: Range<usize>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) endpoint: String,
}

/// One `:_:` thread, or the whole dial string when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Thread {
    pub(crate) blocks: Vec<Block>,
    pub(crate) groups: Vec<Vec<Leg>>,
}

/// A dial string after every pass up to the channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DialList {
    /// `<>` blocks ahead of an enterprise split, installed on every thread.
    pub(crate) blocks: Vec<Block>,
    pub(crate) threads: Vec<Thread>,
    /// `origination_nested_vars=true` appears in the text, which lets a value
    /// holding `${` reach the channel.
    pub(crate) nested_vars: bool,
    /// A quote kept a leg separator from splitting.
    pub(crate) quote_spans_legs: bool,
    /// The dialplan carrier substitutes a `${}` or `$${}` reference with a value
    /// only the switch knows; the reference text is kept as written.
    pub(crate) carrier_expands: bool,
}

/// What stops the switch from reading the dial string at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipelineError {
    /// Nothing to dial.
    Empty,
    /// The API carrier's argument split cuts the dial string into more than one
    /// argument, or leaves a quote open.
    ArgvSplit,
    /// A block never closes, which aborts the whole originate.
    UnclosedBlock { leg: usize },
}

/// Run every pass `target` applies, from the text as given to what each leg's
/// channel receives.
pub(crate) fn read(input: &str, target: DialStringTarget) -> Result<DialList, PipelineError> {
    match target.block_parse() {
        BlockParse::PairSplitCleans => {}
    }
    let input = trace(input);
    let (text, carrier_expands) = match target.carrier() {
        DialStringCarrier::EslApi => (api_argument(&input)?, false),
        DialStringCarrier::Dialplan => expand_escapes(&input),
    };
    let mut reader = Reader::default();
    let (blocks, threads) = if find(&text, ENTERPRISE_DELIM).is_some() {
        let (blocks, data) = head_blocks(skip_spaces(&text), &[('<', '>')], 0)?;
        let data = skip_spaces(data);
        let threads = separate_string_string(data, ENTERPRISE_DELIM, MAX_PEERS)
            .into_iter()
            .map(|span| reader.thread(&data[span]))
            .collect::<Result<_, _>>()?;
        (blocks, threads)
    } else {
        (Vec::new(), vec![reader.thread(&text)?])
    };
    Ok(DialList {
        blocks,
        threads,
        nested_vars: untrace(&text)
            .to_ascii_lowercase()
            .contains("origination_nested_vars=true"),
        quote_spans_legs: reader.quote_spans_legs,
        carrier_expands,
    })
}

/// `originate`'s own `switch_separate_string` on a space, which the dial string
/// must survive as one argument.
fn api_argument(text: &[Traced]) -> Result<Vec<Traced>, PipelineError> {
    let argv = separate(text, ' ', usize::MAX);
    if argv.open_quote
        || argv
            .tokens
            .len()
            > 1
    {
        return Err(PipelineError::ArgvSplit);
    }
    argv.tokens
        .into_iter()
        .next()
        .map(|token| token.text)
        .ok_or(PipelineError::Empty)
}

/// The escape handling of `switch_channel_expand_variables_check`.
fn expand_escapes(text: &[Traced]) -> (Vec<Traced>, bool) {
    let has_escaped_data = text
        .windows(2)
        .any(|w| w[0].0 == '\\' && matches!(w[1].0, '\\' | 'n' | 's' | 't' | '\''));
    if !has_escaped_data
        && !var_check_const(
            text.iter()
                .map(|&(c, _)| c),
        )
    {
        return (text.to_vec(), false);
    }
    let at = |i: usize| {
        text.get(i)
            .map(|&(c, _)| c)
    };
    let mut out = Vec::with_capacity(text.len());
    let mut expands = false;
    let mut p = 0;
    while let Some(c) = at(p) {
        match (c, at(p + 1)) {
            ('\\', Some('$')) => {
                p += 1;
                if at(p + 1) == Some('$') {
                    p += 1;
                }
            }
            ('\\', Some('\'')) => {
                p += 2;
                continue;
            }
            ('\\', Some('\\')) => {
                out.push(text[p]);
                p += 2;
                continue;
            }
            ('$', _) => {
                let reference = p;
                if at(p + 1) == Some('$') {
                    p += 1;
                }
                if at(p + 1) == Some('{') {
                    p = reference_end(text, p + 2);
                    out.extend_from_slice(&text[reference..p]);
                    expands = true;
                    // The char after the reference is copied unescaped, unless it
                    // opens another reference.
                    if at(p).is_some_and(|next| next != '$') {
                        out.push(text[p]);
                        p += 1;
                    }
                    continue;
                }
            }
            _ => {}
        }
        out.push(text[p]);
        p += 1;
    }
    (out, expands)
}

/// The index after the `}` closing a reference whose name starts at `start`.
fn reference_end(text: &[Traced], start: usize) -> usize {
    let mut depth = 1usize;
    for (e, &(c, _)) in text
        .iter()
        .enumerate()
        .skip(start)
    {
        if depth == 1 && c == '}' {
            return e + 1;
        }
        if c == '{' && e != start {
            depth += 1;
        } else if depth > 1 && c == '}' {
            depth -= 1;
        }
    }
    text.len()
}

#[derive(Default)]
struct Reader {
    legs: usize,
    quote_spans_legs: bool,
}

impl Reader {
    fn thread(&mut self, text: &[Traced]) -> Result<Thread, PipelineError> {
        let (blocks, data) = head_blocks(skip_spaces(text), &[('<', '>'), ('{', '}')], self.legs)?;
        let data = skip_spaces(data);
        if data.is_empty() {
            return Err(PipelineError::Empty);
        }
        let groups = separate(data, '|', MAX_PEERS);
        self.quote_spans_legs |= groups.held_delimiter;
        let groups = groups
            .tokens
            .into_iter()
            .map(|group| self.group(group.text))
            .collect::<Result<_, _>>()?;
        Ok(Thread { blocks, groups })
    }

    fn group(&mut self, mut text: Vec<Traced>) -> Result<Vec<Leg>, PipelineError> {
        escape_block_commas(&mut text);
        let legs = separate(&text, ',', MAX_PEERS);
        self.quote_spans_legs |= legs.held_delimiter;
        legs.tokens
            .into_iter()
            .map(|leg| self.leg(leg.text, byte_range(&text, leg.raw)))
            .collect()
    }

    fn leg(&mut self, mut text: Vec<Traced>, raw: Range<usize>) -> Result<Leg, PipelineError> {
        let unclosed = PipelineError::UnclosedBlock { leg: self.legs };
        self.legs += 1;
        let mut pos = text.len() - skip_spaces(&text).len();
        let mut blocks = Vec::new();
        while starts_with(&text[pos..], '[') {
            if let Some(bend) = find_end_paren(&text[pos..], '[', ']') {
                for (c, _) in &mut text[pos + 1..pos + bend] {
                    if *c == QUOTED_ESC_COMMA {
                        *c = ',';
                    }
                }
            }
            let (block, next) =
                parse_block(&text[pos..], '[', ']', UNQUOTED_ESC_COMMA).ok_or(unclosed)?;
            blocks.push(block);
            pos += next;
        }
        Ok(Leg {
            raw,
            blocks,
            endpoint: untrace(skip_spaces(&text[pos..])),
        })
    }
}

fn starts_with(text: &[Traced], c: char) -> bool {
    text.first()
        .is_some_and(|&(first, _)| first == c)
}

/// Each kind of block in turn, as many as follow one another, ahead of `text`.
fn head_blocks<'t>(
    mut text: &'t [Traced],
    kinds: &[(char, char)],
    leg: usize,
) -> Result<(Vec<Block>, &'t [Traced]), PipelineError> {
    let mut blocks = Vec::new();
    for &(open, close) in kinds {
        while starts_with(text, open) {
            let (block, next) =
                parse_block(text, open, close, ',').ok_or(PipelineError::UnclosedBlock { leg })?;
            blocks.push(block);
            text = &text[next..];
        }
    }
    Ok((blocks, text))
}

/// The pre-scan `switch_ivr_originate` runs over a group before its leg split.
fn escape_block_commas(group: &mut [Traced]) {
    let mut end = None;
    let mut quoted = false;
    let mut alt = false;
    for p in 0..group.len() {
        let c = group[p].0;
        if end.is_none() && c == '[' {
            end = find_end_paren(&group[p..], '[', ']').map(|e| p + e);
            alt = matches!(group.get(p + 1..p + 3), Some([('^', _), ('^', _)]));
            quoted = false;
        }
        if c == '\'' {
            quoted = !quoted;
        }
        let escaped = p
            .checked_sub(1)
            .is_some_and(|before| group[before].0 == '\\');
        if c == ',' && !escaped && end.is_some_and(|e| p < e) {
            group[p].0 = if quoted || alt {
                QUOTED_ESC_COMMA
            } else {
                UNQUOTED_ESC_COMMA
            };
        }
        if end == Some(p) {
            end = None;
        }
    }
}

/// `switch_event_create_brackets` on the block opening `text`: the block and the
/// index after its close.
fn parse_block(text: &[Traced], open: char, close: char, comma: char) -> Option<(Block, usize)> {
    let end = find_end_paren(text, open, close)?;
    let content = text.get(1..end)?;
    let (separator, delim, content) = match content {
        [('^', _), ('^', _), (picked, _), rest @ ..] => (*picked, *picked, rest),
        _ => (',', comma, content),
    };
    let pairs = separate(content, delim, BLOCK_PAIRS)
        .tokens
        .iter()
        .map(|token| pair(&token.text))
        .collect();
    Some((
        Block {
            open,
            separator,
            pairs,
        },
        end + 1,
    ))
}

fn pair(text: &[Traced]) -> Pair {
    let mut fields = separate(text, '=', 2)
        .tokens
        .into_iter()
        .map(|field| field.text);
    let key = fields
        .next()
        .map_or_else(String::new, |key| untrace(&key));
    let effect = match fields.next() {
        Some(value) if value.is_empty() => PairEffect::Cleared,
        Some(value) => PairEffect::Set(untrace(&value)),
        None => PairEffect::Ignored,
    };
    Pair { key, effect }
}

/// The value `key` ends with on a leg's channel: `inherited`, the enterprise then thread
/// blocks, installs after the leg's own unless `local_var_clobber` is true among them.
pub(crate) fn resolve<'a>(
    list: &'a DialList,
    thread: &'a Thread,
    leg: &'a Leg,
    key: &str,
) -> Option<&'a str> {
    let global = install(
        list.blocks
            .iter()
            .chain(&thread.blocks),
    );
    let local = install(&leg.blocks);
    let clobber = global
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("local_var_clobber"))
        .is_some_and(|&(_, value)| switch_true(value));
    let (first, last) = if clobber {
        (global, local)
    } else {
        (local, global)
    };
    [last, first]
        .iter()
        .find_map(|headers| {
            headers
                .iter()
                .rev()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|&(_, value)| value)
        })
}

/// The headers of an originate event, which has no `EF_UNIQ_HEADERS`: a set
/// appends, an empty value deletes every header of that name.
fn install<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> Vec<(&'a str, &'a str)> {
    let mut headers: Vec<(&str, &str)> = Vec::new();
    for pair in blocks
        .into_iter()
        .flat_map(|block| &block.pairs)
    {
        match &pair.effect {
            PairEffect::Set(value) => headers.push((&pair.key, value)),
            PairEffect::Cleared => {
                headers.retain(|(name, _)| !name.eq_ignore_ascii_case(&pair.key))
            }
            PairEffect::Ignored => {}
        }
    }
    headers
}

/// `switch_true`.
fn switch_true(value: &str) -> bool {
    let word = ["yes", "on", "true", "t", "enabled", "active", "allow"]
        .iter()
        .any(|word| value.eq_ignore_ascii_case(word));
    let unsigned = value
        .strip_prefix(['-', '+'])
        .unwrap_or(value);
    let number = unsigned
        .chars()
        .all(|c| c == '.' || c.is_ascii_digit());
    let nonzero = unsigned
        .chars()
        .take_while(char::is_ascii_digit)
        .any(|c| c != '0');
    word || (number && nonzero)
}

/// `switch_channel_str2cause` on the text after `error/`.
pub(crate) fn str2cause(text: &str) -> CauseReading {
    if text.starts_with(|c: char| c.is_ascii_digit()) {
        let number = text
            .bytes()
            .take_while(u8::is_ascii_digit)
            .fold(0u32, |n, digit| {
                n.saturating_mul(10)
                    .saturating_add(u32::from(digit - b'0'))
            });
        return CauseReading::Number(number);
    }
    text.to_ascii_uppercase()
        .parse::<HangupCause>()
        .map_or(CauseReading::Unrecognized, CauseReading::Name)
}

/// `switch_string_var_check_const`: whether a value names a variable, which the
/// switch refuses to install while `origination_nested_vars` is off.
pub(crate) fn names_a_variable(value: &str) -> bool {
    var_check_const(value.chars())
}

fn var_check_const(chars: impl IntoIterator<Item = char>) -> bool {
    let mut dollar = false;
    for c in chars {
        if c == '$' {
            dollar = true;
        } else if dollar && c == '{' {
            return true;
        } else if c != '\\' {
            dollar = false;
        }
    }
    false
}
