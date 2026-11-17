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
# the cap or the budget was ever exceeded.
#
# Sequential slots: PRIOR_REPLAY (an earlier slot's OUT/replay) makes this slot compare its plan
# (module -> olean sha256) with the prior one before replaying anything, exit 3 if they differ, and
# skip the prior slot's finished batches. REPLAY_STOP_AT and REPLAY_KILL_AT (Unix times) stop
# admitting batches and kill the running ones. A slot that leaves batches unreplayed exits 2 after
# writing verdicts (each module: verified in this slot, verified earlier, or unverified); merge.py
# then combines the slots. A whole replay in one slot is accounted by replay.py and exits 0.
#
# Parallel shards (shard.sh): REPLAY_EXPECT_PLAN is the sha256 the plan must have (exit 3 before
# anything runs if it differs, which voids the slot that made that plan). REPLAY_BATCHES is a
# shard's batches file from shard_plan.py, used instead of the 400-module batches: every module in
# it must be planned, and none may appear twice. SCHED_WALLS (the shard's walls file) lets sched.py
# admit a batch only if its projected wall ends before REPLAY_KILL_AT. A shard never accounts the
# closure itself: when its batches all ran it exits 2 too, and merge.py combines it with the others.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd); certify=$(cd "$here/.." && pwd)
lean=$(realpath "$1"); out=$(realpath "$2"); par=${3:-4}; head=${4:-25}
peaks=$(realpath "${5:-$here/replay-peaks.59d0fa44}"); name=erc20_any
cd "$lean"
tc=$(lake env bash -c 'dirname $(dirname $(command -v lean))')
mkdir -p "$out/replay"; rd=$out/replay
python3 "$certify/replay.py" plan "$out/check_final.log" "$rd/plan" --syslib "$tc/lib/lean" \
  --require "$name.Final" --require "$name.Statement" --require "$name.Proof" --include-trusted || exit 1
awk '$1 == "replay" { print $2, $3 }' "$rd/plan" | sort > "$rd/inventory"
sha256sum "$rd/plan" "$rd/inventory" | tee "$rd/inventory.sha256"
if [[ -n ${REPLAY_EXPECT_PLAN:-} ]]; then
  got=$(sha256sum < "$rd/plan" | cut -c1-64)
  if [[ $got != "$REPLAY_EXPECT_PLAN" ]]; then
    echo "plan $got is not the expected $REPLAY_EXPECT_PLAN: the earlier slot's replay is void" | tee -a "$rd/summary"
    exit 3
  fi
  echo "plan $got as expected" | tee -a "$rd/summary"
fi
mapfile -t mods < <(awk '$1 == "replay" { print $2 }' "$rd/plan")
echo "closure modules to replay: ${#mods[@]}" | tee "$rd/summary"
: > "$rd/batches"
if [[ -n ${REPLAY_BATCHES:-} ]]; then
  [[ -z ${PRIOR_REPLAY:-} ]] || { echo "REPLAY_BATCHES and PRIOR_REPLAY exclude each other" | tee -a "$rd/summary"; exit 1; }
  awk '$1 == "replay" { print $2 }' "$rd/plan" | sort > "$rd/planned"
  awk '{ for (i = 2; i <= NF; i++) print $i }' "$REPLAY_BATCHES" | sort > "$rd/shard-modules"
  if [[ -n $(uniq -d "$rd/shard-modules") || -n $(comm -13 "$rd/planned" "$rd/shard-modules") ]] \
    || [[ -n $(awk '$1 !~ /^[0-9]+$/ || NF < 2' "$REPLAY_BATCHES") || -n $(awk '{ print $1 }' "$REPLAY_BATCHES" | sort | uniq -d) ]]; then
    echo "shard batches $REPLAY_BATCHES name an unplanned or repeated module or a bad index" | tee -a "$rd/summary"
    exit 1
  fi
  cp "$REPLAY_BATCHES" "$rd/batches"
  echo "shard modules: $(wc -l < "$rd/shard-modules") in $(wc -l < "$rd/batches") batches, batches sha256 $(sha256sum < "$rd/batches" | cut -c1-64)" | tee -a "$rd/summary"
else
  for ((i = 0; i < ${#mods[@]}; i += 400)); do echo "$i ${mods[*]:i:400}" >> "$rd/batches"; done
fi
skip=()
if [[ -n ${PRIOR_REPLAY:-} ]]; then
  if ! cmp -s "$PRIOR_REPLAY/plan" "$rd/plan" || ! cmp -s "$PRIOR_REPLAY/batches" "$rd/batches"; then
    echo "inventory mismatch with $PRIOR_REPLAY: the earlier slot's replay is void" | tee -a "$rd/summary"
    diff <(cat "$PRIOR_REPLAY/inventory") "$rd/inventory" | head -50 > "$rd/inventory.diff"
    exit 3
  fi
  echo "inventory identical to $PRIOR_REPLAY" | tee -a "$rd/summary"
  skip=(SCHED_SKIP="$PRIOR_REPLAY/batch-exits")
fi
find .lake -name '*.olean' -print0 | sort -z | xargs -0 sha256sum | sha256sum > "$rd/oleans.before"
total=$(awk '/MemTotal/ { print $2 }' /proc/meminfo); budget=$(( total * (100 - head) / 100 ))
env "${skip[@]}" SCHED_WALLS="${SCHED_WALLS:-}" SCHED_STOP_AT="${REPLAY_STOP_AT:-inf}" SCHED_KILL_AT="${REPLAY_KILL_AT:-inf}" \
  python3 "$here/sched.py" "$rd" "$par" "$budget" "$peaks" -- lake env "$tc/bin/leanchecker" --verbose \
  | tee -a "$rd/summary"
sched=${PIPESTATUS[0]}
find .lake -name '*.olean' -print0 | sort -z | xargs -0 sha256sum | sha256sum > "$rd/oleans.after"
cmp -s "$rd/oleans.before" "$rd/oleans.after" || { echo "oleans changed during replay" | tee -a "$rd/summary"; exit 1; }
# Verdicts: a module is verified only by a batch that ran here and exited 0.
python3 - "$rd" > "$rd/verdicts" <<'PY'
import sys
rd = sys.argv[1]
st = {f[0]: f for f in (l.split() for l in open(f"{rd}/batch-exits")) if f}
for l in open(f"{rd}/batches"):
    i, *mods = l.split()
    f = st.get(i, [i, "-1", "0", "not-started"])
    v = {("0", "ran"): "verified", ("0", "skipped-done"): "verified-earlier-slot"}.get((f[1], f[3]), "unverified")
    for m in mods:
        print(m, i, v, f[3], f[1])
PY
awk '{ print $3 }' "$rd/verdicts" | sort | uniq -c | tee -a "$rd/summary"
if [[ $sched == 2 ]]; then echo "partial slot: merge with merge.py" | tee -a "$rd/summary"; exit 2; fi
[[ $sched == 0 ]] || exit 1
if [[ -n ${REPLAY_BATCHES:-} ]]; then echo "shard: every batch ran; merge with merge.py" | tee -a "$rd/summary"; exit 2; fi
: > "$rd/all.out"
while read -r i _; do cat "$rd/b$i.out" >> "$rd/all.out"; done < "$rd/batches"
python3 "$certify/replay.py" account "$rd/plan" "$rd/all.out" 0 "$rd/receipt.json" \
  --checker "$tc/bin/leanchecker" --toolchain "$tc" | tee -a "$rd/summary"
rc=${PIPESTATUS[0]}
echo "replay account exit=$rc" | tee -a "$rd/summary"
exit $rc
