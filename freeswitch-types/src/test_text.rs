//! Text for property tests, weighted toward what the switch's string passes treat specially.

use std::io::Write as _;

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

/// Run `property` over `strategy` against the switch's C, seeds kept beside `source`, or write
/// one line naming why the oracle is absent; the line bypasses the harness's output capture.
pub(crate) fn against_the_c<S: Strategy>(
    source: &'static str,
    name: &str,
    strategy: S,
    property: impl Fn(S::Value) -> TestCaseResult,
) {
    if let Some(missing) = freeswitch_c_oracle::missing() {
        writeln!(std::io::stderr(), "{name}: skipped, {missing}")
            .unwrap_or_else(|e| panic!("{name}: writing the skip line: {e}"));
        return;
    }
    let config = ProptestConfig {
        source_file: Some(source),
        ..config()
    };
    if let Err(failure) = TestRunner::new(config).run(&strategy, property) {
        panic!("{name}: {failure}");
    }
}
