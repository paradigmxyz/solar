#!/usr/bin/env bash
set -euo pipefail

solc_version="${SOLC_VERSION:?SOLC_VERSION must be set}"
fandango_version="${FANDANGO_VERSION:?FANDANGO_VERSION must be set}"
workspace="${GITHUB_WORKSPACE:-$(pwd)}"
runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
solc_path="$runner_temp/solc/solc-$solc_version"

cd "$workspace"
mkdir -p target/runtime-fuzz "$runner_temp/solc"

curl -fsSL \
  "https://github.com/ethereum/solidity/releases/download/v${solc_version}/solc-static-linux" \
  -o "$solc_path"
chmod +x "$solc_path"
"$solc_path" --version

cargo build -p solar-compiler --bin solar

anvil --silent --port 8545 > "$runner_temp/fandango-anvil.log" 2>&1 &
anvil_pid=$!
cleanup() {
  kill "$anvil_pid" 2>/dev/null || true
  wait "$anvil_pid" 2>/dev/null || true
}
trap cleanup EXIT

for _ in {1..30}; do
  if cast block-number --rpc-url http://127.0.0.1:8545 >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
cast block-number --rpc-url http://127.0.0.1:8545 >/dev/null

fandango() {
  PYTHONHASHSEED=1 uv tool run --quiet \
    --from "fandango-fuzzer==${fandango_version}" \
    fandango "$@"
}

python3 fuzz/fandango/encode_abi_vectors.py --seed corpus \
  < fuzz/fandango/corpus.jsonl \
  > target/runtime-fuzz/fandango-vectors.jsonl

for seed in 1 2 3; do
  fandango fuzz \
    -f fuzz/fandango/abi-values.fan \
    --random-seed "$seed" \
    -n 32 \
    --separator $'\n' \
    --progress-bar off \
    | python3 fuzz/fandango/encode_abi_vectors.py --seed "$seed" \
    >> target/runtime-fuzz/fandango-vectors.jsonl
done

python3 fuzz/fandango/run_abi_vectors.py \
  --solc "$solc_path" \
  --solar "$workspace/target/debug/solar" \
  --max-vectors 160 \
  --max-transactions 64 \
  --max-calldata-bytes 4096 \
  --timeout 20 \
  < target/runtime-fuzz/fandango-vectors.jsonl \
  | tee target/runtime-fuzz/fandango.json

mkdir -p target/runtime-fuzz/sources
fandango fuzz \
  -f fuzz/fandango/solidity-source.fan \
  --random-seed 1 \
  -n 32 \
  --directory target/runtime-fuzz/sources \
  --filename-extension .sol \
  --progress-bar off

python3 fuzz/fandango/run_solidity_sources.py \
  --source-dir target/runtime-fuzz/sources \
  --solc "$solc_path" \
  --solar "$workspace/target/debug/solar" \
  --failure-dir target/runtime-fuzz/source-failures \
  --max-sources 64 \
  --timeout 20 \
  --verbose \
  | tee target/runtime-fuzz/fandango-sources.json
