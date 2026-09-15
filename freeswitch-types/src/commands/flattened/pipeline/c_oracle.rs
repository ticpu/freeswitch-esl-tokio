//! The dial-string passes against the switch's own C on every built tree.

use freeswitch_c_oracle::Pair;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

use super::{expand_escapes, parse_block, switch_true, Block, PairEffect, UNQUOTED_ESC_COMMA};
use crate::test_text::{against_the_c, text};
use crate::tokenizer::{trace, untrace, Traced};

/// The headers installing `blocks` adds, as the switch hands them to the event.
fn installed<'a>(blocks: impl IntoIterator<Item = &'a Block>) -> Vec<Pair> {
    blocks
        .into_iter()
        .flat_map(|block| &block.pairs)
        .filter_map(|pair| {
            let value = match &pair.effect {
                PairEffect::Set(value) => value.as_str(),
                PairEffect::Cleared => "",
                PairEffect::Ignored => return None,
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
fn block_text(open: char, close: char) -> impl Strategy<Value = String> {
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
            let port = parse_block(&text, open, close, comma);
            if port
                .as_ref()
                .is_some_and(|(block, _)| block.separator_unreadable())
            {
                return Ok(());
            }
            let port =
                port.map(|(block, next)| (installed([&block]), byte_offset(&text, &input, next)));
            let switch = c
                .brackets(input.as_bytes(), open as u8, close as u8, comma as u8)
                .map(|read| (read.pairs, read.rest));
            prop_assert_eq!(port, switch, "{:?} split on {:?}", input, comma);
            Ok(())
        },
    );
}

/// Text weighted toward what the dialplan carrier's expansion reads: references, escapes, `$$`.
fn expansion_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        2 => text(),
        3 => select(&[
            "${", "$${", "}", "$", "$$", "{", r"\$", r"\$$", r"\'", r"\\", r"\n", "${a}", "$${g}",
            "${f(x)}", "${cmd arg}", "${a:1}", "${a[0]}", "${a${b}}", " ", "(", ")",
        ][..])
        .prop_map(str::to_owned),
    ];
    vec(piece, 0..8).prop_map(|pieces| pieces.concat())
}

/// Every reference the port keeps as written is one the switch looks up, and substituting each
/// with nothing leaves the switch's output.
#[test]
fn expansion_matches_the_switch() {
    against_the_c(
        file!(),
        "expansion_matches_the_switch",
        expansion_text(),
        |c, input| {
            let (out, references) = expand_escapes(&trace(&input));
            let mut kept = String::new();
            let mut at = 0;
            for reference in &references {
                kept.push_str(&untrace(&out[at..reference.start]));
                at = reference.end;
            }
            kept.push_str(&untrace(&out[at..]));
            let switch = c.expand(input.as_bytes());
            let looked_up = !switch
                .lookups
                .is_empty()
                || !switch
                    .api_calls
                    .is_empty();
            prop_assert_eq!(
                (kept.as_bytes(), !references.is_empty()),
                (&switch.text[..], looked_up),
                "{:?}: switch looked up {:?}, called {:?}",
                input,
                switch.lookups,
                switch.api_calls
            );
            Ok(())
        },
    );
}

#[test]
fn switch_true_matches_the_switch() {
    let word = select(
        &[
            "yes", "YES", "On", "true", "t", "T", "enabled", "active", "allow", "no", "false", "0",
            "1", "-1", "+2", "00", "0.5", ".", "", "-", "+", "1a", "１", "10.", "-0",
        ][..],
    );
    let value = prop_oneof![
        2 => word.prop_map(str::to_owned),
        1 => text(),
    ];
    against_the_c(
        file!(),
        "switch_true_matches_the_switch",
        value,
        |c, value| {
            prop_assert_eq!(
                switch_true(&value),
                c.switch_true(value.as_bytes()),
                "{:?}",
                value
            );
            Ok(())
        },
    );
}

#[test]
fn the_oracle_reads_one_originate() {
    let Some(c) = freeswitch_c_oracle::trees()
        .iter()
        .find_map(|tree| {
            tree.oracle()
                .ok()
        })
    else {
        return;
    };
    let dial = c.dial(b"<e=1>{g=2}[l=3]loopback/9199,error/USER_BUSY|null/a");
    let pair = |key: &[u8], value: &[u8]| -> Pair { (key.to_vec(), value.to_vec()) };
    assert_eq!(dial.enterprise, []);
    let [thread] = &dial.threads[..] else {
        panic!("one thread: {dial:?}");
    };
    assert_eq!(thread.pairs, [pair(b"e", b"1"), pair(b"g", b"2")]);
    assert_eq!(
        thread
            .groups
            .len(),
        2
    );
    assert_eq!(thread.groups[0][0].pairs, [pair(b"l", b"3")]);
    assert_eq!(
        thread.groups[0][0]
            .endpoint
            .as_deref(),
        Some(&b"loopback/9199"[..])
    );
    assert_eq!(
        thread.groups[1][0]
            .endpoint
            .as_deref(),
        Some(&b"null/a"[..])
    );
}
