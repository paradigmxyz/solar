#!/usr/bin/env bash
# Run one build step under an enforced whole-tree memory cap and record whether it held.
#
#   tools/certify/erc20_any/build_cap.sh JOBS BUDGET_KB GATE NAME -- CMD...
#
# Admission is fail-closed: before CMD starts, the script creates its own cgroup (v2) under
# CG_ROOT (default /sys/fs/cgroup), checks that the memory controller is enabled there, writes
# memory.max (BUDGET_KB rounded down to a whole page, since the kernel stores the limit in pages)
# and memory.swap.max 0, checks every write, and reads both files back. A missing or unwritable
# cgroup, a failed write, or a readback that differs from the requested value refuses admission:
# the script appends an "admission ... REFUSED" line to GATE, removes the cgroup and exits 95
# without running CMD. Joining the cgroup is checked the same way (exit 96).
#
# Concurrency is not enforced: LEAN_NUM_THREADS=JOBS bounds lake's task pool, and a sampler counts
# the cgroup's lean and rustc processes every second, so max_lean and max_rustc are sampled-only
# and the gate line says so. At each new peak of the tree's summed RSS the sampler appends the
# time, the sum, and the pid, RSS and argv of the largest process to GATE's directory as
# NAME.peaks. After CMD exits, memory.max and memory.swap.max are read back again. GATE gets one
# line: the requested and read-back limits, the cgroup's peak memory (memory.peak), the sampled
# lean and rustc maxima, OOM kills, and PASS or FAILED. FAILED (a sampled count above JOBS, any
# OOM kill, or a limit that changed during the step) exits 97 and voids the slot; otherwise the
# step's own exit is returned.
#
# CG_ROOT and SUDO (default "sudo -n") exist for the negative controls in neg_cap.sh.
set -uo pipefail
jobs=$1 budget=$2 gate=$3 name=$4; shift 5
root=${CG_ROOT:-/sys/fs/cgroup}
read -ra su <<< "${SUDO-sudo -n}"
cg=$root/erc20cap-$name-$$
page=$(getconf PAGESIZE)
refuse() {
  echo "admission $name cg=$cg want_max=${want_max:-?} got_max=${got_max:-?} want_swap=0 got_swap=${got_swap:-?} REFUSED: $1" | tee -a "$gate" >&2
  [[ -d $cg ]] && "${su[@]}" rmdir "$cg" 2>/dev/null
  exit 95
}
[[ $jobs =~ ^[1-9][0-9]*$ && $budget =~ ^[1-9][0-9]*$ ]] || refuse "bad JOBS or BUDGET_KB"
want_max=$(( budget * 1024 / page * page ))
"${su[@]}" mkdir "$cg" 2>/dev/null || refuse "cannot create cgroup"
grep -qw memory "$cg/cgroup.controllers" 2>/dev/null || refuse "memory controller not enabled"
echo "$want_max" | "${su[@]}" tee "$cg/memory.max" > /dev/null 2>&1 || refuse "memory.max write failed"
echo 0 | "${su[@]}" tee "$cg/memory.swap.max" > /dev/null 2>&1 || refuse "memory.swap.max write failed"
got_max=$(cat "$cg/memory.max" 2>/dev/null) || refuse "memory.max unreadable"
got_swap=$(cat "$cg/memory.swap.max" 2>/dev/null) || refuse "memory.swap.max unreadable"
[[ $got_max == "$want_max" ]] || refuse "memory.max readback mismatch"
[[ $got_swap == 0 ]] || refuse "memory.swap.max readback mismatch"
echo "admission $name cg=$cg want_max=$want_max got_max=$got_max want_swap=0 got_swap=$got_swap concurrency=sampled-only ADMITTED" | tee -a "$gate"
( me=$BASHPID; echo "$me" | "${su[@]}" tee "$cg/cgroup.procs" > /dev/null || exit 96
  grep -qx "$me" "$cg/cgroup.procs" || exit 96
  exec env LEAN_NUM_THREADS="$jobs" "$@" ) &
pid=$!
maxl=0 maxr=0 maxsum=0 peaks="$(dirname "$gate")/$name.peaks"
while kill -0 $pid 2>/dev/null; do
  l=0 r=0 sum=0 big=0 bigp=0
  while read -r p; do
    case $(cat /proc/$p/comm 2>/dev/null) in lean) l=$((l + 1)) ;; rustc) r=$((r + 1)) ;; esac
    k=$(awk '/^VmRSS:/ { print $2 }' /proc/$p/status 2>/dev/null); k=${k:-0}
    sum=$((sum + k)); (( k > big )) && { big=$k; bigp=$p; }
  done < "$cg/cgroup.procs"
  (( l > maxl )) && maxl=$l; (( r > maxr )) && maxr=$r
  if (( sum > maxsum )); then
    maxsum=$sum
    echo "$(date -u +%FT%TZ) tree_rss_kb=$sum lean=$l largest_pid=$bigp largest_rss_kb=$big argv=$(tr '\0' ' ' < /proc/$bigp/cmdline 2>/dev/null | cut -c1-400)" >> "$peaks"
  fi
  sleep 1
done
wait $pid; rc=$?
post_max=$(cat "$cg/memory.max" 2>/dev/null || echo unreadable)
post_swap=$(cat "$cg/memory.swap.max" 2>/dev/null || echo unreadable)
peak=$(( $(cat "$cg/memory.peak" 2>/dev/null || echo 0) / 1024 ))
oom=$(awk '$1 == "oom_kill" { print $2 }' "$cg/memory.events" 2>/dev/null); oom=${oom:-unreadable}
verdict=PASS
[[ $oom == 0 && $post_max == "$want_max" && $post_swap == 0 ]] || verdict=FAILED
(( maxl > jobs || maxr > jobs )) && verdict=FAILED
echo "build $name exit=$rc cap_jobs=$jobs budget_kb=$budget max_bytes=$want_max post_max=$post_max post_swap=$post_swap peak_tree_kb=$peak max_sampled_tree_rss_kb=$maxsum max_lean=$maxl max_rustc=$maxr concurrency=sampled-only oom_kill=$oom $verdict" | tee -a "$gate"
"${su[@]}" rmdir "$cg" 2>/dev/null
[[ $verdict == PASS ]] || exit 97
exit $rc
