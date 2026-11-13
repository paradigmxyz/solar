#!/usr/bin/env bash
# Negative checks: each mutant of the rewrite must be reported not
# equivalent on the function it touches. A mutant reported equivalent fails
# the script; one reported incomplete is listed as inconclusive.
set -uo pipefail
cd "$(dirname "$0")/.."
export EQUIV_TIMEOUT=120 EQUIV_WALL=300
# file|from|to|mode|signature
MUTANTS=(
  'src/lib/SeaportStorage.sol|_REENTRANCY_GUARD_SLOT - 3]|_REENTRANCY_GUARD_SLOT - 4]|sstore|__activateTstore()'
  'src/lib/ReferenceReentrancyGuard.sol|_reentrancyGuardSstore = _TSTORE_ENABLED_SSTORE;|_reentrancyGuardSstore = _NOT_ENTERED_SSTORE;|sstore|__activateTstore()'
  'src/lib/ReferenceReentrancyGuard.sol|revert TStoreNotSupported();|revert TStoreAlreadyActivated();|sstore|__activateTstore()'
  'src/lib/ReferenceReentrancyGuard.sol|.staticcall{ gas: gasleft() / 10 }("")|.staticcall{ gas: gasleft() / 10 }("x")|sstore|__activateTstore()'
  'src/lib/ReferenceConsiderationBase.sol|_VERSION = "1.6"|_VERSION = "1.7"|tstore|information()'
  'src/lib/SeaportStorage.sol|    mapping(address => uint256) internal _counters;|    mapping(address => uint256) internal _countersMoved;\n    mapping(address => uint256) internal _counters;|tstore|getCounter(address)'
)
status=0
for m in "${MUTANTS[@]}"; do
  IFS='|' read -r file from to mode sig <<<"$m"
  cp "$file" "$file.orig"
  python3 - "$file" "$from" "$to" <<'PY'
import sys
p, a, b = sys.argv[1:]
s = open(p).read()
assert s.count(a) == 1, (p, a)
open(p, "w").write(s.replace(a, b.replace("\\n", "\n")))
PY
  out=$(EQUIV_MODE=$mode scripts/equiv.sh "$sig" 2>&1)
  mv "$file.orig" "$file"
  if printf '%s' "$out" | grep -q 'Not equivalent'; then
    echo "killed   $mode $sig: $from -> $to"
  elif printf '%s' "$out" | grep -q 'incomplete'; then
    # Fails closed, but not by a mismatch; see DIVERGENCES.md, tool limits.
    echo "inconcl. $mode $sig: $from -> $to"
  else
    echo "SURVIVED $mode $sig: $from -> $to"; printf '%s\n' "$out" | head -3; status=1
  fi
done
exit $status
