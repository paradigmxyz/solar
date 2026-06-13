#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

rustup target add wasm32-unknown-unknown
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=--export-table -C link-arg=--growable-table"
cargo build -p solar-cli --lib --profile minsize --no-default-features --target wasm32-unknown-unknown

out_dir="target/dist/solar-soljson"
mkdir -p "$out_dir"
cp target/wasm32-unknown-unknown/minsize/solar_cli.wasm "$out_dir/solar.wasm"
cp crates/cli/soljson.js "$out_dir/soljson-wrapper.js"
scripts/pack-soljson.sh "$out_dir/solar.wasm" "$out_dir/soljson-wrapper.js" "$out_dir/soljson.js"
