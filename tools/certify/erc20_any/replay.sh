#!/usr/bin/env bash
# Kernel replay of the whole closure of erc20_any.Final (toolchain and upstream packages included),
# accounted module by module with tools/certify/replay.py, as scripts/certify does it.
#
#   tools/certify/erc20_any/replay.sh LEAN_DIR OUT [PARALLEL] [HEADROOM_PERCENT] [PEAKS_FILE]
#
# Needs run.sh's OUT/check_final.log. Batches of 400 modules run under sched.py: at most PARALLEL
# (default 4) leanchecker processes in the whole tree, started only while the machine's used memory
# and the running batches' estimated peaks (PEAKS_FILE, default replay-peaks.59d0fa44 beside this
# script, times 1.25) leave HEADROOM_PERCENT (default 25) of memory free. sched.py samples the
# process tree every 2 s, kills every batch past half the remaining headroom, and fails the gate if
# the cap or the budget was ever exceeded. Exits 0 only if every batch exited 0, the gate held,
# every planned module was replayed and no olean changed since the plan.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd); certify=$(cd "$here/.." && pwd)
lean=$(realpath "$1"); out=$(realpath "$2"); par=${3:-4}; head=${4:-25}
peaks=$(realpath "${5:-$here/replay-peaks.59d0fa44}"); name=erc20_any
cd "$lean"
tc=$(lake env bash -c 'dirname $(dirname $(command -v lean))')
mkdir -p "$out/replay"; rd=$out/replay
python3 "$certify/replay.py" plan "$out/check_final.log" "$rd/plan" --syslib "$tc/lib/lean" \
  --require "$name.Final" --require "$name.Statement" --require "$name.Proof" --include-trusted || exit 1
mapfile -t mods < <(awk '$1 == "replay" { print $2 }' "$rd/plan")
echo "closure modules to replay: ${#mods[@]}" | tee "$rd/summary"
find .lake -name '*.olean' -print0 | sort -z | xargs -0 sha256sum | sha256sum > "$rd/oleans.before"
: > "$rd/batches"
for ((i = 0; i < ${#mods[@]}; i += 400)); do echo "$i ${mods[*]:i:400}" >> "$rd/batches"; done
total=$(awk '/MemTotal/ { print $2 }' /proc/meminfo); budget=$(( total * (100 - head) / 100 ))
python3 "$here/sched.py" "$rd" "$par" "$budget" "$peaks" -- lake env "$tc/bin/leanchecker" --verbose \
  | tee -a "$rd/summary"
status=${PIPESTATUS[0]}; : > "$rd/all.out"
while read -r i _; do cat "$rd/b$i.out" >> "$rd/all.out" 2>/dev/null; done < "$rd/batches"
find .lake -name '*.olean' -print0 | sort -z | xargs -0 sha256sum | sha256sum > "$rd/oleans.after"
cmp -s "$rd/oleans.before" "$rd/oleans.after" || { echo "oleans changed during replay"; status=1; }
python3 "$certify/replay.py" account "$rd/plan" "$rd/all.out" "$status" "$rd/receipt.json" \
  --checker "$tc/bin/leanchecker" --toolchain "$tc" | tee -a "$rd/summary"
rc=${PIPESTATUS[0]}
echo "replay account exit=$rc status=$status" | tee -a "$rd/summary"
exit $rc
