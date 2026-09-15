# Contributing

## Hooks

```sh
./hooks/install.sh   # symlinks the pre-commit and pre-push hooks
```

The pre-commit hook enforces:

- Cargo.lock stays off branches -- it may only be committed on the release tag's own detached commit
- `cargo fmt --check` -- formatting
- `cargo clippy --all-features --all-targets -- -D warnings` -- lint warnings as errors
- `RUSTDOCFLAGS="-D missing_docs" cargo doc` -- all public items documented
- `cargo test --workspace --all-features` -- the full test suite, doctests included
- `hooks/check-enums.py` -- validates `EslEventType`, `HangupCause`, `ChannelState`, `CallState`, `CoreMediaVariable` (`core-media-vars`), `ConferenceVariable` (`conference-vars`), `SipHeaderPrefix` (`sip-header-prefixes`), and `EventHeader` (`event-headers`) against FreeSWITCH C source
- `hooks/check-source-refs.py` -- verifies every `file.c:NNN` citation against the pinned FreeSWITCH commit

The pre-push hook backstops the Cargo.lock check against a rebase or cherry-pick that reintroduces it after the commit gate ran.

## Testing

Unit and mock-server tests run without external dependencies:

```sh
cargo test --lib
cargo test --test connection_tests --test command_wire_tests \
    --test connection_failure_tests --test reexec_tests
```

Live integration tests require FreeSWITCH ESL on `127.0.0.1:8022` (password `ClueCon`). They are `#[ignore]` by default:

```sh
cargo test --test 'live_*' -- --ignored
```

They run in parallel against that one switch and raise its `sessions-per-second` to make that safe. [live-test-switch.md](docs/live-test-switch.md) documents the dialplan, modules, and directory users they expect, and the two rules for writing a new one.

## C oracle

The ports in freeswitch-types are checked against the switch's own C, on every FreeSWITCH tree the crate is used against. The unpublished workspace crate freeswitch-c-oracle compiles that C from each tree hooks/source-refs.yaml names: the commit the source references pin, built as pin, and every entry under trees. Its build script reads each file out of the clone `FREESWITCH_SOURCE` names with git show, extracts what it needs into its build directory and compiles one unit per tree with every symbol prefixed by the tree's name, so one test binary runs each differential property against every built tree. Nothing of the FreeSWITCH tree is committed.

Covered on every tree: the string tokenizer of switch_utils.c, switch_true, switch_url_encode_opt with switch_url_encode, switch_core_url_encode_opt and switch_needs_url_encode, switch_event_create_brackets, switch_channel_expand_variables_check, originate_function from mod_commands.c, and the passes switch_ivr_originate and switch_ivr_enterprise_originate run over a dial string up to each leg's endpoint. Stubs stand in for the event, channel and session calls and report each header installed, variable looked up and API called. A lookup answers nothing, so a reference expands to an empty string, and the `<>` event the enterprise originate hands each thread is not modelled.

A function is taken whole from its definition at column 0, a #define by name, and code living inside a larger function as the brace-balanced statement opening on a marker line, which must occur once in that function. Line drift never breaks the extraction, and a tree that renames a function or moves a marker fails the build rather than the oracle. To cover another function, add it to the tables in freeswitch-c-oracle/build.rs, stub what it calls in the prelude, expose it on `Oracle`, and write a property that runs through against_the_c, or through on_every_tree in the crate's own tests where nothing in freeswitch-types ports it.

To track another tree, add its name and commit under trees in hooks/source-refs.yaml, with fetch naming its public remote when it has one; check-source-refs.py carries the entry through --update. A property whose expectation differs between trees names the tree it expects, as the URL-encoding rules in freeswitch-c-oracle/tests/url_encode.rs do, and fails on a tree it has no rule for.

Without `FREESWITCH_SOURCE`, or for a tree whose commit the clone lacks, the build prints a cargo warning naming the tree and the reason, and each property prints one skip line for that tree and passes. The pre-commit hook inherits the variable, and its source-reference check refuses a commit without it, so the oracle runs there on every tree the clone holds. CI fetches the pin and every tree with a public remote shallowly and fails when one of those is not built; a tree without one is skipped with a line saying so.
