#!/usr/bin/env bash
set -euo pipefail

solc_version="${SOLC_VERSION:?SOLC_VERSION must be set}"
fandango_version="${FANDANGO_VERSION:?FANDANGO_VERSION must be set}"
workspace="${GITHUB_WORKSPACE:-$(pwd)}"
runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
solc_path="$runner_temp/solc/solc-$solc_version"
solar_path="$workspace/target/debug/solar"
abi_seed_count="${FANDANGO_ABI_SEED_COUNT:-3}"
abi_values_per_seed="${FANDANGO_ABI_VALUES_PER_SEED:-32}"
max_abi_vectors="${FANDANGO_MAX_ABI_VECTORS:-160}"
source_count="${FANDANGO_SOURCE_COUNT:-32}"
max_source_count="${FANDANGO_MAX_SOURCE_COUNT:-$source_count}"
runtime_source_count="${FANDANGO_RUNTIME_SOURCE_COUNT:-16}"
max_runtime_sources="${FANDANGO_MAX_RUNTIME_SOURCES:-$runtime_source_count}"
solsmith_count="${FANDANGO_SOLSMITH_COUNT:-16}"
max_solsmith_sources="${FANDANGO_MAX_SOLSMITH_SOURCES:-$solsmith_count}"
runtime_cases="${FANDANGO_RUNTIME_CASES:-8}"
foundry_targets="${FANDANGO_FOUNDRY_TARGETS:-2}"
foundry_fuzz_runs="${FANDANGO_FOUNDRY_FUZZ_RUNS:-64}"

cd "$workspace"
mkdir -p target/runtime-fuzz "$runner_temp/solc"

curl -fsSL \
  --retry 3 \
  --retry-delay 2 \
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

generate_sources() {
  local grammar="$1"
  local count="$2"
  local out_dir="$3"
  shift 3
  mkdir -p "$out_dir"
  fandango fuzz \
    -f "$grammar" \
    --random-seed 1 \
    "$@" \
    -n "$count" \
    --directory "$out_dir" \
    --filename-extension .sol \
    --progress-bar off
}

run_compile_diff() {
  local source_dir="$1"
  local failure_dir="$2"
  local max_sources="$3"
  local out_json="$4"
  python3 fuzz/fandango/run_solidity_sources.py \
    --source-dir "$source_dir" \
    --solc "$solc_path" \
    --solar "$solar_path" \
    --failure-dir "$failure_dir" \
    --max-sources "$max_sources" \
    --timeout 20 \
    --verbose \
    | tee "$out_json"
}

run_runtime_diff() {
  local source_dir="$1"
  local failure_dir="$2"
  local max_sources="$3"
  local out_json="$4"
  python3 fuzz/fandango/run_source_runtime.py \
    --source-dir "$source_dir" \
    --solc "$solc_path" \
    --solar "$solar_path" \
    --failure-dir "$failure_dir" \
    --max-sources "$max_sources" \
    --cases-per-source "$runtime_cases" \
    --timeout 20 \
    --verbose \
    | tee "$out_json"
}

run_runtime_regressions() {
  shopt -s nullglob
  local regressions=(fuzz/fandango/runtime-regressions/*.json)
  shopt -u nullglob
  if (( ${#regressions[@]} == 0 )); then
    return
  fi

  mkdir -p target/runtime-fuzz/runtime-regressions
  for index in "${!regressions[@]}"; do
    python3 fuzz/fandango/run_source_runtime.py \
      --replay-failure "${regressions[$index]}" \
      --solc "$solc_path" \
      --solar "$solar_path" \
      --timeout 20 \
      --verbose \
      | tee "target/runtime-fuzz/runtime-regressions/replay-$index.json"
  done
}

python3 fuzz/fandango/encode_abi_vectors.py --seed corpus \
  < fuzz/fandango/corpus.jsonl \
  > target/runtime-fuzz/fandango-vectors.jsonl

for seed in $(seq 1 "$abi_seed_count"); do
  fandango fuzz \
    -f fuzz/fandango/abi-values.fan \
    --random-seed "$seed" \
    -n "$abi_values_per_seed" \
    --separator $'\n' \
    --progress-bar off \
    | python3 fuzz/fandango/encode_abi_vectors.py --seed "$seed" \
    >> target/runtime-fuzz/fandango-vectors.jsonl
done

python3 fuzz/fandango/run_abi_vectors.py \
  --solc "$solc_path" \
  --solar "$solar_path" \
  --max-vectors "$max_abi_vectors" \
  --max-transactions 64 \
  --max-calldata-bytes 4096 \
  --timeout 20 \
  < target/runtime-fuzz/fandango-vectors.jsonl \
  | tee target/runtime-fuzz/fandango.json

generate_sources fuzz/fandango/solidity-source.fan "$source_count" target/runtime-fuzz/sources
run_compile_diff \
  target/runtime-fuzz/sources \
  target/runtime-fuzz/source-failures \
  "$max_source_count" \
  target/runtime-fuzz/fandango-sources.json

generate_sources \
  fuzz/fandango/solidity-runtime-source.fan \
  "$runtime_source_count" \
  target/runtime-fuzz/runtime-sources \
  --initial-population fuzz/fandango/runtime-corpus \
  --population-size 24 \
  --mutation-rate 0.4 \
  --crossover-rate 0.4
run_runtime_diff \
  target/runtime-fuzz/runtime-sources \
  target/runtime-fuzz/runtime-failures \
  "$max_runtime_sources" \
  target/runtime-fuzz/fandango-runtime-sources.json

python3 fuzz/fandango/solsmith.py \
  --seed 1 \
  --count "$solsmith_count" \
  --require-default-features \
  --out-dir target/runtime-fuzz/solsmith-sources \
  --metadata target/runtime-fuzz/solsmith-metadata.json \
  | tee target/runtime-fuzz/solsmith-generation.json

run_runtime_diff \
  target/runtime-fuzz/solsmith-sources \
  target/runtime-fuzz/solsmith-failures \
  "$max_solsmith_sources" \
  target/runtime-fuzz/solsmith-runtime.json

run_runtime_regressions

if (( foundry_targets > 0 )); then
  mapfile -t foundry_sources < <(
    find target/runtime-fuzz/solsmith-sources -name '*.sol' -type f | sort | head -n "$foundry_targets"
  )
  for index in "${!foundry_sources[@]}"; do
    python3 fuzz/fandango/run_foundry_target.py \
      --source "${foundry_sources[$index]}" \
      --solc "$solc_path" \
      --solar "$solar_path" \
      --fuzz-runs "$foundry_fuzz_runs" \
      --timeout 60 \
      --verbose \
      | tee "target/runtime-fuzz/foundry-differential-$index.json"
  done
fi
