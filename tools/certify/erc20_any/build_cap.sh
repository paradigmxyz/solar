#!/usr/bin/env bash
# Run one build step under an enforced whole-tree cap and record whether it held.
#
#   tools/certify/erc20_any/build_cap.sh JOBS BUDGET_KB GATE NAME -- CMD...
#
# CMD runs in its own cgroup (v2) whose memory.max is BUDGET_KB and memory.swap.max 0, so the
# kernel refuses the whole process tree more memory than the budget (it is OOM-killed instead of
# exceeding it). LEAN_NUM_THREADS=JOBS bounds lake's task pool, and so the lean processes it runs
# at once; each lean's heap is bounded by the -KleanMemory the step passes. A sampler counts the
# cgroup's lean and rustc processes every second. GATE gets one line: the cap, the cgroup's peak
# memory (memory.peak), the most lean and rustc processes seen at once, OOM kills, and PASS or
# FAILED. FAILED (more than JOBS lean or rustc processes at once, or any OOM kill) exits 97 and
# voids the slot; otherwise the step's own exit is returned.
set -uo pipefail
jobs=$1 budget=$2 gate=$3 name=$4; shift 5
cg=/sys/fs/cgroup/erc20cap-$name-$$
sudo -n mkdir "$cg" || exit 1
echo $(( budget * 1024 )) | sudo -n tee "$cg/memory.max" > /dev/null
echo 0 | sudo -n tee "$cg/memory.swap.max" > /dev/null
( me=$BASHPID; echo "$me" | sudo -n tee "$cg/cgroup.procs" > /dev/null || exit 96
  grep -qx "$me" "$cg/cgroup.procs" || exit 96
  exec env LEAN_NUM_THREADS="$jobs" "$@" ) &
pid=$!
maxl=0 maxr=0
while kill -0 $pid 2>/dev/null; do
  l=0 r=0
  while read -r p; do
    case $(cat /proc/$p/comm 2>/dev/null) in lean) l=$((l + 1)) ;; rustc) r=$((r + 1)) ;; esac
  done < "$cg/cgroup.procs"
  (( l > maxl )) && maxl=$l; (( r > maxr )) && maxr=$r
  sleep 1
done
wait $pid; rc=$?
peak=$(( $(cat "$cg/memory.peak" 2>/dev/null || echo 0) / 1024 ))
oom=$(awk '$1 == "oom_kill" { print $2 }' "$cg/memory.events")
verdict=PASS; (( maxl > jobs || maxr > jobs || oom > 0 )) && verdict=FAILED
echo "build $name exit=$rc cap_jobs=$jobs budget_kb=$budget peak_tree_kb=$peak max_lean=$maxl max_rustc=$maxr oom_kill=$oom $verdict" | tee -a "$gate"
sudo -n rmdir "$cg" 2>/dev/null
[[ $verdict == PASS ]] || exit 97
exit $rc
