# Release checklist

This checklist is meant to be used as a guide for the `crates.io` release process.

Releases are always made in lockstep, meaning that all crates in the repository
are released with the same version number, regardless of whether they have
changed or not.

Publish the crates last. Pushing the tag starts the release workflow, and a
failure there only needs a fix on `main` and a new tag while nothing is on
`crates.io` yet. This keeps the crates, the tag, and the release binaries on one
commit. Once the publish job has uploaded a crate, rerun it instead of tagging
again.

## One-time setup

The release workflow publishes with crates.io trusted publishing, so no
`CARGO_REGISTRY_TOKEN` secret exists. Set this up once:

- [ ] In the repository settings, create the GitHub environment `crates-io`.
  Limit its deployment branches and tags to the `v*` tag pattern and add a
  required reviewer. The trust that crates.io gives the workflow follows push
  access to the repository, so the reviewer is the control on publishing.
- [ ] For each crate below, open crates.io → the crate → Settings → Trusted
  Publishing → Add → GitHub, and enter: repository owner `paradigmxyz`,
  repository `solar`, workflow filename `release.yml`, environment `crates-io`.
  crates.io matches the `workflow_ref` claim of the OIDC token, which names the
  top-level workflow that the tag push starts (`release.yml`), even though the
  publish job is the reusable `publish-crates.yml`; do not enter
  `publish-crates.yml`. The publish job runs in the `crates-io` environment, so
  the environment must match exactly.
- [ ] A new crate needs one manual first publish with an API token before
  crates.io accepts a trusted publisher for it. Every crate below is already
  published.

Crates: `solar-compiler`, `solar-ast`, `solar-capi`, `solar-cli`, `solar-codegen`,
`solar-config`, `solar-data-structures`, `solar-evmir`, `solar-evmir-interpreter`,
`solar-interface`, `solar-ir`, `solar-lint`, `solar-lsp`, `solar-macros`, `solar-mir`,
`solar-mir-interpreter`, `solar-parse`, `solar-sema`.

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
- [ ] Wait for the release workflow to publish the GitHub release. If it fails
  before the publish job uploads a crate, fix the cause on `main`, delete the
  remote tag, and tag the fix.
- [ ] Check the GitHub release: the notes render, every artifact from `dist plan`
  is attached, and a downloaded binary reports `solar <version>+commit...`
  without `-dev`.

## Publish

- [ ] The tag's release workflow publishes the crates after it creates the GitHub
  release (`.github/workflows/publish-crates.yml`, `cargo publish` with trusted
  publishing). The job waits for approval of the `crates-io` deployment: approve
  it only after the GitHub release check above passes. It runs only for stable
  `v<version>` tags, checks that the tag still points at the commit it built,
  and publishes only the crates that are missing from crates.io. Check that the
  `custom-publish-crates` job passed and that every crate shows the new version
  on crates.io. If it fails, fix the cause and rerun the failed job.

## Downstream

- [ ] In foundry-core and Foundry, set the solar requirements to `=<version>`
  and point the solar `[patch.crates-io]` entries at `tag = "v<version>"`.
- [ ] Raise their MSRV if solar's `rust-version` is higher.

These steps are adapted from the recommended `cargo-release` + `dist` workflow, described in more detail [here](https://opensource.axo.dev/cargo-dist/book/workspaces/cargo-release-guide.html#using-cargo-release-with-pull-requests).
