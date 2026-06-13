#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

rustup target add wasm32-unknown-unknown
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=--export-table -C link-arg=--growable-table"
cargo build -p solar-capi --lib --profile minsize --no-default-features --target wasm32-unknown-unknown

out_dir="target/dist"
mkdir -p "$out_dir"
cp target/wasm32-unknown-unknown/minsize/solar_capi.wasm "$out_dir/solar.wasm"
cp crates/capi/soljson.js "$out_dir/soljson-wrapper.js"
scripts/wasm/pack-soljson.sh "$out_dir/solar.wasm" "$out_dir/soljson-wrapper.js" "$out_dir/soljson.js"

bundle_dir="$out_dir/solar-wasm"
mkdir -p "$bundle_dir"
cp "$out_dir/solar.wasm" "$bundle_dir/solar.wasm"
cp "$out_dir/soljson-wrapper.js" "$bundle_dir/soljson-wrapper.js"
cp "$out_dir/soljson.js" "$bundle_dir/soljson.js"
tar -C "$out_dir" -czf "$out_dir/solar-wasm.tar.gz" \
  solar-wasm/solar.wasm \
  solar-wasm/soljson-wrapper.js \
  solar-wasm/soljson.js
