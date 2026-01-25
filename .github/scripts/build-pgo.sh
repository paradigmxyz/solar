#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

cargo install cargo-pgo
rustup component add llvm-tools-preview

cargo pgo build -- --profile dist --features cli,asm,mimalloc
find testdata -maxdepth 1 -name '*.sol' ! -name 'Optimism.sol' -exec \
    ./target/x86_64-unknown-linux-gnu/dist/solar {} +

llvm-profdata merge -o target/pgo-profiles/merged.profdata target/pgo-profiles
echo "RUSTFLAGS=-Cprofile-use=$PWD/target/pgo-profiles/merged.profdata" >> "$GITHUB_ENV"
