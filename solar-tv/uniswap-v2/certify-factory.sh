#!/usr/bin/env bash
# Certifies UniswapV2Factory's backend stages with solar-tv/milestone/certify.sh.
#
#     solar-tv/uniswap-v2/certify-factory.sh [--v2-core DIR]
#
# v2-core is GPL-3.0, so the repository keeps no copy of it: port.py writes the ^0.8 port to
# target/uniswap-v2/src, this script flattens the factory's unit into one file there (the
# order below, imports and repeated headers dropped), and certify.sh copies it to
# solar-tv/milestone/uni_factory. The factory's runtime has no loops, so certify.sh proves it
# without loop bounds; it embeds the pair's creation code as program data, which the
# assembly walk leaves after the code it covers. The run proofs take more steps and memory
# than certify.sh's defaults: CERTIFY_HEARTBEATS=0 lifts the heartbeat bound (the kernel
# checks every proof whatever it is) and LEAN_MEMORY defaults to 13800 MB here (stack scheduling's
# proof needs more than 12000 in the kernel).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
src=target/uniswap-v2/src
python3 solar-tv/uniswap-v2/port.py --out "$src" "$@"
flat=target/uniswap-v2/factory.sol
{
    echo "// SPDX-License-Identifier: GPL-3.0"
    echo "pragma solidity ^0.8.0;"
    for f in interfaces/IUniswapV2Factory.sol interfaces/IUniswapV2Pair.sol \
        interfaces/IUniswapV2ERC20.sol interfaces/IERC20.sol interfaces/IUniswapV2Callee.sol \
        libraries/SafeMath.sol libraries/Math.sol libraries/UQ112x112.sol \
        UniswapV2ERC20.sol UniswapV2Pair.sol UniswapV2Factory.sol; do
        echo
        echo "// $f"
        grep -v -E '^(// SPDX-License-Identifier|pragma solidity|import )' "$src/$f"
    done
} > "$flat"
export CERTIFY_HEARTBEATS="${CERTIFY_HEARTBEATS:-0}" LEAN_MEMORY="${LEAN_MEMORY:-13800}"
exec solar-tv/milestone/certify.sh "$flat" UniswapV2Factory --name uni_factory
