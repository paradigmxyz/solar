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

## Start the next version

- [ ] Bump `main` to the next patch version as a `-dev` pre-release, such as
  `0.3.1-dev` after `0.3.0`, in a PR named `chore: bump to <next>-dev`:
  `cargo release version --execute <next>-dev`. Builds read the release from the
  version, because Cargo git checkouts have no tags, so merge nothing else to
  `main` between the release PR and this bump.

## Publish

- [ ] Publish the crates from the tag: `git checkout v<version>` and
  `cargo release publish --execute [--no-verify]`

## Downstream

- [ ] In foundry-core and Foundry, set the solar requirements to `=<version>`
  and point the solar `[patch.crates-io]` entries at `tag = "v<version>"`.
  To patch them to an unreleased `main` commit instead, set the requirements to
  its `=<next>-dev` version.
- [ ] Raise their MSRV if solar's `rust-version` is higher.

These steps are adapted from the recommended `cargo-release` + `dist` workflow, described in more detail [here](https://opensource.axo.dev/cargo-dist/book/workspaces/cargo-release-guide.html#using-cargo-release-with-pull-requests).
