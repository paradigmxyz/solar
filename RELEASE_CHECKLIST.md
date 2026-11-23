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

## Publishing setup

The release workflow publishes with crates.io trusted publishing, so no
`CARGO_REGISTRY_TOKEN` secret exists. The current configuration:

- The GitHub environment `crates-io` limits deployments to the `v*` tag policy.
- Each crate below has the trusted publisher `paradigmxyz` / `solar` /
  `release.yml` / `crates-io` on crates.io. The workflow filename is the
  top-level `release.yml`, not the reusable `publish-crates.yml` that it calls.
- The `release` tag ruleset protects `refs/tags/v*`: it blocks creating,
  updating, deleting and force-pushing tags, and the maintain and admin roles
  bypass it.

A new crate needs one manual publish with an API token, then a trusted
publisher with the same values.

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
  chore entries, and add a `### Highlights` list as described in [Highlights](#highlights).
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
  release (`.github/workflows/publish-crates.yml`). It runs only for stable
  `v<version>` tags, checks that every crate has the tag's version and that the
  tagged commit is on `main`, and skips versions that are already on crates.io.
  Check that the `custom-publish-crates` job passed and that every crate shows
  the new version on crates.io. If it fails, fix the cause and rerun the failed
  job.

## Downstream

- [ ] In foundry-core and Foundry, set the solar requirements to `=<version>`
  and point the solar `[patch.crates-io]` entries at `tag = "v<version>"`.
- [ ] Raise their MSRV if solar's `rust-version` is higher.

These steps are adapted from the recommended `cargo-release` + `dist` workflow, described in more detail [here](https://opensource.axo.dev/cargo-dist/book/workspaces/cargo-release-guide.html#using-cargo-release-with-pull-requests).

## Highlights

Each release's changelog opens with a `### Highlights` section, above the generated sections. Write
it by hand, short, and about what users get, not about individual PRs:

- Use a few one-line bullets, most important first, and no PR lists or per-PR numbers. Link a PR or
  doc only where it helps the reader.
- Lead with headline numbers against solc, measured with the benchmark suite behind the blog post
  (`benches/runtime/benchmark.py`, one benchmark per `m4-metal-large` box, five compile repeats,
  solc 0.8.38, Cancun, Solar `--profile bench`) at Solar's default thread count (`-j0`, all cores),
  not `-j8`: compile speed as a geometric mean with the per-project range (via-IR with shared
  settings, and each project's own settings), and runtime gas, deploy gas and bytecode size over the
  runtime corpus. Shared settings force `viaIR`, relax every `pragma solidity` and use one EVM
  version for all compilers; `benchmark.py` on `main` has no switch for that, so use the override
  of commit `711c741f7` on `dani/tmp-solar-blog` (`BENCH_VIA_IR=1 BENCH_RELAX_PRAGMAS=1`) and check
  the effective settings and case IDs before aggregating. Say which main commit you measured.
- Add at most two or three user-facing features or behavior changes, such as a new output or a
  changed default.
- Use only real measurements. Do not average across runs, round up or quote a number you did not
  measure or read from a merged PR body.
