#!/usr/bin/env bash
set -eo pipefail

echo "::group::Install"
version=$(cargo metadata --format-version=1 |\
    jq '.packages[] | select(.name == "iai-callgrind").version' |\
    tr -d '"'
)
cargo binstall iai-callgrind-runner --version "$version" --no-confirm --no-symlinks --force
echo "::endgroup::"
echo "::group::Verification"
which iai-callgrind-runner
echo "::endgroup::"
