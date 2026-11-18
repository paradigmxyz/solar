# Release checklist

This checklist is meant to be used as a guide for the `crates.io` release process.

Releases are always made in lockstep, meaning that all crates in the repository
are released with the same version number, regardless of whether they have
changed or not.

Publish the crates last. Pushing the tag starts the release workflow, and a
failure there only needs a fix on `main` and a new tag while nothing is on
`crates.io` yet. This keeps the crates, the tag, and the release binaries on one
commit.

## Requirements

- [cargo-release](https://github.com/crate-ci/cargo-release): `cargo install cargo-release`
- [git-cliff](https://github.com/orhun/git-cliff) (patched with [orhun/git-cliff#711](https://github.com/orhun/git-cliff/pull/711)): `cargo install --git https://github.com/DaniPopes/git-cliff.git --branch fix-include-paths git-cliff`
- [dist](https://github.com/astral-sh/cargo-dist): `cargo install cargo-dist`

## Prepare

- [ ] Check that the last weekly release build passed:
  `gh run list --workflow build-release-binaries.yml --branch main -L 1`. If it
  is stale, run it on `main` with `gh workflow run build-release-binaries.yml`.
- [ ] Check that the tag does not exist yet, locally or on the remote:
  `git tag -l v<version>` and `git ls-remote --tags origin v<version>`. Delete
  local tags fetched from the `testdata/solidity` submodule, which use the same
  `v0.x.y` names.
- [ ] Create a new branch: `git checkout -b release-<version>`
- [ ] Run `cargo-release` to handle the version bump, changelog, and commit: `cargo release --execute --no-publish --no-tag --allow-branch=<branch> <version>`
- [ ] Check the changelogs with `grep -n '^## ' CHANGELOG.md crates/*/CHANGELOG.md`.
  Every version must appear once; git-cliff can prepend older releases again.
  Keep `crates/solar/CHANGELOG.md` a symlink to the root changelog.
- [ ] Edit the changelog: drop dependency bumps, CI, bench, test, refactor, and
  chore entries, and add a short `### Highlights` list with linked PRs.
- [ ] Update the pinned versions in `README.md`'s `### Library usage` and `### Binary usage`.
- [ ] Push, open the PR, and wait for CI to pass before merging. The name of the
  PR should be the same as the `cargo-release` commit message.

## Tag and build

- [ ] `git checkout main` and `git pull`.
- [ ] Verify `dist plan` is correct.
- [ ] Tag and push the tag: `cargo release tag --execute` and `cargo release push --execute`.
- [ ] Wait for the release workflow to publish the GitHub release. If it fails,
  fix the cause on `main`, delete the remote tag, and tag the fix.
- [ ] Check the GitHub release: the notes render, every artifact from `dist plan`
  is attached, and a downloaded binary reports `solar <version>+commit...`
  without `-dev`.

## Publish

- [ ] Publish the crates: `cargo release publish --execute [--no-verify]`

## Downstream

- [ ] In foundry-core and Foundry, set the solar requirements to `=<version>`
  and point the solar `[patch.crates-io]` entries at `tag = "v<version>"`.
- [ ] Raise their MSRV if solar's `rust-version` is higher.

These steps are adapted from the recommended `cargo-release` + `dist` workflow, described in more detail [here](https://opensource.axo.dev/cargo-dist/book/workspaces/cargo-release-guide.html#using-cargo-release-with-pull-requests).

## Changelog format

The root `CHANGELOG.md` is the release note. `crates/solar/CHANGELOG.md` stays a symlink to it, and
each version appears once in every changelog file. `cargo release` runs `git-cliff` with
[`cliff.toml`](cliff.toml), which writes the area sections below from conventional-commit types and
scopes. Edit its output by hand into this order:

1. **Breaking changes and compatibility.** Only when there is something to say; cliff fills it from
   `!` commits, and you add the rest. List the MSRV, a changed default (and what it costs), code that
   used to compile and now errors, output that changes for the same input (bytecode, ABI, NatSpec,
   source maps, flags, Standard JSON), and removed or renamed flags. Say who is affected.
2. **Highlights.** Written by hand, grouped by the area names below, a few bullets per group with
   linked PRs and numbers. Cover only what users notice.
3. **Area sections.** Generated, in this order, one line per user-facing change, newest first:
   Language and diagnostics; Code generation: gas and size; Code generation: features and fixes;
   Debug info and coverage; Output: ABI, AST, NatSpec and metadata; Compile time and memory; CLI and
   Standard JSON; Language server. Features, fixes and performance changes share a section, because
   the commit scope and subject already say which is which. Write a fix in the section of the
   behavior it changes: a codegen change that only makes source maps better belongs to Debug info
   and coverage.

The grouping comes from the commit, so keep the scope honest when you write one: `sema`, `parse`,
`ast` and `lint` land in Language and diagnostics; `codegen`, `mir`, `evmir`, `ir` and `inline` in
Code generation (`perf` goes to gas and size); `cli`, `config` and `interface` in CLI and Standard
JSON; `lsp` in Language server. A `perf` with a frontend scope, or none, goes to Compile time and
memory. Subjects that mention debug info, origins, source maps, spans or coverage go to Debug info
and coverage, and subjects that mention NatSpec, the AST, the ABI, metadata or storage layout go to
Output. A commit that matches nothing lands in Other: move it to its area or delete it. To change a
rule, edit `commit_parsers` in `cliff.toml`; the numbers in the group names order the sections.

### What to drop

- Dependency bumps, CI, bench, tester, test, refactor, chore, docs about agents or the release
  process, and merge commits. Cliff already skips these by type and scope.
- Revert pairs: delete both the change and its revert.
- Fixes for a bug introduced in the same release, and follow-ups to a change in the same release.
- Runs of related internal fixes. Collapse them into the one entry that names the user-visible result,
  such as "Source maps keep the origin of code through rewrites", and keep the PRs that explain it.
- Compile-time wins that cliff filed under codegen go to Compile time and memory; move them.

### Numbers

Use only measurements that a merged PR body states, with the PR linked next to the number. Quote
the baseline and the result, name the input (`seaport-1.6`, `prb-math`, a 6,132-file corpus), and
give the unit and the build level (`-Ogas`, `-Osize`, `-j1`). Report regressions and trade-offs the
PR states, such as a contract that grows or memory that rises. Do not average across PRs or round up.
If a PR gives no number, describe the change without one.

### Why this format

solc lists "Language Features", "Compiler Features", "Bugfixes" and "Build System", and prefixes each
line with a component such as "Code Generator:" or "SMTChecker:". rustc groups by audience (Language,
Compiler, Libraries, Cargo, Compatibility Notes) and keeps compatibility separate. Go and LLVM go by
component (compiler, linker, runtime; IR, debug info, backends) after a short list of the main changes.
We list areas, because a Solidity developer looks for what changes in the bytecode, the diagnostics
and the build time, and most of our commits share the `codegen` scope, so the scope alone says little.
The area carries what solc's component prefix and rustc's section names carry.
