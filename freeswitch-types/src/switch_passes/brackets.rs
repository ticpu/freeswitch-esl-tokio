//! Port of `switch_event_create_brackets`, which reads the pairs of a `<>`, `{}` or `[]` block out
//! of the buffer it is handed, and of the originate event those pairs install into.

use super::separate::{find_end_paren, CBuffer};
use super::{untrace, PipelineError, Traced};

#[cfg(test)]
pub(super) mod c_oracle;

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
    /// The pair ends at its `=`, whose empty field the split drops, so nothing is installed.
    Valueless,
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

/// What `switch_event_create_brackets` read of a block.
pub(crate) struct Parsed {
    pub(crate) block: Block,
    /// The index after the close.
    pub(crate) next: usize,
    /// A non-ASCII separator's split runs past the close, splitting what follows by byte.
    splits_past_close_by_byte: bool,
}

impl Parsed {
    /// The block, or the refusal of a split no string carries.
    pub(crate) fn read(self) -> Result<(Block, usize), PipelineError> {
        if self.splits_past_close_by_byte {
            return Err(PipelineError::SplitSeparatorUnreadable);
        }
        Ok((self.block, self.next))
    }
}

/// `switch_event_create_brackets` on the block opening at `at` of `buffer`, rewriting the buffer
/// as the switch does.
pub(crate) fn parse_block(
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
    let end = index
        + buffer
            .c_str(index)
            .len();
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
        [_] if split
            .tokens
            .first()
            .is_some_and(|field| {
                field
                    .raw
                    .end
                    < end
            }) =>
        {
            PairEffect::Valueless
        }
        _ => PairEffect::Ignored,
    };
    Pair { key, effect }
}

/// Whether two names are one header of an originate event, which compares them by `strcasecmp`.
pub(crate) fn same_header(name: &str, other: &str) -> bool {
    name.eq_ignore_ascii_case(other)
}

/// The headers of an originate event, which carries `EF_UNIQ_HEADERS`: a set replaces every header
/// of that name ignoring case, an empty value deletes them.
pub(crate) fn install<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> Vec<(&'a str, &'a str)> {
    let mut headers: Vec<(&str, &str)> = Vec::new();
    for pair in blocks
        .into_iter()
        .flat_map(|block| &block.pairs)
    {
        match &pair.effect {
            PairEffect::Set(value) => {
                headers.retain(|(name, _)| !same_header(name, &pair.key));
                headers.push((&pair.key, value))
            }
            PairEffect::Cleared => headers.retain(|(name, _)| !same_header(name, &pair.key)),
            PairEffect::Ignored | PairEffect::Unreadable | PairEffect::Valueless => {}
        }
    }
    headers
}
