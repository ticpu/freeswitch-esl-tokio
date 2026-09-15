//! The dial-string passes against the switch's own C on every built tree.

use freeswitch_c_oracle::{Dial, Oracle, Pair};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use super::{
    dial_list, expand_escapes, install, parse_block, switch_true, Block, DialList, PairEffect,
    PipelineError, UNQUOTED_ESC_COMMA,
};
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
                PairEffect::Ignored | PairEffect::Unreadable => return None,
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
fn unmodelled(block: &Block) -> bool {
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
                .is_some_and(|(block, _)| unmodelled(block))
            {
                return Ok(());
            }
            let port = port.map(|(block, next)| {
                let rest = byte_offset(&text, &input, next);
                (installed([&block]), rest, block.rewrites_following_text)
            });
            let switch = c
                .brackets(input.as_bytes(), open as u8, close as u8, comma as u8)
                .map(|read| {
                    let rewritten = read.following != input.as_bytes()[read.rest..];
                    (read.pairs, read.rest, rewritten)
                });
            prop_assert_eq!(port, switch, "{:?} split on {:?}", input, comma);
            Ok(())
        },
    );
}

fn spaces() -> impl Strategy<Value = &'static str> {
    select(&["", "", " ", "  "][..])
}

/// One thread's text: its `<>` and `{}` blocks, then `|` groups of `,` legs under `[]` blocks.
fn thread_text() -> impl Strategy<Value = String> {
    let leg = (spaces(), vec(block_text('[', ']'), 0..3), text())
        .prop_map(|(lead, blocks, endpoint)| format!("{lead}{}{endpoint}", blocks.concat()));
    let group = vec(leg, 1..4).prop_map(|legs| legs.join(","));
    (
        spaces(),
        option::of(block_text('<', '>')),
        vec(block_text('{', '}'), 0..3),
        vec(group, 1..4),
    )
        .prop_map(|(lead, ultra, global, groups)| {
            format!(
                "{lead}{}{}{}",
                ultra.unwrap_or_default(),
                global.concat(),
                groups.join("|")
            )
        })
}

const NESTED_VARS: &[&str] = &[
    "",
    "",
    "",
    "{origination_nested_vars=true}",
    "[ORIGINATION_NESTED_VARS=TRUE]",
    "<origination_nested_vars=yes>",
];

/// A dial string built from the grammar the passes read, spaces and stray text included.
fn dial_text() -> impl Strategy<Value = String> {
    let enterprise = (
        spaces(),
        select(NESTED_VARS),
        option::of(block_text('<', '>')),
        vec(thread_text(), 2..4),
    )
        .prop_map(|(lead, nested, ultra, threads)| {
            format!(
                "{lead}{nested}{}{}",
                ultra.unwrap_or_default(),
                threads.join(":_:")
            )
        });
    prop_oneof![
        4 => (select(NESTED_VARS), thread_text()).prop_map(|(nested, thread)| format!("{nested}{thread}")),
        2 => enterprise,
        1 => text(),
    ]
}

/// Every block of the list, the port flagging at least one it does not read.
fn flags_a_block(list: &DialList) -> bool {
    list.blocks
        .iter()
        .chain(
            list.threads
                .iter()
                .flat_map(|thread| {
                    thread
                        .blocks
                        .iter()
                        .chain(
                            thread
                                .groups
                                .iter()
                                .flatten()
                                .flat_map(|leg| &leg.blocks),
                        )
                }),
        )
        .any(|block| unmodelled(block) || block.rewrites_following_text)
}

/// How a thread reads, the same shape whichever side produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ThreadView {
    pairs: Vec<Pair>,
    groups: Vec<Vec<(Vec<Pair>, Vec<u8>)>>,
}

/// What stopped a read: nothing to dial, or a block that never closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Empty,
    Unclosed,
}

fn port_view(list: &DialList) -> (Vec<Pair>, Vec<ThreadView>) {
    let threads = list
        .threads
        .iter()
        .map(|thread| ThreadView {
            pairs: installed(&thread.blocks),
            groups: thread
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|leg| {
                            (
                                installed(&leg.blocks),
                                leg.endpoint
                                    .clone()
                                    .into_bytes(),
                            )
                        })
                        .collect()
                })
                .collect(),
        })
        .collect();
    (installed(&list.blocks), threads)
}

fn switch_stop(failure: &[u8]) -> Stop {
    match failure {
        b"No origination URL specified!" => Stop::Empty,
        b"Parse Error!" => Stop::Unclosed,
        other => panic!("no port reading for {:?}", String::from_utf8_lossy(other)),
    }
}

/// The switch's reading, or the first thing that stopped it.
fn switch_view(dial: &Dial) -> Result<(Vec<Pair>, Vec<ThreadView>), Stop> {
    if let Some(failure) = &dial.failure {
        return Err(switch_stop(failure));
    }
    let threads = dial
        .threads
        .iter()
        .map(|thread| {
            if let Some(failure) = &thread.failure {
                return Err(switch_stop(failure));
            }
            Ok(ThreadView {
                pairs: thread
                    .pairs
                    .clone(),
                groups: thread
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|leg| {
                                (
                                    leg.pairs
                                        .clone(),
                                    leg.endpoint
                                        .clone()
                                        .unwrap_or_default(),
                                )
                            })
                            .collect()
                    })
                    .collect(),
            })
        })
        .collect::<Result<_, _>>()?;
    Ok((
        dial.enterprise
            .clone(),
        threads,
    ))
}

/// Whether the `<>` event `switch_ivr_enterprise_originate` hands every thread turns nested vars
/// on: its first `origination_nested_vars` header, read by the switch's `switch_true`. The stub
/// stores no headers, so the event store is the port's.
fn enterprise_nests(c: Oracle, enterprise: &[Pair]) -> bool {
    let utf8 = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let block = Block {
        open: '<',
        separator: ',',
        pairs: enterprise
            .iter()
            .map(|(key, value)| super::Pair {
                key: utf8(key),
                effect: match &value[..] {
                    [] => PairEffect::Cleared,
                    value => PairEffect::Set(utf8(value)),
                },
            })
            .collect(),
        rewrites_following_text: false,
    };
    install([&block])
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("origination_nested_vars"))
        .is_some_and(|(_, value)| c.switch_true(value.as_bytes()))
}

#[test]
fn dial_lists_match_the_switch() {
    against_the_c(
        file!(),
        "dial_lists_match_the_switch",
        dial_text(),
        |c, input| {
            let text = trace(&input);
            let port = dial_list(&text, 0..input.len(), false);
            if port
                .as_ref()
                .is_ok_and(flags_a_block)
            {
                return Ok(());
            }
            let dial = c.dial(input.as_bytes());
            let port_read = port
                .as_ref()
                .map(port_view)
                .map_err(|e| match e {
                    PipelineError::Empty => Stop::Empty,
                    PipelineError::UnclosedBlock { .. } => Stop::Unclosed,
                    PipelineError::ArgvSplit => unreachable!("no carrier pass runs"),
                });
            prop_assert_eq!(&port_read, &switch_view(&dial), "{:?}", input);
            if let Ok(list) = &port {
                let inherited = enterprise_nests(c, &dial.enterprise);
                let switch: Vec<bool> = dial
                    .threads
                    .iter()
                    .map(|thread| thread.nested_vars || inherited)
                    .collect();
                let port: Vec<bool> = list
                    .threads
                    .iter()
                    .map(|_| list.nested_vars)
                    .collect();
                prop_assert_eq!(port, switch, "nested vars per thread in {:?}", input);
            }
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
