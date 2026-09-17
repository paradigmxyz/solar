#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

output="${1:-target/evm-rules}"
uv run scripts/evm-rules/verify.py --help >/dev/null

prove() {
  local suite="$1" shard="$2" shards="$3" directory="$4"
  local files=("$suite")
  local options=(--partition-shifts --index-partition-timeout-ms 30000)
  if [[ "$suite" == other ]]; then
    files=(word_sequence stack_select stack_peephole late_word)
  elif [[ "$suite" == egraph ]]; then
    options+=(--fallback-solver cvc5
      --bit-partition-timeout-ms 120000 --bit-partition-jobs 2)
  fi
  local inputs=()
  for file in "${files[@]}"; do
    inputs+=("crates/codegen/isle/$file.isle")
  done
  uv run scripts/evm-rules/verify.py verify "${inputs[@]}" \
    --shard-index "$shard" --shard-count "$shards" "${options[@]}" \
    --output "$directory/proofs.json" --artifacts "$directory/smt"
  uv run scripts/evm-rules/replay.py "$directory/proofs.json" \
    --jobs 2 --output "$directory/cvc5.json"
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
