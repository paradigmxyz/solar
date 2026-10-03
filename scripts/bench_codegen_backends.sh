#!/usr/bin/env bash
# Compare code generation backends with the shared runtime and project corpus.
# Usage: scripts/bench_codegen_backends.sh OUTPUT_DIR [benchmark.py arguments...]
set -euo pipefail

output_dir="${1:?Usage: scripts/bench_codegen_backends.sh OUTPUT_DIR [benchmark.py arguments...]}"
shift
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(dirname -- "$script_dir")"
cd "$repo_root"

if [[ -e "$output_dir" ]]; then
    echo "Output directory already exists: $output_dir" >&2
    exit 1
fi
mkdir -p "$output_dir"
cargo build -p solar-compiler --bin solar
failed=0
for backend in evm yul sonatina sir llvm; do
    if ! uv run benches/runtime/benchmark.py \
        --solar target/debug/solar --codegen-backend "$backend" \
        --evm-version osaka --mode runtime compile-time --suite all \
        --compile-repeats 5 --gas --gas-profile hot --start-anvil \
        --output "$output_dir/$backend/results.json" \
        --artifacts "$output_dir/$backend/artifacts" "$@"; then
        failed=1
    fi
    if [[ "$backend" != evm && -f "$output_dir/$backend/results.json" ]]; then
        if ! uv run benches/runtime/benchmark-compare.py \
            "$output_dir/evm" "$output_dir/$backend" \
            --report-output "$output_dir/$backend/comparison.md" \
            --json-output "$output_dir/$backend/comparison.json" \
            --diff-output "$output_dir/$backend/changes.patch"; then
            failed=1
        fi
    fi
done
exit "$failed"
