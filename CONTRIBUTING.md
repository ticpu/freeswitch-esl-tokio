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
