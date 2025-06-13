#!/usr/bin/env bash
# Build script for cargo-dist that applies PGO when possible
set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[PGO-BUILD]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[PGO-BUILD]${NC} $1"
}

log_error() {
    echo -e "${RED}[PGO-BUILD]${NC} $1"
}

# Detect host and target architectures
HOST_TARGET=$(rustc -vV | sed -n 's/host: //p')
BUILD_TARGET="${CARGO_DIST_TARGET:-$HOST_TARGET}"

log_info "Host target: $HOST_TARGET"
log_info "Build target: $BUILD_TARGET"

# Function to perform standard build
standard_build() {
    log_info "Performing standard optimized build"
    cargo build --profile dist --locked --target "$BUILD_TARGET"
}

# Function to perform PGO build
pgo_build() {
    log_info "Performing PGO-optimized build"
    
    # Check if cargo-pgo is installed
    if ! command -v cargo-pgo &> /dev/null; then
        log_warn "cargo-pgo not found, attempting to install..."
        cargo install cargo-pgo || {
            log_error "Failed to install cargo-pgo, falling back to standard build"
            standard_build
            return
        }
    fi
    
    # Check if llvm-tools-preview is installed
    if ! rustup component list --installed | grep -q llvm-tools-preview; then
        log_warn "llvm-tools-preview not found, attempting to install..."
        rustup component add llvm-tools-preview || {
            log_error "Failed to install llvm-tools-preview, falling back to standard build"
            standard_build
            return
        }
    fi
    
    # Clean any existing PGO data
    log_info "Cleaning previous PGO data..."
    cargo pgo clean
    
    # Build instrumented binary
    log_info "Building instrumented binary..."
    cargo pgo build -- --profile dist --features "cli asm mimalloc" || {
        log_error "Failed to build instrumented binary, falling back to standard build"
        standard_build
        return
    }
    
    # Run profiling workload
    log_info "Running profiling workload..."
    # Use testdata files for profiling, handling potential errors gracefully
    if [ -d "testdata" ] && ls testdata/*.sol 1> /dev/null 2>&1; then
        cargo pgo run -- --profile dist --features "cli asm mimalloc" -- \
            testdata/*.sol > /dev/null 2>&1 || {
            log_warn "Some profiling runs failed, continuing with partial profile data"
        }
    else
        log_warn "No testdata/*.sol files found, using minimal profiling"
        # Run with --help as minimal profiling
        cargo pgo run -- --profile dist --features "cli asm mimalloc" -- --help > /dev/null 2>&1
    fi
    
    # Build optimized binary using profile data
    log_info "Building PGO-optimized binary..."
    cargo pgo optimize build -- --profile dist --features "cli asm mimalloc" --target "$BUILD_TARGET" || {
        log_error "Failed to build PGO-optimized binary, falling back to standard build"
        standard_build
        return
    }
    
    log_info "PGO build completed successfully"
}

# Main logic
if [ "$HOST_TARGET" = "$BUILD_TARGET" ]; then
    log_info "Native build detected, attempting PGO optimization"
    
    # Special handling for musl target
    if [[ "$BUILD_TARGET" == *"musl"* ]]; then
        log_warn "musl target detected, PGO may not work optimally"
        # Still attempt PGO but be ready to fall back
    fi
    
    pgo_build
else
    log_info "Cross-compilation detected, PGO not available"
    standard_build
fi
