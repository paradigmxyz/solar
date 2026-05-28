#!/usr/bin/env bash
# Benchmark compile time for synthetic repros.
#
# Usage:
#   ./scripts/bench_compile_time.sh [size]
#
# Arguments:
#   size: small, medium, large (default: small)
#
# Examples:
#   ./scripts/bench_compile_time.sh small
#   ./scripts/bench_compile_time.sh large

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$REPO_ROOT"

SIZE="${1:-small}"
REPROS_DIR="testdata/repros"

if [[ ! -d "$REPROS_DIR" ]]; then
    echo "Repros not found. Generating..."
    cargo +nightly -Zscript scripts/gen_compile_repros.rs --sizes "$SIZE"
fi

echo "=== Solar Compile-Time Benchmarks (size: $SIZE) ==="
echo ""

# Build release binary
echo "Building release binary..."
cargo build --release --quiet 2>/dev/null

# Collect all repros for this size
REPROS=$(ls "$REPROS_DIR"/*_${SIZE}.sol 2>/dev/null || true)

if [[ -z "$REPROS" ]]; then
    echo "No repros found for size: $SIZE"
    exit 1
fi

# Print header
printf "%-30s %8s %10s %12s %12s\n" "Repro" "Lines" "Bytes" "Parse(ms)" "Sema(ms)"
printf "%-30s %8s %10s %12s %12s\n" "-----" "-----" "-----" "---------" "--------"

for repro in $REPROS; do
    name=$(basename "$repro" .sol)
    lines=$(wc -l < "$repro" | tr -d ' ')
    bytes=$(wc -c < "$repro" | tr -d ' ')
    
    # Time parse phase (just parsing, no sema)
    parse_time=$(./target/release/solar "$repro" --stop-after=parsing 2>&1 | grep -oE '[0-9]+\.[0-9]+ms' | head -1 || echo "N/A")
    
    # Time full compile (parse + sema)
    start=$(gdate +%s.%N 2>/dev/null || date +%s.%N 2>/dev/null || echo "0")
    ./target/release/solar "$repro" --emit=hir 2>&1 >/dev/null || true
    end=$(gdate +%s.%N 2>/dev/null || date +%s.%N 2>/dev/null || echo "0")
    
    if [[ "$start" != "0" && "$end" != "0" ]]; then
        sema_time=$(echo "scale=2; ($end - $start) * 1000" | bc)
    else
        sema_time="N/A"
    fi
    
    printf "%-30s %8s %10s %12s %12s\n" "$name" "$lines" "$bytes" "${parse_time:-N/A}" "${sema_time:-N/A}"
done

echo ""
echo "=== Done ==="
