#!/usr/bin/env bash
# PGO (Profile-Guided Optimization) build script for cargo-dist releases.
#
# This script gathers PGO profiles from representative workloads and sets
# RUSTFLAGS so that cargo-dist's build uses the profiles.
set -euo pipefail

cd "$(dirname "$0")/../.."

PROFILE=dist
FEATURES=cli,asm,mimalloc

# Install cargo-pgo and llvm-tools-preview.
cargo install cargo-pgo
rustup component add llvm-tools-preview

# Get list of test files for profiling.
readarray -t TESTDATA < <(find testdata -maxdepth 1 -name '*.sol' ! -name 'Optimism.sol')
echo "Profiling with ${#TESTDATA[@]} files: ${TESTDATA[*]}"

# ============================================================================
# Step 1: Build PGO-instrumented binary
# ============================================================================
echo "=== Building PGO-instrumented binary ==="
cargo pgo build -- --profile "$PROFILE" --features "$FEATURES"

# ============================================================================
# Step 2: Gather PGO profiles
# ============================================================================
echo "=== Gathering PGO profiles ==="
cargo pgo run -- --profile "$PROFILE" --features "$FEATURES" -- "${TESTDATA[@]}"

# ============================================================================
# Step 3: Merge profiles and set RUSTFLAGS for cargo-dist build
# ============================================================================
echo "=== Merging PGO profiles ==="

# Find llvm-profdata from rustup's llvm-tools.
LLVM_PROFDATA=$(find "$(rustc --print sysroot)" -name llvm-profdata -type f | head -1)
if [[ -z "$LLVM_PROFDATA" ]]; then
    echo "Error: llvm-profdata not found"
    exit 1
fi

# Merge raw profiles into a single profdata file.
PROFILE_DIR="$PWD/target/pgo-profiles"
MERGED_PROFILE="$PROFILE_DIR/merged.profdata"
"$LLVM_PROFDATA" merge -o "$MERGED_PROFILE" "$PROFILE_DIR"

if [[ ! -f "$MERGED_PROFILE" ]]; then
    echo "Error: Failed to create merged profile at $MERGED_PROFILE"
    exit 1
fi

echo "=== Setting up RUSTFLAGS for PGO-optimized build ==="

# Set RUSTFLAGS for the subsequent dist build.
# -Cprofile-use: Use PGO profiles for optimization
RUSTFLAGS="-Cprofile-use=${MERGED_PROFILE}"

if [[ -n "${GITHUB_ENV:-}" ]]; then
    echo "RUSTFLAGS=${RUSTFLAGS}" >> "$GITHUB_ENV"
    echo "Exported RUSTFLAGS to GITHUB_ENV"
else
    echo "RUSTFLAGS=${RUSTFLAGS}"
    export RUSTFLAGS
fi

echo "=== PGO build setup complete ==="
