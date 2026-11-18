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

## Highlights

Each release's changelog opens with a `### Highlights` section, above the generated sections. Write
it by hand:

- Use 5 to 8 bullets, one per theme, most important first. Cover only what users notice most;
  everything else stays in the generated sections.
- Lead each bullet with a bold label, and group related PRs in one bullet. Use these labels where they
  fit: Code generation: gas and size; Debug info and coverage; Output (ABI, AST, NatSpec, metadata);
  Language and diagnostics; Correctness of generated code; Standard JSON; Language server; Compile
  time and memory. End with a Compatibility bullet when something breaks, such as a new MSRV, a
  changed default or code that stopped compiling.
- Link every PR as `[#N]`, with the `[#N]: https://github.com/paradigmxyz/solar/issues/N`
  definitions at the end of the version's section.
- Use only real measurements that a merged PR body states, next to a link to that PR. Give the
  baseline and the result, the input (`seaport-1.6`, `prb-math`) and the build level (`-Ogas`,
  `-Osize`, `-j1`), and state the regressions and trade-offs the PR states, such as a contract that
  grows or memory that rises. Do not average across PRs or round up. Without a number in the PR, write
  the change without one.

