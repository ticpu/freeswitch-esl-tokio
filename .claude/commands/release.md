Perform a release of the rust-freeswitch-platform-ng workspace.

Optional override: $ARGUMENTS (format: vX.Y.Z). If provided, use that version.

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

2. Bump `version` in the appropriate `crates/*/Cargo.toml` files.

3. Run the full release validation sequence — stop and report on any failure:

```sh
cargo clippy --release --message-format=short && cargo test --release -- --quiet && cargo build --release --bins --examples
```

4. Draft a changelog from `git log --oneline <last-tag>..HEAD`.

   **Rules:**
   - Group entries under section headings: `New features:`, `Bug fixes:`,
     `Build:`, `Refactoring:` — omit empty sections.
   - Within each section, group by component/crate in parentheses when
     there are multiple components (e.g. `- showcalls: ...`, `- fs-calld: ...`).
   - Describe user-visible or operator-visible behavior, not implementation
     details. "expose real TCP peer address" not "add peer_addr field to struct".
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

5. Stage, commit, tag, and build .deb in sequence:

```sh
git add crates/*/Cargo.toml
git add -f Cargo.lock
git commit -m "release: vX.Y.Z"
git tag -as vX.Y.Z -m "$(cat <<'EOF'
vX.Y.Z

<changelog>
EOF
)"
git push && git push --tags
make deb
```

The tag is IMMUTABLE once pushed — never retag.

6. Report the tag, the changelog, and the .deb file paths.

## Important

- Release commits must include the Cargo.lock update — the Makefile's `deb_version`
  uses `git diff-index` to detect a dirty tree.
- Never retag. If the tag was pushed and something is wrong, make a new patch release.
