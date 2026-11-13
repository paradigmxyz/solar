#!/usr/bin/env bash
# Checks each UniswapV2 contract as built by solc against the build of the compiler under test
# with forge equivalence, one entry point at a time, after setup.sh. Writes one JSON line per
# check to code/equivalence.jsonl. Calls into the tokens are abstracted (--promise-no-reent),
# so re-entry into the pair is left to the fuzz tests.
#
#   bash tools/difffuzz/forge/equivalence.sh [timeout-seconds]
set -uo pipefail
cd "$(dirname "$0")"
T=${1:-600}
mkdir -p code
targets=(
    "UniswapV2Pair|mint(address) burn(address) swap(uint256,uint256,address,bytes) skim(address) sync() getReserves() initialize(address,address) price0CumulativeLast() kLast()"
    "UniswapV2ERC20|transfer(address,uint256) transferFrom(address,address,uint256) approve(address,uint256) permit(address,address,uint256,uint256,uint8,bytes32,bytes32) balanceOf(address) DOMAIN_SEPARATOR()"
    "UniswapV2Factory|createPair(address,address) setFeeTo(address) setFeeToSetter(address) allPairsLength() getPair(address,address)"
)
: > code/equivalence.jsonl
for t in "${targets[@]}"; do
    c=${t%%|*}
    for leg in solc solar; do
        jq -r .deployedBytecode.object "lib/uniswap-v2/out-$leg/$c.sol/$c.json" > "code/$c-$leg.hex"
    done
    for sig in ${t#*|}; do
        out=$(forge equivalence --code-a-file "code/$c-solc.hex" --code-b-file "code/$c-solar.hex" \
            --sig "$sig" --ignore-out-of-gas --promise-no-reent --timeout "$T" --json 2>&1)
        # A finished check prints one JSON object; an incomplete one prints only an error.
        json=$(printf '%s\n' "$out" | grep '^{' | tail -n 1)
        if [ -z "$json" ]; then
            json=$(jq -cn --arg e "$(printf '%s' "$out" | grep -m1 'Error')" '{status: "incomplete", error: $e}')
        fi
        jq -c --arg c "$c" --arg sig "$sig" '{contract: $c, sig: $sig} + .' <<< "$json" |
            tee -a code/equivalence.jsonl
    done
done
