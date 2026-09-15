//! What the build made of the trees the index names.

use std::io::Write as _;

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
