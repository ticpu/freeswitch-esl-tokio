//! `switch_event_create_brackets` against the switch's own C on every built tree.

use freeswitch_c_oracle::{against_the_c, Pair};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

use super::{parse_block, Block, PairEffect};
use crate::switch_passes::originate_legs::UNQUOTED_ESC_COMMA;
use crate::switch_passes::separate::CBuffer;
use crate::switch_passes::{trace, untrace, Traced};
use crate::test_text::text;

/// The headers installing `blocks` adds, as the switch hands them to the event.
pub(crate) fn installed<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> Vec<Pair> {
    blocks
        .into_iter()
        .flat_map(|block| &block.pairs)
        .filter_map(|pair| {
            let value = match &pair.effect {
                PairEffect::Set(value) => value.as_str(),
                PairEffect::Cleared => "",
                PairEffect::Ignored | PairEffect::Unreadable | PairEffect::Valueless => {
                    return None
                }
            };
            Some((
                pair.key
                    .clone()
                    .into_bytes(),
                value
                    .as_bytes()
                    .to_vec(),
            ))
        })
        .collect()
}

/// A block the port flags rather than reads: a non-ASCII separator, or a pair opening one.
pub(crate) fn unmodelled(block: &Block) -> bool {
    block.separator_unreadable()
        || block
            .pairs
            .iter()
            .any(|pair| pair.effect == PairEffect::Unreadable)
}

fn byte_offset(text: &[Traced], input: &str, index: usize) -> usize {
    text.get(index)
        .map_or(input.len(), |&(_, start, _)| start)
}

const HEADS: &[&str] = &[
    "", "", "", "^^", "^^,", "^^:", "^^~", "^^'", "^^\\", "^^=", "^^é",
];

/// Block content: text, the separators the parse reads, and brackets of any kind.
fn content() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        3 => text(),
        2 => select(&["=", ",", ":", "~", "\u{2}", "{", "}", "[", "]", "<", ">", "k=v", "'", "^^"][..])
            .prop_map(str::to_owned),
    ];
    vec(piece, 0..6).prop_map(|pieces| pieces.concat())
}

/// A block of `open` and `close`, its close sometimes missing.
pub(crate) fn block_text(open: char, close: char) -> impl Strategy<Value = String> {
    (
        select(HEADS),
        content(),
        prop_oneof![6 => Just(true), 1 => Just(false)],
    )
        .prop_map(move |(head, content, closed)| {
            let close = if closed {
                close.to_string()
            } else {
                String::new()
            };
            format!("{open}{head}{content}{close}")
        })
}

#[test]
fn blocks_match_the_switch() {
    let kind = select(
        &[
            ('<', '>', ','),
            ('{', '}', ','),
            ('[', ']', ','),
            ('[', ']', UNQUOTED_ESC_COMMA),
        ][..],
    );
    let case = kind.prop_flat_map(|(open, close, comma)| {
        (Just((open, close, comma)), block_text(open, close), text())
    });
    against_the_c(
        file!(),
        "blocks_match_the_switch",
        case,
        |c, ((open, close, comma), block, tail)| {
            let input = format!("{block}{tail}");
            let text = trace(&input);
            let mut buffer = CBuffer::new(&text);
            let port = parse_block(&mut buffer, 0, open, close, comma);
            if port
                .as_ref()
                .is_some_and(|parsed| unmodelled(&parsed.block))
            {
                return Ok(());
            }
            let port = port.map(|parsed| {
                (
                    installed([&parsed.block]),
                    byte_offset(&text, &input, parsed.next),
                    untrace(buffer.c_str(parsed.next)).into_bytes(),
                )
            });
            let switch = c
                .brackets(input.as_bytes(), open as u8, close as u8, comma as u8)
                .map(|read| (read.pairs, read.rest, read.following));
            prop_assert_eq!(port, switch, "{:?} split on {:?}", input, comma);
            Ok(())
        },
    );
}
