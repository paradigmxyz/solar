#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
audit="${PROOF_AUDIT:-false}"
if [[ "$audit" != true && "$audit" != false ]]; then
  echo "PROOF_AUDIT must be true or false" >&2
  exit 1
fi
unset SOLAR_PROOF_CACHE
uv run scripts/evm-rules/verify.py --help >/dev/null

prove() {
  local suite="$1" shard="$2" shards="$3" directory="$4"
  local files=("mir/$suite")
  local options=(--index-partition-timeout-ms 30000 --fallback-solver cvc5
    --bit-partition-timeout-ms 120000 --bit-partition-jobs 2)
  if [[ "$audit" == true ]]; then
    options+=(--partition-shifts)
  else
    options+=(--cache-dir "${PROOF_CACHE_DIR:-target/evm-proof-cache}")
  fi
  if [[ "$suite" == other ]]; then
    files=(mir/word_sequence mir-to-evm/stack_select evm-ir/stack_peephole evm-ir/late_word)
  fi
  local inputs=()
  for file in "${files[@]}"; do
    inputs+=("crates/codegen/isle/$file.isle")
  done
  uv run scripts/evm-rules/verify.py verify "${inputs[@]}" \
    --shard-index "$shard" --shard-count "$shards" "${options[@]}" \
    --output "$directory/proofs.json" --artifacts "$directory/smt"
  if [[ "$audit" == true ]]; then
    uv run scripts/evm-rules/replay.py "$directory/proofs.json" \
      --jobs 2 --output "$directory/cvc5.json"
  fi
}

pids=()
names=()
for suite in word egraph other; do
  case "$suite" in
    word) shards=4 ;;
    egraph) shards=8 ;;
    other) shards=1 ;;
  esac
  for ((shard=0; shard<shards; shard++)); do
    name="$suite-$shard"
    mkdir -p "$output/$name"
    printf 'Starting %s\n' "$name"
    prove "$suite" "$shard" "$shards" "$output/$name" >"$output/$name/run.log" 2>&1 &
    pids+=("$!")
    names+=("$name")
  done
done

status=0
for i in "${!pids[@]}"; do
  if wait "${pids[$i]}"; then
    printf 'PASS %s\n' "${names[$i]}"
  else
    printf 'FAIL %s\n' "${names[$i]}"
    status=1
  fi
  cat "$output/${names[$i]}/run.log"
done
exit "$status"
