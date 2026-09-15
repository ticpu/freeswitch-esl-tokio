//! Text for property tests, weighted toward what the switch's string passes treat specially.

use std::io::Write as _;

use freeswitch_c_oracle::Oracle;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{TestCaseResult, TestRunner};

const HOSTILE: &[&str] = &[
    " ", "\t", "'", "\"", "\\", ",", "|", ":", "_", ":_:", "=", "{", "}", "[", "]", "<", ">", "^",
    "^^", "~", "!", ";", "$", "${", "(", ")", "&", r"\n", r"\r", r"\t", r"\s", "n", "r", "t", "s",
    "é", "😀", "\u{b}",
];
const ORDINARY: &[&str] = &["a", "Z", "0", "42", "bob", "x9"];
const WHOLE: &[&str] = &["", "undef", "UNDEF", "Undef"];
const EDGE: &[&str] = &["", "", " ", "  "];

/// `PROPTEST_CASES` raises the count the pre-commit hook runs.
pub(crate) fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|cases| {
            cases
                .parse()
                .ok()
        })
        .unwrap_or(1024);
    ProptestConfig {
        cases,
        ..ProptestConfig::default()
    }
}

pub(crate) fn text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![3 => select(HOSTILE), 2 => select(ORDINARY)];
    let body = vec(piece, 0..8).prop_map(|pieces| pieces.concat());
    prop_oneof![
        8 => (select(EDGE), body, select(EDGE))
            .prop_map(|(lead, body, trail)| format!("{lead}{body}{trail}")),
        1 => select(WHOLE).prop_map(str::to_owned),
    ]
}

/// Run `property` over `strategy` against the C of every built tree, seeds kept beside `source`,
/// writing one line per tree not built; the line bypasses the harness's output capture.
pub(crate) fn against_the_c<S: Strategy>(
    source: &'static str,
    name: &str,
    strategy: S,
    property: impl Fn(Oracle, S::Value) -> TestCaseResult,
) {
    for tree in freeswitch_c_oracle::trees() {
        let oracle = match tree.oracle() {
            Ok(oracle) => oracle,
            Err(missing) => {
                writeln!(
                    std::io::stderr(),
                    "{name}: skipped on tree {}, {missing}",
                    tree.name()
                )
                .unwrap_or_else(|e| panic!("{name}: writing the skip line: {e}"));
                continue;
            }
        };
        let config = ProptestConfig {
            source_file: Some(source),
            ..config()
        };
        if let Err(failure) =
            TestRunner::new(config).run(&strategy, |value| property(oracle, value))
        {
            panic!("{name} on tree {}: {failure}", tree.name());
        }
    }
}

/// CI fetches every public tree, so one missing there is a broken fetch, never a skip.
#[test]
fn public_trees_are_built_under_ci() {
    if !std::env::var_os("CI").is_some_and(|ci| ci == "true") {
        return;
    }
    for tree in freeswitch_c_oracle::trees() {
        match (tree.oracle(), tree.is_public()) {
            (Ok(_), _) => {}
            (Err(missing), true) => {
                panic!(
                    "CI runs the C oracle on tree {}, which was not built: {missing}",
                    tree.name()
                )
            }
            (Err(missing), false) => writeln!(
                std::io::stderr(),
                "tree {} is not public, CI skips it: {missing}",
                tree.name()
            )
            .unwrap_or_else(|e| panic!("writing the skip line for tree {}: {e}", tree.name())),
        }
    }
}
