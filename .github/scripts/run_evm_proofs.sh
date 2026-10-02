#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
audit="${PROOF_AUDIT:-false}"
if [[ "$audit" != true && "$audit" != false ]]; then
  echo "PROOF_AUDIT must be true or false" >&2
  exit 1
fi
unset SOLAR_PROOF_CACHE
options=()
if [[ "$audit" != true ]]; then
  options+=(--cache-dir "${PROOF_CACHE_DIR:-target/evm-proof-cache}")
fi
mkdir -p "$output"
lean --version
# Every selected rule is one Lean theorem, checked in its own process on every core.
uv run scripts/evm-rules/verify.py verify ${options[@]+"${options[@]}"} \
  --work-dir "$output/theorems" --output "$output/proofs.json"
