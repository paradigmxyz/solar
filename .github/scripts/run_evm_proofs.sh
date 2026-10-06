#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
mkdir -p "$output"
lean --version
# Every selected theorem and applicability check is proved afresh by two reusable workers.
uv run scripts/evm-rules/verify.py verify --jobs 2 \
  --work-dir "$output/theorems" --output "$output/proofs.json"
