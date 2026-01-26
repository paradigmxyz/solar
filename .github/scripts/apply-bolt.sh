#!/usr/bin/env bash
# BOLT (Binary Optimization and Layout Tool) post-processing script.
#
# This script applies BOLT optimization to a PGO-optimized binary.
# It instruments the binary, gathers profiles, and applies BOLT optimizations.
#
# Usage: ./apply-bolt.sh <binary-path>
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <binary-path>"
    exit 1
fi

BINARY="$1"
if [[ ! -f "$BINARY" ]]; then
    echo "Error: Binary not found: $BINARY"
    exit 1
fi

cd "$(dirname "$0")/../.."

# Get LLVM version from rustc to install matching BOLT.
LLVM_VERSION=$(rustc -Vv | grep -oP 'LLVM version: \K\d+')
echo "Detected LLVM version: $LLVM_VERSION"

# Install BOLT from apt.llvm.org.
install_bolt() {
    echo "=== Installing BOLT (LLVM $LLVM_VERSION) ==="
    
    if command -v llvm-bolt &>/dev/null; then
        echo "llvm-bolt already installed"
        return
    fi
    
    # Add LLVM apt repository.
    wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc >/dev/null
    
    # Detect Ubuntu codename.
    CODENAME=$(lsb_release -cs)
    echo "deb http://apt.llvm.org/$CODENAME/ llvm-toolchain-$CODENAME-$LLVM_VERSION main" | sudo tee /etc/apt/sources.list.d/llvm.list >/dev/null
    
    sudo apt-get update -qq
    sudo apt-get install -y -qq "bolt-$LLVM_VERSION"
    
    # Symlink to standard names.
    sudo ln -sf "/usr/bin/llvm-bolt-$LLVM_VERSION" /usr/local/bin/llvm-bolt
    sudo ln -sf "/usr/bin/merge-fdata-$LLVM_VERSION" /usr/local/bin/merge-fdata
}

install_bolt

# Get test files for profiling.
readarray -t TESTDATA < <(find testdata -maxdepth 1 -name '*.sol' ! -name 'Optimism.sol')
echo "Profiling with ${#TESTDATA[@]} files"

# Paths for BOLT workflow.
INSTRUMENTED="${BINARY}.inst"
PROFILE_DIR="$PWD/target/bolt-profiles"
MERGED_PROFILE="$PROFILE_DIR/merged.fdata"
OPTIMIZED="${BINARY}.bolt"

mkdir -p "$PROFILE_DIR"

# ============================================================================
# Step 1: Instrument binary with BOLT
# ============================================================================
echo "=== Instrumenting binary with BOLT ==="
llvm-bolt "$BINARY" \
    -instrument \
    -instrumentation-file-append-pid \
    -instrumentation-file="$PROFILE_DIR/prof" \
    -o "$INSTRUMENTED"

# ============================================================================
# Step 2: Gather BOLT profiles
# ============================================================================
echo "=== Gathering BOLT profiles ==="
"$INSTRUMENTED" "${TESTDATA[@]}" || true

# ============================================================================
# Step 3: Merge profiles
# ============================================================================
echo "=== Merging BOLT profiles ==="
PROFILE_FILES=("$PROFILE_DIR"/prof.*.fdata)
if [[ ${#PROFILE_FILES[@]} -eq 0 ]]; then
    echo "Warning: No BOLT profiles generated, skipping optimization"
    exit 0
fi

merge-fdata "$PROFILE_DIR"/prof.*.fdata > "$MERGED_PROFILE"

# ============================================================================
# Step 4: Apply BOLT optimization
# ============================================================================
echo "=== Applying BOLT optimization ==="
llvm-bolt "$BINARY" \
    -o "$OPTIMIZED" \
    -data="$MERGED_PROFILE" \
    -reorder-blocks=ext-tsp \
    -reorder-functions=hfsort \
    -split-functions \
    -split-all-cold \
    -split-eh \
    -dyno-stats

# Replace original binary with optimized version.
mv "$OPTIMIZED" "$BINARY"
echo "=== BOLT optimization complete ==="
ls -lh "$BINARY"
