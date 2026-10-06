#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
mkdir -p "$output"
lean --version
# Every selected rule is one Lean theorem, proved afresh in its own single-threaded `lean`
# process; two run at a time, as on the standard CI runner.
uv run scripts/evm-rules/verify.py verify --jobs 2 \
  --work-dir "$output/theorems" --output "$output/proofs.json"
