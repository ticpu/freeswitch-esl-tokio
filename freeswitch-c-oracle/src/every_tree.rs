//! Running a property against the C of every built tree.

use std::fmt::Debug;
use std::io::Write as _;

use proptest::prelude::{prop_assert_eq, ProptestConfig};
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

/// Every built tree, by name, with its C.
pub fn oracles() -> impl Iterator<Item = (&'static str, Oracle)> {
    trees()
        .iter()
        .filter_map(|tree| {
            tree.oracle()
                .ok()
                .map(|oracle| (tree.name(), oracle))
        })
}

/// [`on_every_tree`] asserting `read` gives on each tree what it gives on the first built one,
/// the pin where it is built.
///
/// # Panics
///
/// On the first tree that reads a value differently, naming it.
pub fn trees_agree<S, T>(
    source: &'static str,
    name: &str,
    strategy: S,
    read: impl Fn(Oracle, &S::Value) -> T,
) where
    S: Strategy,
    S::Value: Debug,
    T: PartialEq + Debug,
{
    let reference = oracles().next();
    on_every_tree(source, name, strategy, |tree, oracle, value| {
        if let Some((first, reference)) = reference {
            prop_assert_eq!(
                read(oracle, &value),
                read(reference, &value),
                "tree {} against {} on {:?}",
                tree,
                first,
                value
            );
        }
        Ok(())
    });
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
