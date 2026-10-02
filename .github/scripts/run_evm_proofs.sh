#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
uv run scripts/evm-rules/verify.py verify --jobs 2 \
  --output "$output/proofs.json" --artifacts "$output/proofs"
