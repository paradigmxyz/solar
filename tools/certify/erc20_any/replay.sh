#!/usr/bin/env bash
# Kernel replay of the whole closure of erc20_any.Final (toolchain and upstream packages included),
# accounted module by module with tools/certify/replay.py, as scripts/certify does it.
#
#   tools/certify/erc20_any/replay.sh LEAN_DIR OUT [PARALLEL]
#
# Needs run.sh's OUT/check_final.log. Batches of 400 modules, PARALLEL (default 6) leanchecker
# processes at once; each batch's exit is in OUT/replay/batch-exits; replay.py account fails unless
# every batch exited 0, every planned module was replayed and no olean changed since the plan.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd); certify=$(cd "$here/.." && pwd)
lean=$(realpath "$1"); out=$(realpath "$2"); par=${3:-6}; name=erc20_any
cd "$lean"
tc=$(lake env bash -c 'dirname $(dirname $(command -v lean))')
mkdir -p "$out/replay"; rd=$out/replay
python3 "$certify/replay.py" plan "$out/check_final.log" "$rd/plan" --syslib "$tc/lib/lean" \
  --require "$name.Final" --require "$name.Statement" --require "$name.Proof" --include-trusted || exit 1
mapfile -t mods < <(awk '$1 == "replay" { print $2 }' "$rd/plan")
echo "closure modules to replay: ${#mods[@]}" | tee "$rd/summary"
find .lake -name '*.olean' -print0 | sort -z | xargs -0 sha256sum | sha256sum > "$rd/oleans.before"
: > "$rd/batch-exits"
run_batch() { local i=$1; shift
  /usr/bin/time -v -o "$rd/b$i.time" lake env "$tc/bin/leanchecker" --verbose "$@" > "$rd/b$i.out" 2>&1
  echo "$i $? $(awk '/Maximum resident/{print $NF}' "$rd/b$i.time")" >> "$rd/batch-exits"; }
idx=()
for ((i = 0; i < ${#mods[@]}; i += 400)); do
  while (( $(jobs -rp | wc -l) >= par )); do wait -n; done
  run_batch $i "${mods[@]:i:400}" &
  idx+=($i)
done
wait
status=0; : > "$rd/all.out"
for i in "${idx[@]}"; do cat "$rd/b$i.out" >> "$rd/all.out"; done
sort -n "$rd/batch-exits" -o "$rd/batch-exits"
awk '$2 != 0 { bad = 1 } END { exit bad }' "$rd/batch-exits" || status=1
find .lake -name '*.olean' -print0 | sort -z | xargs -0 sha256sum | sha256sum > "$rd/oleans.after"
cmp -s "$rd/oleans.before" "$rd/oleans.after" || { echo "oleans changed during replay"; status=1; }
python3 "$certify/replay.py" account "$rd/plan" "$rd/all.out" "$status" "$rd/receipt.json" \
  --checker "$tc/bin/leanchecker" --toolchain "$tc" | tee -a "$rd/summary"
rc=${PIPESTATUS[0]}
echo "replay account exit=$rc status=$status" | tee -a "$rd/summary"
exit $rc
