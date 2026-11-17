#!/usr/bin/env bash
# Admits a run of JOBS Lean processes at once, each capped at MB megabytes, only when JOBS * MB
# fits the memory limit of the cgroup the driver runs in: the cgroup v2 memory.max of its own
# cgroup ("0::PATH" in /proc/self/cgroup, read under CGROUP_ROOT/PATH, CGROUP_ROOT being
# CERTIFY_CGROUP_ROOT or /sys/fs/cgroup). It fails closed: above one job, a memory.max that cannot
# be read or is "max" (no limit) refuses, since nothing then bounds the run's peak. One job needs
# no limit, as before.
#
#   tools/certify/admit.sh JOBS MB
set -euo pipefail
jobs=$1 mb=$2
refuse() { echo "certify: error: CERTIFY_LAKE_JOBS=$jobs: $*" >&2; exit 1; }
[[ "$jobs" =~ ^[1-9][0-9]*$ ]] || { echo "certify: error: CERTIFY_LAKE_JOBS must be a positive integer" >&2; exit 1; }
[[ "$mb" =~ ^[1-9][0-9]*$ ]] || { echo "certify: error: LEAN_MEMORY must be a positive integer" >&2; exit 1; }
[[ "$jobs" != 1 ]] || exit 0
cgroot="${CERTIFY_CGROUP_ROOT:-/sys/fs/cgroup}"
path="$(sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null | head -1)" || path=
[[ -n "$path" ]] || refuse "no cgroup v2 entry in /proc/self/cgroup, so no memory limit to admit against"
file="$cgroot${path%/}/memory.max"
limit="$(cat "$file" 2>/dev/null)" || refuse "cannot read $file, so no memory limit to admit against"
[[ "$limit" != max ]] || refuse "$file is max (no limit), so nothing bounds $jobs Lean processes of $mb MB"
[[ "$limit" =~ ^[0-9]+$ ]] || refuse "$file holds no byte count: $limit"
need=$((jobs * mb * 1024 * 1024))
[[ $need -le $limit ]] ||
    refuse "$jobs Lean processes of $mb MB (LEAN_MEMORY) need $((need / 1048576)) MB, more than the $((limit / 1048576)) MB of $file"
