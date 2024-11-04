# Release checklist

This checklist is meant to be used as a guide for the `crates.io` release process.

Releases are always made in lockstep, meaning that all crates in the repository
are released with the same version number, regardless of whether they have
changed or not.

## Requirements

- [cargo-release](https://github.com/crate-ci/cargo-release): `cargo install cargo-release`
- [dist](https://github.com/axodotdev/cargo-dist): `cargo install cargo-dist`

## Steps

- [ ] Create a new branch: `git checkout -b release-<version>`
- [ ] Update CHANGELOG.md with the new version and the changes since the last release.
- [ ] Run `cargo-release` to handle the version bump and commit: `cargo release --execute --no-publish --no-tag --allow-branch=<branch> <version>`
- [ ] Push, open and merge the PR. The name of the PR should be the same as the `cargo-release` commit message.
- [ ] `git checkout main` and `git pull`.
- [ ] Verify `dist plan` is correct.
- [ ] Run `cargo-release` to tag and publish: `cargo release --execute [--no-verify] <version>`

These steps are adapted from the recommended `cargo-release` + `dist` workflow, described in more detail [here](https://opensource.axo.dev/cargo-dist/book/workspaces/cargo-release-guide.html#using-cargo-release-with-pull-requests).
