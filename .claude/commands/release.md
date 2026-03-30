Perform a release of the rust-freeswitch-platform-ng workspace.

Optional override: $ARGUMENTS (format: vX.Y.Z). If provided, use that version.

## Release Workflow

This is a two-crate workspace. `freeswitch-esl-tokio` depends on
`freeswitch-types`, so **types must be published first**.

### Pre-release checks

```sh
cargo fmt --all
cargo clippy --workspace --release -- -D warnings
cargo test --workspace --release
cargo test --test live_freeswitch -- --ignored
cargo build --workspace --release
cargo build --examples
cargo semver-checks check-release -p freeswitch-types
cargo semver-checks check-release -p freeswitch-esl-tokio
cargo publish --dry-run -p freeswitch-types
```

### Publish order

```sh
cargo publish -p freeswitch-types
cargo publish -p freeswitch-esl-tokio
```

**Never `cargo publish` without completing these steps first:**

1. Create signed annotated tags (`git tag -as`) with a brief changelog
   in the tag message (use `git log --oneline <previous-tag>..HEAD` to
   generate it)
2. Push the tags (`git push --tags`)
3. Wait for CI to pass on the tagged commit
4. Only then `cargo publish` (types first, then ESL)

## Version determination

1. Find the last release tag (`git tag --sort=-v:refname | head -1`).
2. Examine commits since that tag to classify the release type:
   - **Patch**: only bug fixes, dependency bumps, build changes, docs.
   - **Minor**: new features (`feat:`), new crates, new public API surface.
   - **Major**: breaking API changes, removed crates, incompatible config changes.
3. Bump the version accordingly from the last tag.
4. If the computed type is **major**, stop and confirm with the user before proceeding.
   For patch and minor, proceed automatically.

## Version bumping rules

- **Patch releases**: only bump the version of crates that were actually modified
  since the last release tag. Use `git log` from the last tag to identify changed
  crates.
- **Minor/major releases**: bump all workspace crates together.

## Steps

1. Identify the last release tag and which crates changed since then.

2. Bump `version` in the appropriate `Cargo.toml` files (`freeswitch-types/Cargo.toml`
   and/or the root `Cargo.toml` for `freeswitch-esl-tokio`).

3. Run the full release validation sequence — stop and report on any failure:

```sh
cargo fmt --all && \
cargo clippy --workspace --release -- -D warnings && \
cargo test --workspace --release && \
cargo test --test live_freeswitch -- --ignored && \
cargo build --workspace --release && \
cargo build --examples && \
cargo semver-checks check-release -p freeswitch-types && \
cargo semver-checks check-release -p freeswitch-esl-tokio && \
cargo publish --dry-run -p freeswitch-types
```

4. Draft a changelog from `git log --oneline <last-tag>..HEAD`.

   **Rules:**
   - Group entries under section headings: `New features:`, `Bug fixes:`,
     `Build:`, `Refactoring:` — omit empty sections.
   - Within each section, group by component/crate when there are multiple
     (e.g. `- freeswitch-types: ...`, `- freeswitch-esl-tokio: ...`).
   - Describe user-visible behavior, not implementation details.
   - Merge related commits for the same feature into one bullet.
   - No git hashes, no raw commit subjects, no co-author lines in the changelog.

   The tag annotation format is:
   ```
   vX.Y.Z

   New features:
   - component: what changed

   Bug fixes:
   - component: what was fixed

   Build:
   - what changed
   ```

5. Stage, commit, tag, and push in sequence:

```sh
git add freeswitch-types/Cargo.toml Cargo.toml
git commit -m "release: vX.Y.Z"
git tag -as vX.Y.Z -m "$(cat <<'EOF'
vX.Y.Z

<changelog>
EOF
)"
git push && git push --tags
```

6. Wait for CI to pass on the tagged commit, then publish:

```sh
cargo publish -p freeswitch-types
cargo publish -p freeswitch-esl-tokio
```

7. Report the tag and the changelog.

## Important

- **Never commit Cargo.lock** — this is a library crate. Cargo.lock stays gitignored.
- The tag is IMMUTABLE once pushed — never retag. If something is wrong,
  make a new patch release.
