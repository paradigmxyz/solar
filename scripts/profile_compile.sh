#!/usr/bin/env bash
# Profile Solar compile times with tracing.
#
# Usage:
#   ./scripts/profile_compile.sh [repro_file.sol]
#
# Environment variables:
#   SOLAR_LOG=debug   Enable debug tracing
#   PROFILE=1         Generate samply profile
#
# Examples:
#   # Profile with tracing
#   SOLAR_LOG=debug ./scripts/profile_compile.sh testdata/repros/many_functions_large.sol
#
#   # Generate flame graph with samply
#   PROFILE=1 ./scripts/profile_compile.sh testdata/repros/many_functions_large.sol

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$REPO_ROOT"

# Default to a medium-sized repro
INPUT_FILE="${1:-testdata/repros/many_functions_medium.sol}"

if [[ ! -f "$INPUT_FILE" ]]; then
    echo "Error: Input file not found: $INPUT_FILE"
    echo "Generate repros first with: cargo +nightly -Zscript scripts/gen_compile_repros.rs"
    exit 1
fi

echo "=== Profiling: $INPUT_FILE ==="
echo "File size: $(wc -c < "$INPUT_FILE") bytes"
echo "Lines: $(wc -l < "$INPUT_FILE")"
echo ""

# Build release binary
echo "Building release binary..."
cargo build --release --quiet

if [[ "${PROFILE:-0}" == "1" ]]; then
    echo "Profiling with samply..."
    # Check if samply is installed
    if ! command -v samply &> /dev/null; then
        echo "Error: samply not found. Install with: cargo install samply"
        exit 1
    fi
    samply record -o "${INPUT_FILE%.sol}.json" -- \
        ./target/release/solar "$INPUT_FILE" --emit=hir 2>&1 || true
    echo "Profile saved to: ${INPUT_FILE%.sol}.json"
    echo "Open in Firefox Profiler: https://profiler.firefox.com/"
else
    # Run with timing and optional tracing
    echo "Compiling..."
    if [[ -n "${SOLAR_LOG:-}" ]]; then
        export RUST_LOG="${SOLAR_LOG}"
    fi
    
    # Use hyperfine if available for more accurate timing
    if command -v hyperfine &> /dev/null; then
        hyperfine --warmup 2 --min-runs 5 \
            "./target/release/solar $INPUT_FILE --emit=hir" \
            "./target/release/solar $INPUT_FILE --emit=abi"
    else
        # Simple timing
        echo "Parse + HIR:"
        time ./target/release/solar "$INPUT_FILE" --emit=hir 2>&1 || true
        echo ""
        echo "Parse + HIR + ABI:"
        time ./target/release/solar "$INPUT_FILE" --emit=abi 2>&1 || true
    fi
fi

echo ""
echo "=== Done ==="
