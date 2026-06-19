#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

work_dir="${SOLJSON_DIFF_DIR:-target/soljson-diff}"
mkdir -p "$work_dir"

jobs="${JOBS:-}"
if [[ -z "$jobs" ]]; then
    if command -v nproc >/dev/null 2>&1; then
        jobs="$(nproc)"
    else
        jobs="4"
    fi
fi

if [[ $# -gt 0 ]]; then
    input="$1"
else
    input="$work_dir/input.json"
    cat > "$input" <<'JSON'
{
  "language": "Solidity",
  "sources": {
    "A.sol": {
      "content": "pragma solidity ^0.8.0; contract A { function f() public pure { uint x = true; } }"
    }
  },
  "settings": {
    "outputSelection": {
      "*": {
        "*": ["abi"],
        "": ["ast"]
      }
    }
  }
}
JSON
fi

if [[ -n "${SOLC:-}" ]]; then
    solc="$SOLC"
else
    solidity_build="$work_dir/solidity-build"
    solc="$solidity_build/solc/solc"
    if [[ ! -x "$solc" ]]; then
        cmake -S testdata/solidity -B "$solidity_build" -G Ninja \
            -DCMAKE_BUILD_TYPE=Release \
            -DTESTS=OFF \
            -DPEDANTIC=OFF \
            -DSTRICT_Z3_VERSION=OFF
        cmake --build "$solidity_build" --target solc --parallel "$jobs"
    fi
fi

scripts/wasm/dist-wasm.sh

wasm="target/dist/solar.wasm"
wrapper="target/dist/soljson-wrapper.js"
solc_out="$work_dir/solc.json"
solar_out="$work_dir/solar-wasm.json"
solc_norm="$work_dir/solc.normalized.json"
solar_norm="$work_dir/solar-wasm.normalized.json"

"$solc" --standard-json < "$input" > "$solc_out"

node - "$wrapper" "$wasm" "$input" "$solar_out" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const [, , wrapperPath, wasmPath, inputPath, outputPath] = process.argv;
const wrapper = require(path.resolve(wrapperPath));
const wasm = fs.readFileSync(wasmPath);

(async () => {
  const { instance } = await WebAssembly.instantiate(wasm, {});
  const soljson = {
    instance,
    exports: instance.exports,
    memory: instance.exports.memory,
  };
  const compiler = wrapper.setupMethods(soljson);
  const input = fs.readFileSync(inputPath, "utf8");
  fs.writeFileSync(outputPath, compiler.compile(input) + "\n");
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
NODE

node - "$solc_out" "$solc_norm" "$solar_out" "$solar_norm" <<'NODE'
const fs = require("node:fs");

function normalize(path) {
  const output = JSON.parse(fs.readFileSync(path, "utf8"));
  const errors = (output.errors || []).filter((error) => error.severity === "error");
  return {
    accepted: errors.length === 0,
    errorCount: errors.length,
    errorFiles: [...new Set(errors.map((error) => error.sourceLocation?.file ?? null))],
  };
}

for (let i = 2; i < process.argv.length; i += 2) {
  fs.writeFileSync(process.argv[i + 1], JSON.stringify(normalize(process.argv[i]), null, 2) + "\n");
}
NODE

diff -u "$solc_norm" "$solar_norm"
