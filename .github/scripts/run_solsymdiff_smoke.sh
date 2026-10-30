#!/usr/bin/env bash
# Checks that solsymdiff agrees on faithful Solar builds. With
# SOLSYMDIFF_SELF_TEST=1 it also checks that solsymdiff reports a replayed
# mismatch for each kind of difference it is meant to catch.
set -euo pipefail

workspace="${GITHUB_WORKSPACE:-$(pwd)}"
solc="${SOLC:-solc}"
solar="${SOLAR:-$workspace/target/debug/solar}"
forge="${FORGE:-forge}"
solver="${SOLVER:-z3}"
out="${SOLSYMDIFF_OUT:-$workspace/target/solsymdiff-smoke}"
fixtures="$workspace/fuzz/fandango/symbolic-fixtures"
mutate="$fixtures/mutate.py"
fee5=0x0000000000000000000000000000000000000000000000000000000000000005

cd "$workspace"
mkdir -p "$out"
export SOLSYMDIFF_SOLC="$solc"

# check NAME EXPECTED_STATUS EXPECTED_LABEL COMPILER MUTATION ARGS...
check() {
  local name="$1" status="$2" label="$3" compiler="$4" mutation="$5"
  shift 5
  local code=0
  SOLSYMDIFF_MUTATION="$mutation" python3 fuzz/bin/solsymdiff \
    --solc "$solc" --solar "$compiler" --forge "$forge" --solver "$solver" \
    --output-root "$out" --timeout 900 --symbolic-caller \
    "$@" > "$out/$name.json" || code=$?
  python3 - "$out/$name.json" "$name" "$status" "$label" <<'PY'
import json, sys

path, name, status, label = sys.argv[1:]
result = json.load(open(path))
checks = result.get("checks", [])
failed = [c["label"] for c in checks if c["status"] == "mismatch"]
if result["status"] != status or (label and label not in failed):
    print(f"{name}: expected {status} {label}, got {result['status']}", file=sys.stderr)
    print(json.dumps(result, indent=2), file=sys.stderr)
    raise SystemExit(1)
print(f"{name}: {status} {label}".rstrip())
PY
  echo "  exit $code"
}

bank_contract=(--source "$fixtures/Bank.sol" --contract Bank)
bank=("${bank_contract[@]}" --all-functions)
vault=(--source "$fixtures/Vault.sol" --contract Vault --all-functions)

check bank bounded_agreement "" "$solar" "[]" "${bank[@]}" --constructor-args "$fee5"
check vault-arbitrary-storage bounded_agreement "" "$solar" "[]" "${vault[@]}" \
  --arbitrary-storage

# These check that the harness catches each kind of difference, using solc and
# a mutated solc; they never run Solar.
if [[ "${SOLSYMDIFF_SELF_TEST:-}" == 1 ]]; then
  check bank-constructor mismatch "fee()" "$mutate" \
    '["fee = f;", "fee = f == 7 ? 8 : f;"]' "${bank[@]}"
  check bank-created-contract mismatch "spawn(uint256)" "$mutate" \
    '["v = x;", "v = x == 5 ? 6 : x;"]' "${bank[@]}" --constructor-args "$fee5"
  check bank-fallback mismatch "<other calldata>" "$mutate" \
    '["count += 2;", "count += 3;"]' "${bank[@]}" --constructor-args "$fee5"
  # Only reachable after a deposit, so only a two-call sequence finds it.
  check bank-sequence mismatch "deposit() -> quote(uint256)" "$mutate" \
    '["return x * fee / 100;", "return count == 1 ? 0 : x * fee / 100;"]' \
    "${bank_contract[@]}" --constructor-args "$fee5" --signature 'quote(uint256)' \
    --depth 2
  check vault-unreachable-state mismatch "read()" "$mutate" \
    '["return total + level;", "return level == 7 ? 0 : total + level;"]' \
    "${vault[@]}" --arbitrary-storage
fi
