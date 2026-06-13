#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

rustup target add wasm32-unknown-unknown
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=--export-table -C link-arg=--growable-table"
cargo build -p solar-cli --lib --release --no-default-features --target wasm32-unknown-unknown

out_dir="target/dist/solar-soljson"
mkdir -p "$out_dir"
cp target/wasm32-unknown-unknown/release/solar_cli.wasm "$out_dir/solar.wasm"
cp crates/cli/soljson.js "$out_dir/soljson.js"

tar -C target/dist -czf target/dist/solar-soljson.tar.gz solar-soljson
