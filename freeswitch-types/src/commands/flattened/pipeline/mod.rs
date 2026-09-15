//! Port of the passes a dial string meets between a carrier and the channel:
//! the carrier's own pass, `switch_ivr_originate`'s thread and leg splits, and
//! `switch_event_create_brackets` for every block.

use std::ops::Range;

use super::CauseReading;
use crate::channel::HangupCause;
use crate::commands::variables::{BlockParse, DialStringCarrier, DialStringTarget};
use crate::switch_passes::separate::{
    find, find_end_paren, separate_string_string, skip_spaces, ArgvCut, CBuffer,
};
use crate::switch_passes::{byte_range, extent, trace, untrace, Traced};

#[cfg(test)]
mod c_oracle;
#[cfg(test)]
mod tests;

/// `MAX_PEERS` in `switch_ivr_originate.c`: the most threads, groups or legs a
/// split keeps.
pub(crate) const MAX_PEERS: usize = 128;

/// `SWITCH_ENT_ORIGINATE_DELIM`.
pub(crate) const ENTERPRISE_DELIM: &str = ":_:";
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
    /// A `^^` head names a non-ASCII separator, which the split takes as its first byte; whatever
    /// it installs no string carries.
    Unreadable,
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
    /// The parse wrote into the text after the close, which the switch then reads rewritten.
    pub(crate) rewrites_following_text: bool,
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
    pub(crate) raw: Range<usize>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) groups: Vec<Vec<Leg>>,
    /// `origination_nested_vars=true` appears in the thread's text, or the `<>` event ahead of an
    /// enterprise split sets it true; either lets a value holding `${` reach the channel.
    pub(crate) nested_vars: bool,
}

/// A dial string after every pass up to the channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DialList {
    /// `<>` blocks ahead of an enterprise split, installed on every thread.
    pub(crate) blocks: Vec<Block>,
    pub(crate) threads: Vec<Thread>,
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
    /// argument, leaves a quote open, or finds a quote holding its separator.
    ArgvSplit,
    /// A block never closes, which aborts the whole originate.
    UnclosedBlock { leg: usize },
    /// A split the switch runs on a non-ASCII `^^` separator's first byte reaches text no string
    /// carries: a group or leg opening such a head, or a block with one ending in a backslash.
    SplitSeparatorUnreadable,
}

/// Run every pass `target` applies, from the text as given to what each leg's
/// channel receives.
pub(crate) fn read(input: &str, target: DialStringTarget) -> Result<DialList, PipelineError> {
    match target.block_parse() {
        BlockParse::PairSplitCleans => {}
    }
    let input = trace(input);
    let (text, raw, carrier_expands) = match target.carrier() {
        DialStringCarrier::EslApi => {
            let (text, raw) = api_argument(&input, target)?;
            (text, raw, false)
        }
        DialStringCarrier::Dialplan => {
            let (text, references) = expand_escapes(&input);
            (text, extent(&input), !references.is_empty())
        }
    };
    dial_list(&text, raw, carrier_expands)
}

/// `switch_ivr_originate`'s passes over `text`, what the carrier's pass left of the input bytes
/// `raw`.
fn dial_list(
    text: &[Traced],
    raw: Range<usize>,
    carrier_expands: bool,
) -> Result<DialList, PipelineError> {
    let mut reader = Reader::default();
    let (blocks, threads) = if find(text, ENTERPRISE_DELIM).is_some() {
        let head = head_blocks(text, &[('<', '>')], 0)?;
        let scanned = &head.text[..head.data_end];
        let mut threads: Vec<Thread> =
            separate_string_string(&scanned[head.data..], ENTERPRISE_DELIM, MAX_PEERS)
                .into_iter()
                .enumerate()
                .map(|(k, span)| {
                    let span = head.span(k, span);
                    reader.thread(
                        &scanned[span.clone()],
                        byte_range(scanned, raw.clone(), span),
                    )
                })
                .collect::<Result<_, _>>()?;
        let inherited = enterprise_nests(&head.blocks);
        for thread in &mut threads {
            thread.nested_vars |= inherited;
        }
        (head.blocks, threads)
    } else {
        (Vec::new(), vec![reader.thread(text, raw)?])
    };
    Ok(DialList {
        blocks,
        threads,
        quote_spans_legs: reader.quote_spans_legs,
        carrier_expands,
    })
}

/// Whether `switch_ivr_originate` takes `text` down the enterprise path, whatever quoting surrounds
/// the delimiter.
pub(crate) fn splits_into_threads(text: &str) -> bool {
    text.contains(ENTERPRISE_DELIM)
}

/// `switch_stristr` for the opt-in `switch_ivr_originate` looks for in the text it dials.
fn opts_into_nested_vars(text: &[Traced]) -> bool {
    untrace(text)
        .to_ascii_lowercase()
        .contains("origination_nested_vars=true")
}

/// The `<>` event `switch_ivr_enterprise_originate` hands every thread holds a true
/// `origination_nested_vars`, as `switch_event_get_header` finds it.
pub(crate) fn enterprise_nests<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> bool {
    install(blocks)
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("origination_nested_vars"))
        .is_some_and(|(_, value)| switch_true(value))
}

/// `originate`'s own `switch_separate_string`, which the dial string must survive as one
/// argument: that argument and the input bytes it covers.
fn api_argument(
    text: &[Traced],
    target: DialStringTarget,
) -> Result<(Vec<Traced>, Range<usize>), PipelineError> {
    let Some(token) = target.split_argument(text) else {
        return Ok((text.to_vec(), extent(text)));
    };
    token
        .map_err(|ArgvCut| PipelineError::ArgvSplit)?
        .map(|token| (token.text, byte_range(text, extent(text), token.raw)))
        .ok_or(PipelineError::Empty)
}

/// The escape handling of `switch_channel_expand_variables_check`, and the spans of its output
/// holding a reference the switch substitutes.
fn expand_escapes(text: &[Traced]) -> (Vec<Traced>, Vec<Range<usize>>) {
    let has_escaped_data = text
        .windows(2)
        .any(|w| w[0].0 == '\\' && matches!(w[1].0, '\\' | 'n' | 's' | 't' | '\''));
    if !has_escaped_data
        && !var_check_const(
            text.iter()
                .map(|&(c, ..)| c),
        )
    {
        return (text.to_vec(), Vec::new());
    }
    let at = |i: usize| {
        text.get(i)
            .map(|&(c, ..)| c)
    };
    let mut out = Vec::with_capacity(text.len());
    let mut references = Vec::new();
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
                out.push((c, text[p].1, text[p + 1].2));
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
                    let written = out.len();
                    out.extend_from_slice(&text[reference..p]);
                    references.push(written..out.len());
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
    (out, references)
}

/// The index after the `}` closing a reference whose name starts at `start`.
fn reference_end(text: &[Traced], start: usize) -> usize {
    let mut depth = 1usize;
    for (e, &(c, ..)) in text
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
    /// The thread `text`, which covers the input bytes `raw`. Its groups and legs are split in one
    /// copy of the data, as `switch_ivr_originate` splits them in its own.
    fn thread(&mut self, text: &[Traced], raw: Range<usize>) -> Result<Thread, PipelineError> {
        let head = head_blocks(text, &[('<', '>'), ('{', '}')], self.legs)?;
        let scanned = &head.text[..head.data_end];
        let data = &scanned[head.data..];
        if data.is_empty() {
            return Err(PipelineError::Empty);
        }
        let mut buffer = CBuffer::new(data);
        let split = buffer.separate(0, '|', MAX_PEERS);
        if split.unreadable_head {
            return Err(PipelineError::SplitSeparatorUnreadable);
        }
        self.quote_spans_legs |= split.held_delimiter;
        let groups = split
            .tokens
            .into_iter()
            .enumerate()
            .map(|(k, group)| {
                let span = head.span(k, group.raw);
                self.group(
                    &mut buffer,
                    group.start,
                    byte_range(scanned, raw.clone(), span),
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(Thread {
            raw,
            blocks: head.blocks,
            groups,
            nested_vars: opts_into_nested_vars(text),
        })
    }

    /// The group at `start` of `buffer`, which covers the input bytes `raw`. A leg the split reads
    /// past the group's terminator covers none.
    fn group(
        &mut self,
        buffer: &mut CBuffer,
        start: usize,
        raw: Range<usize>,
    ) -> Result<Vec<Leg>, PipelineError> {
        escape_block_commas(buffer.c_str_mut(start));
        let scanned = buffer
            .c_str(start)
            .to_vec();
        let split = buffer.separate(start, ',', MAX_PEERS);
        if split.unreadable_head {
            return Err(PipelineError::SplitSeparatorUnreadable);
        }
        self.quote_spans_legs |= split.held_delimiter;
        split
            .tokens
            .into_iter()
            .map(|leg| {
                let within = |at: usize| (at - start).min(scanned.len());
                let span = within(
                    leg.raw
                        .start,
                )
                    ..within(
                        leg.raw
                            .end,
                    );
                self.leg(buffer, leg.start, byte_range(&scanned, raw.clone(), span))
            })
            .collect()
    }

    fn leg(
        &mut self,
        buffer: &mut CBuffer,
        start: usize,
        raw: Range<usize>,
    ) -> Result<Leg, PipelineError> {
        let unclosed = PipelineError::UnclosedBlock { leg: self.legs };
        self.legs += 1;
        let mut pos = start + leading_spaces(buffer.c_str(start));
        let mut blocks = Vec::new();
        while buffer.at(pos) == '[' {
            if let Some(bend) = find_end_paren(buffer.c_str(pos), '[', ']') {
                for (c, ..) in &mut buffer.c_str_mut(pos)[1..bend] {
                    if *c == QUOTED_ESC_COMMA {
                        *c = ',';
                    }
                }
            }
            let (block, next) = parse_block(buffer, pos, '[', ']', UNQUOTED_ESC_COMMA)
                .ok_or(unclosed)?
                .read()?;
            blocks.push(block);
            pos = next;
        }
        Ok(Leg {
            raw,
            blocks,
            endpoint: untrace(skip_spaces(buffer.c_str(pos))),
        })
    }
}

fn leading_spaces(text: &[Traced]) -> usize {
    text.len() - skip_spaces(text).len()
}

/// The blocks ahead of a list or thread, and where in its text they end.
struct Head {
    blocks: Vec<Block>,
    /// The text read, as the blocks' parse left it.
    text: Vec<Traced>,
    /// The index after the last block, or 0 with none.
    end: usize,
    /// The index the data starts at, past the spaces around the blocks.
    data: usize,
    /// The index of the data's terminator.
    data_end: usize,
}

impl Head {
    /// Token `k` of a split of the data, as indices into the whole text. The first token
    /// reaches back to the blocks, so the spaces the switch skips go with it.
    fn span(&self, k: usize, span: Range<usize>) -> Range<usize> {
        let start = match (k, span.start) {
            (0, 0) => self.end,
            (_, start) => start + self.data,
        };
        start..span.end + self.data
    }
}

/// Each kind of block in turn, as many as follow one another, after the spaces ahead of
/// `text`.
fn head_blocks(text: &[Traced], kinds: &[(char, char)], leg: usize) -> Result<Head, PipelineError> {
    let mut buffer = CBuffer::new(text);
    let mut pos = leading_spaces(text);
    let mut end = 0;
    let mut blocks = Vec::new();
    for &(open, close) in kinds {
        while buffer.at(pos) == open {
            let (block, next) = parse_block(&mut buffer, pos, open, close, ',')
                .ok_or(PipelineError::UnclosedBlock { leg })?
                .read()?;
            blocks.push(block);
            pos = next;
            end = pos;
        }
    }
    let data = pos + leading_spaces(buffer.c_str(pos));
    let data_end = data
        + buffer
            .c_str(data)
            .len();
    Ok(Head {
        blocks,
        text: buffer.into_chars(),
        end,
        data,
        data_end,
    })
}

/// The pre-scan `switch_ivr_originate` runs over a group before its leg split.
pub(crate) fn escape_block_commas(group: &mut [Traced]) {
    let mut end = None;
    let mut quoted = false;
    let mut alt = false;
    for p in 0..group.len() {
        let c = group[p].0;
        if end.is_none() && c == '[' {
            end = find_end_paren(&group[p..], '[', ']').map(|e| p + e);
            alt = matches!(group.get(p + 1..p + 3), Some([('^', ..), ('^', ..)]));
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

/// What `switch_event_create_brackets` read of a block.
struct Parsed {
    block: Block,
    /// The index after the close.
    next: usize,
    /// A non-ASCII separator's split runs past the close, splitting what follows by byte.
    splits_past_close_by_byte: bool,
}

impl Parsed {
    /// The block, or the refusal of a split no string carries.
    fn read(self) -> Result<(Block, usize), PipelineError> {
        if self.splits_past_close_by_byte {
            return Err(PipelineError::SplitSeparatorUnreadable);
        }
        Ok((self.block, self.next))
    }
}

/// `switch_event_create_brackets` on the block opening at `at` of `buffer`, rewriting the buffer
/// as the switch does.
fn parse_block(
    buffer: &mut CBuffer,
    at: usize,
    open: char,
    close: char,
    comma: char,
) -> Option<Parsed> {
    let end = at + find_end_paren(buffer.c_str(at), open, close)?;
    let separator = match buffer
        .c_str(at)
        .get(1..end - at)?
    {
        [('^', ..), ('^', ..), (picked, ..), ..] => *picked,
        [('^', ..), ('^', ..)] => '\0',
        _ => ',',
    };
    let mut block = Block {
        open,
        separator,
        pairs: Vec::new(),
        rewrites_following_text: false,
    };
    let next = end + 1;
    if block.separator_unreadable() {
        let trailing_backslashes = buffer
            .c_str(at)
            .get(4..end - at)
            .unwrap_or_default()
            .iter()
            .rev()
            .take_while(|&&(c, ..)| c == '\\')
            .count();
        return Some(Parsed {
            block,
            next,
            splits_past_close_by_byte: trailing_backslashes % 2 == 1,
        });
    }
    let following = chars(buffer.c_str(next));
    buffer.terminate(end);
    let (mut data, mut delim) = (at + 1, comma);
    let mut second = following_block(buffer, next, open, close);
    loop {
        if buffer.at(data) == '^' && buffer.at(data + 1) == '^' {
            delim = buffer.at(data + 2);
            data += 3;
        }
        let split = buffer.separate(data, delim, BLOCK_PAIRS);
        if split.unreadable_head {
            block
                .pairs
                .push(Pair {
                    key: untrace(buffer.c_str(data)),
                    effect: PairEffect::Unreadable,
                });
        } else {
            for token in split.tokens {
                block
                    .pairs
                    .push(pair(buffer, token.start));
            }
        }
        match second.take() {
            Some(close) => data = close,
            None => break,
        }
    }
    block.rewrites_following_text = chars(buffer.c_str(next)) != following;
    Some(Parsed {
        block,
        next,
        splits_past_close_by_byte: false,
    })
}

fn chars(text: &[Traced]) -> Vec<char> {
    text.iter()
        .map(|&(c, ..)| c)
        .collect()
}

/// Where `switch_event_create_brackets` reads more pairs after the first: the close of a block
/// opening right after an opener that follows the close past spaces.
fn following_block(buffer: &CBuffer, after: usize, open: char, close: char) -> Option<usize> {
    let rest = buffer.c_str(after);
    let opener = match rest
        .iter()
        .position(|&(c, ..)| c != ' ')
    {
        _ if rest.is_empty() => return None,
        Some(at) if rest[at].0 == open => after + at,
        Some(_) => return None,
        None => after + rest.len(),
    };
    find_end_paren(buffer.c_str(opener + 1), open, close).map(|at| opener + 1 + at)
}

impl Block {
    /// A non-ASCII `^^` separator, which the switch reads as its first UTF-8 byte and
    /// which no char split mirrors; such a block carries no pairs.
    pub(crate) fn separator_unreadable(&self) -> bool {
        !self
            .separator
            .is_ascii()
    }
}

/// The pair the `=` split reads of the token at `index`, in place.
fn pair(buffer: &mut CBuffer, index: usize) -> Pair {
    let token = untrace(buffer.c_str(index));
    let split = buffer.separate(index, '=', 2);
    if split.unreadable_head {
        return Pair {
            key: token,
            effect: PairEffect::Unreadable,
        };
    }
    let fields: Vec<usize> = split
        .tokens
        .iter()
        .map(|field| field.start)
        .collect();
    let key = fields
        .first()
        .map_or_else(String::new, |&at| untrace(buffer.c_str(at)));
    let effect = match fields[..] {
        [_, value] => match untrace(buffer.c_str(value)) {
            value if value.is_empty() => PairEffect::Cleared,
            value => PairEffect::Set(value),
        },
        _ => PairEffect::Ignored,
    };
    Pair { key, effect }
}

/// The value `key` ends with on a leg's channel: `inherited` (enterprise then thread) installs after
/// the leg's own unless `local_var_clobber` says otherwise, refusing `${` values unless `nested_vars`.
pub(crate) fn resolve<'a>(
    inherited: impl IntoIterator<Item = &'a Block>,
    leg: &'a Leg,
    key: &str,
    nested_vars: bool,
) -> Option<&'a str> {
    let global = install(inherited);
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
    first
        .iter()
        .chain(&last)
        .filter(|(name, value)| {
            name.eq_ignore_ascii_case(key) && (nested_vars || !names_a_variable(value))
        })
        .map(|&(_, value)| value)
        .next_back()
}

/// The headers of an originate event, which carries `EF_UNIQ_HEADERS`: a set replaces every header
/// of that name ignoring case, an empty value deletes them.
fn install<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> Vec<(&'a str, &'a str)> {
    let mut headers: Vec<(&str, &str)> = Vec::new();
    for pair in blocks
        .into_iter()
        .flat_map(|block| &block.pairs)
    {
        match &pair.effect {
            PairEffect::Set(value) => {
                headers.retain(|(name, _)| !name.eq_ignore_ascii_case(&pair.key));
                headers.push((&pair.key, value))
            }
            PairEffect::Cleared => {
                headers.retain(|(name, _)| !name.eq_ignore_ascii_case(&pair.key))
            }
            PairEffect::Ignored | PairEffect::Unreadable => {}
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
