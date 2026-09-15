//! The port against the switch's own tokenizer, compiled from the pinned source, byte for byte.

use proptest::prelude::*;
use proptest::sample::select;

use super::{
    cleanup, delimiter_override, find_end_paren, separate, separate_on, separate_string_string,
    trace, untrace,
};
use crate::test_text::{against_the_c, text};

/// Text that may open with a `^^X` head, ASCII or not.
fn line() -> impl Strategy<Value = String> {
    let head = select(&["^^", "^^ ", "^^~", "^^,", "^^'", "^^\\", "^^é", "^^😀"][..]);
    prop_oneof![
        3 => text(),
        1 => (head, text()).prop_map(|(head, rest)| format!("{head}{rest}")),
    ]
}

/// `switch_separate_string` takes the first byte after `^^` whatever it is; the port takes no
/// head naming a non-ASCII separator, which no char split mirrors.
fn opens_with_a_non_ascii_head(input: &str) -> bool {
    matches!(input.as_bytes(), [b'^', b'^', picked, _, ..] if !picked.is_ascii())
}

fn owned(tokens: impl IntoIterator<Item = String>) -> Vec<Vec<u8>> {
    tokens
        .into_iter()
        .map(String::into_bytes)
        .collect()
}

const CLEANUP_DELIMS: &[u8] = &[0, b',', b'~', b' ', b'|', b'=', b'\'', b'\\', b'n', b':'];
const SPLIT_DELIMS: &[u8] = b" ,|=~:'";
const LIMITS: &[u32] = &[1, 2, 3, 10, 128, 1024];

#[test]
fn cleanup_matches_the_switch() {
    against_the_c(
        file!(),
        "cleanup_matches_the_switch",
        (line(), select(CLEANUP_DELIMS)),
        |(input, delim)| {
            let port = untrace(&cleanup(
                &trace(&input),
                (delim != 0).then_some(char::from(delim)),
            ));
            prop_assert_eq!(
                freeswitch_c_oracle::cleanup(input.as_bytes(), delim),
                Some(port.into_bytes()),
                "{:?} cleaned up on {:?}",
                input,
                char::from(delim)
            );
            Ok(())
        },
    );
}

#[test]
fn separate_matches_the_switch() {
    against_the_c(
        file!(),
        "separate_matches_the_switch",
        (line(), select(SPLIT_DELIMS), select(LIMITS)),
        |(input, delim, limit)| {
            let text = trace(&input);
            let port = separate(&text, char::from(delim), limit as usize);
            if opens_with_a_non_ascii_head(&input) {
                prop_assert!(delimiter_override(&text)
                    .0
                    .is_none());
                return Ok(());
            }
            let port = owned(
                port.tokens
                    .iter()
                    .map(|token| untrace(&token.text)),
            );
            prop_assert_eq!(
                freeswitch_c_oracle::separate_string(input.as_bytes(), delim, limit),
                Some(port),
                "{:?} on {:?} keeping {}",
                input,
                char::from(delim),
                limit
            );
            Ok(())
        },
    );
}

#[test]
fn char_and_blank_splits_match_the_switch() {
    against_the_c(
        file!(),
        "char_and_blank_splits_match_the_switch",
        (line(), select(SPLIT_DELIMS), select(LIMITS)),
        |(input, delim, limit)| {
            let port = owned(
                separate_on(&trace(&input), char::from(delim), limit as usize)
                    .tokens
                    .iter()
                    .map(|token| untrace(&token.text)),
            );
            let c = match delim {
                b' ' => freeswitch_c_oracle::blank_delim(input.as_bytes(), limit),
                delim => freeswitch_c_oracle::char_delim(input.as_bytes(), delim, limit),
            };
            prop_assert_eq!(
                c,
                Some(port),
                "{:?} on {:?} keeping {}",
                input,
                char::from(delim),
                limit
            );
            Ok(())
        },
    );
}

#[test]
fn string_split_matches_the_switch() {
    against_the_c(
        file!(),
        "string_split_matches_the_switch",
        (
            line(),
            select(&[":_:", ",", "ab", "::"][..]),
            select(LIMITS),
        ),
        |(input, delim, limit)| {
            let text = trace(&input);
            let port = owned(
                separate_string_string(&text, delim, limit as usize)
                    .into_iter()
                    .map(|span| untrace(&text[span])),
            );
            prop_assert_eq!(
                freeswitch_c_oracle::separate_string_string(
                    input.as_bytes(),
                    delim.as_bytes(),
                    limit
                ),
                Some(port),
                "{:?} on {:?} keeping {}",
                input,
                delim,
                limit
            );
            Ok(())
        },
    );
}

#[test]
fn end_paren_matches_the_switch() {
    against_the_c(
        file!(),
        "end_paren_matches_the_switch",
        (
            line(),
            select(&[(b'[', b']'), (b'{', b'}'), (b'<', b'>'), (b'\'', b'\'')][..]),
        ),
        |(input, (open, close))| {
            let text = trace(&input);
            let port =
                find_end_paren(&text, char::from(open), char::from(close)).map(|at| text[at].1);
            prop_assert_eq!(
                freeswitch_c_oracle::find_end_paren(input.as_bytes(), open, close),
                Some(port),
                "{:?} from {:?} to {:?}",
                input,
                char::from(open),
                char::from(close)
            );
            Ok(())
        },
    );
}
