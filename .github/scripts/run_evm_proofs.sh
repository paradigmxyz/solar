#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
mkdir -p "$output"
lean --version
# Every selected rule is one Lean theorem, proved afresh in its own process on every core.
uv run scripts/evm-rules/verify.py verify \
  --work-dir "$output/theorems" --output "$output/proofs.json"
