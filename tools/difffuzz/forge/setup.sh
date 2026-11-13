#!/usr/bin/env bash
# Builds the UniswapV2 targets twice, with solc and with the compiler under test, into
# lib/uniswap-v2/out-solc and lib/uniswap-v2/out-solar.
#
#   bash tools/difffuzz/forge/setup.sh [path/to/solar] [path/to/port]
#
# The port defaults to Uniswap v2-core at the pinned commit with uniswap-v2-port.patch applied
# (README.md lists every change). Pass the vendored port's contracts directory to build that
# instead.
set -euo pipefail
cd "$(dirname "$0")"
SOLAR=$(realpath "${1:-../../../target/debug/solar}")
PORT=${2:-}
V2_CORE=6a9e7c97860676e0992f22a49665760444c1cdf5
FORGE_STD=77041d2ce690e692d6e03cc812b57d1ddaa4d505
NIGHTLY=5717c1b8fea92d59da287e56e8b6c798a3a36e74

forge --version | grep -q "$NIGHTLY" ||
    echo "warning: expected Foundry nightly $NIGHTLY (foundryup --install nightly-$NIGHTLY)" >&2

fetch() {
    [ -d "$2/.git" ] || git clone -q "$1" "$2"
    git -C "$2" checkout -q "$3"
}
fetch https://github.com/foundry-rs/forge-std lib/forge-std "$FORGE_STD"

rm -rf lib/uniswap-v2
mkdir -p lib/uniswap-v2
if [ -n "$PORT" ]; then
    cp -r "$PORT" lib/uniswap-v2/src
else
    fetch https://github.com/Uniswap/v2-core lib/v2-core "$V2_CORE"
    cp -r lib/v2-core/contracts lib/uniswap-v2/src
    patch -s -p1 -d lib/uniswap-v2/src < uniswap-v2-port.patch
fi

wrapper=$(mktemp -d)/solc
printf '#!/bin/sh\nexec %s "$@"\n' "$SOLAR" > "$wrapper"
chmod +x "$wrapper"

cd lib/uniswap-v2
printf '[profile.default]\nsrc = "src"\nsolc = "0.8.24"\nevm_version = "cancun"\noptimizer = true\noptimizer_runs = 999999\nbytecode_hash = "none"\n' > foundry.toml
forge build --out out-solc --cache-path cache-solc
SOLC_WRAPPER=1 SOLC_WRAPPER_VERSION=0.8.24 FOUNDRY_SOLC=$wrapper \
    forge build --out out-solar --cache-path cache-solar
