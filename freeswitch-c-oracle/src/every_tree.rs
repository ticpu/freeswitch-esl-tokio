//! Running a property against the C of every built tree.

use std::io::Write as _;

use proptest::prelude::ProptestConfig;
use proptest::strategy::Strategy;
use proptest::test_runner::{TestCaseResult, TestRunner};

use crate::{trees, Oracle};

/// `PROPTEST_CASES` raises the count the pre-commit hook runs.
pub fn config() -> ProptestConfig {
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

/// Run `property` over `strategy` against the C of every built tree, with the tree's name, seeds
/// kept beside `source`, writing one line per tree not built past the harness's output capture.
///
/// # Panics
///
/// On the first tree the property fails on, naming it.
pub fn on_every_tree<S: Strategy>(
    source: &'static str,
    name: &str,
    strategy: S,
    property: impl Fn(&'static str, Oracle, S::Value) -> TestCaseResult,
) {
    for tree in trees() {
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
            TestRunner::new(config).run(&strategy, |value| property(tree.name(), oracle, value))
        {
            panic!("{name} on tree {}: {failure}", tree.name());
        }
    }
}

/// [`on_every_tree`] for a property every tree must meet alike.
///
/// # Panics
///
/// On the first tree the property fails on, naming it.
pub fn against_the_c<S: Strategy>(
    source: &'static str,
    name: &str,
    strategy: S,
    property: impl Fn(Oracle, S::Value) -> TestCaseResult,
) {
    on_every_tree(source, name, strategy, |_, oracle, value| {
        property(oracle, value)
    });
}
