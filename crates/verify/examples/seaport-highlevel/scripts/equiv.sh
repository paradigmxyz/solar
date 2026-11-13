#!/usr/bin/env bash
# Checks the optimized Seaport 1.6 runtime against the high-level rewrite,
# function by function, with the pinned Foundry nightly's forge equivalence.
# EQUIV_MODE=tstore (as deployed on Cancun, default) or sstore (as deployed
# where the TLOAD probe fails, so the guard lives in storage until
# __activateTstore; see sstore_mode.py).
# Usage: scripts/equiv.sh [signature...]; default: every function in SIGS.
set -uo pipefail
cd "$(dirname "$0")/.."
SIGS=(
  "getCounter(address)"
  "getContractOffererNonce(address)"
  "getOrderStatus(bytes32)"
  "information()"
  "name()"
  "incrementCounter()"
  "__activateTstore()"
)
[ $# -gt 0 ] && SIGS=("$@")
MODE=${EQUIV_MODE:-tstore}
mkdir -p equiv
forge test --mt test_dumpRuntime -q >/dev/null || { echo "dump failed"; exit 1; }
if [ "$MODE" = sstore ]; then
  scripts/sstore_mode.py lib/seaport/optimized-out/Seaport.sol/Seaport.json \
    equiv/optimized-tstore.hex equiv/optimized-sstore.hex || exit 1
  scripts/sstore_mode.py out/ReferenceConsideration.sol/ReferenceConsideration.json \
    equiv/highlevel-tstore.hex equiv/highlevel-sstore.hex || exit 1
fi
A=equiv/optimized-$MODE.hex
B=equiv/${EQUIV_B:-highlevel}-$MODE.hex
fail=0
for sig in "${SIGS[@]}"; do
  out=$(timeout "${EQUIV_WALL:-900}" forge equivalence -j1 \
    --code-a-file "$A" --code-b-file "$B" \
    --sig "$sig" --ignore-out-of-gas --timeout "${EQUIV_TIMEOUT:-600}" 2>&1)
  code=$?
  last=$(printf '%s\n' "$out" | grep -E '^(Equivalent|Not equivalent|Error)' | head -1)
  printf '%-6s %-36s %s\n' "$MODE" "$sig" "$last"
  if [ $code -ne 0 ]; then
    fail=1
    printf '%s\n' "$out" | grep -E '^\s+(args|code-a|code-b|sender)' | sed 's/^/    /'
  fi
done
exit $fail
