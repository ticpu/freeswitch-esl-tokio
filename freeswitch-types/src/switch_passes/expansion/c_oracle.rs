//! The dialplan carrier's expansion against the switch's own C on every built tree.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

use super::expand_escapes;
use crate::switch_passes::{trace, untrace};
use crate::test_text::{against_the_c, text};

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
