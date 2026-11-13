#!/usr/bin/env bash
# Checks each Seaport implementation as built by solc against the same source built by the
# compiler under test, with forge equivalence, one entry point at a time, after setup.sh. The
# optimized and the reference implementation lay out storage differently, so they are compared
# by the fuzz and invariant tests instead. Writes one JSON line per check to code/equivalence.jsonl.
#
#   bash tools/difffuzz/forge/equivalence.sh [timeout-seconds]
set -uo pipefail
cd "$(dirname "$0")"
T=${1:-600}
mkdir -p code
forge script script/DumpCode.s.sol >/dev/null
pairs=(
    "solc-optimized solar-optimized"
    "solc-reference solar-reference"
)
sigs=(
    "name()"
    "information()"
    "getCounter(address)"
    "getOrderStatus(bytes32)"
    "incrementCounter()"
    "getContractOffererNonce(address)"
    "getOrderHash((address,address,(uint8,address,uint256,uint256,uint256)[],(uint8,address,uint256,uint256,uint256,address)[],uint8,uint256,uint256,bytes32,uint256,bytes32,uint256))"
    "cancel((address,address,(uint8,address,uint256,uint256,uint256)[],(uint8,address,uint256,uint256,uint256,address)[],uint8,uint256,uint256,bytes32,uint256,bytes32,uint256)[])"
    "fulfillBasicOrder((address,uint256,uint256,address,address,address,uint256,uint256,uint8,uint256,uint256,bytes32,uint256,bytes32,bytes32,uint256,(uint256,address)[],bytes))"
)
: > code/equivalence.jsonl
for pair in "${pairs[@]}"; do
    read -r a b <<< "$pair"
    for sig in "${sigs[@]}"; do
        out=$(forge equivalence --code-a-file "code/$a.hex" --code-b-file "code/$b.hex" \
            --sig "$sig" --timeout "$T" --json 2>&1)
        # A finished check prints one JSON object; an incomplete one prints only an error.
        json=$(printf '%s\n' "$out" | grep '^{' | tail -n 1)
        if [ -z "$json" ]; then
            json=$(jq -cn --arg e "$(printf '%s' "$out" | grep -m1 'Error')" '{status: "incomplete", error: $e}')
        fi
        jq -c --arg a "$a" --arg b "$b" --arg sig "$sig" '{a: $a, b: $b, sig: $sig} + .' <<< "$json" |
            tee -a code/equivalence.jsonl
    done
done
