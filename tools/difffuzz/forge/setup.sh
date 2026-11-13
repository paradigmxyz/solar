#!/usr/bin/env bash
# Fetches Seaport at the external suite's pinned revision into lib/seaport and builds four
# variants there: its optimized and reference implementations, each with solc (forge's own
# 0.8.24) and with the compiler under test.
#
#   tools/difffuzz/forge/setup.sh [path/to/solar]
set -euo pipefail
cd "$(dirname "$0")"
SOLAR=$(realpath "${1:-../../../target/debug/solar}")
REV=080133906585660f6a76b82984f3fb690ff4b2a9
NIGHTLY=5717c1b8fea92d59da287e56e8b6c798a3a36e74

forge --version | grep -q "$NIGHTLY" ||
    echo "warning: expected Foundry nightly $NIGHTLY (foundryup --install nightly-$NIGHTLY)" >&2

if [ ! -d lib/seaport/.git ] && [ ! -L lib/seaport ]; then
    git clone -q https://github.com/ProjectOpenSea/seaport lib/seaport
fi
git -C lib/seaport checkout -q "$REV"
git -C lib/seaport submodule update -q --init --recursive

wrapper=$(mktemp -d)/solc
printf '#!/bin/sh\nexec %s "$@"\n' "$SOLAR" > "$wrapper"
chmod +x "$wrapper"

cd lib/seaport
for p in optimized reference; do
    FOUNDRY_PROFILE=$p forge build --no-lint
    # Seaport pins solidity =0.8.24; the compiler reports that version through the wrapper.
    SOLC_WRAPPER=1 SOLC_WRAPPER_VERSION=0.8.24 FOUNDRY_SOLC=$wrapper FOUNDRY_OPTIMIZER=true \
        FOUNDRY_PROFILE=$p FOUNDRY_OUT=solar-$p-out FOUNDRY_CACHE_PATH=solar-$p-cache \
        forge build --no-lint
done
