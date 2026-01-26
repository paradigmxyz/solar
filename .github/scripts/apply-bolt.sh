#!/usr/bin/env bash
# BOLT optimization script using cargo-pgo.
#
# Assumes PGO profiles already exist (run build-pgo.sh first).
# Uses cargo-pgo's BOLT integration for instrumentation and optimization.
set -euo pipefail

cd "$(dirname "$0")/../.."

PROFILE=dist
FEATURES=cli,asm,mimalloc

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
    
    wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc >/dev/null
    CODENAME=$(lsb_release -cs)
    echo "deb http://apt.llvm.org/$CODENAME/ llvm-toolchain-$CODENAME-$LLVM_VERSION main" | sudo tee /etc/apt/sources.list.d/llvm.list >/dev/null
    sudo apt-get update -qq
    sudo apt-get install -y -qq "bolt-$LLVM_VERSION"
    sudo ln -sf "/usr/bin/llvm-bolt-$LLVM_VERSION" /usr/local/bin/llvm-bolt
    sudo ln -sf "/usr/bin/merge-fdata-$LLVM_VERSION" /usr/local/bin/merge-fdata
}

install_bolt

# Get test files for profiling.
readarray -t TESTDATA < <(find testdata -maxdepth 1 -name '*.sol' ! -name 'Optimism.sol')
echo "Profiling with ${#TESTDATA[@]} files"

# ============================================================================
# Step 1: Build BOLT-instrumented binary (with PGO)
# ============================================================================
echo "=== Building BOLT-instrumented binary ==="
cargo pgo bolt build --with-pgo -- --profile "$PROFILE" --features "$FEATURES"

# ============================================================================
# Step 2: Gather BOLT profiles
# ============================================================================
echo "=== Gathering BOLT profiles ==="
INSTRUMENTED="target/x86_64-unknown-linux-gnu/$PROFILE/solar-bolt-instrumented"
"$INSTRUMENTED" "${TESTDATA[@]}" || true

# ============================================================================
# Step 3: Build BOLT-optimized binary
# ============================================================================
echo "=== Building BOLT-optimized binary ==="
cargo pgo bolt optimize --with-pgo -- --profile "$PROFILE" --features "$FEATURES"

OPTIMIZED="target/x86_64-unknown-linux-gnu/$PROFILE/solar-bolt-optimized"
echo "=== BOLT optimization complete ==="
ls -lh "$OPTIMIZED"

# Copy to standard location for cargo-dist.
cp "$OPTIMIZED" "target/$PROFILE/solar"
echo "Copied to target/$PROFILE/solar"
