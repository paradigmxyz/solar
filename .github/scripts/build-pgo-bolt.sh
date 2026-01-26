#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

PROFILE="${PROFILE:-dist}"
FEATURES="${FEATURES:-cli,asm,mimalloc}"
CARGO_ARGS=(--profile "$PROFILE" --features "$FEATURES")

TARGET=$(rustc -Vv | grep host | cut -d' ' -f2)
LLVM_VERSION=$(rustc -Vv | grep -oP 'LLVM version: \K\d+')

install_bolt() {
    if command -v llvm-bolt &>/dev/null; then
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

cargo install cargo-pgo
rustup component add llvm-tools-preview
install_bolt

readarray -t TESTDATA < <(find testdata -maxdepth 1 -name '*.sol' ! -name 'Optimism.sol')

# PGO: build instrumented, run, gather profiles
cargo pgo build -- "${CARGO_ARGS[@]}"
"target/$TARGET/release/solar" "${TESTDATA[@]}"

# BOLT: build instrumented with PGO, run, optimize
cargo pgo bolt build --with-pgo -- "${CARGO_ARGS[@]}"
"target/$TARGET/$PROFILE/solar-bolt-instrumented" "${TESTDATA[@]}" || true
cargo pgo bolt optimize --with-pgo -- "${CARGO_ARGS[@]}"

cp "target/$TARGET/$PROFILE/solar-bolt-optimized" "target/$PROFILE/solar"
ls -lh "target/$PROFILE/solar"
