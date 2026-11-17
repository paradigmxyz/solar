#!/usr/bin/env bash
# Admission and receipt checks for a certify run in a memory-limited cgroup scope. Each
# subcommand fails closed: anything it cannot read or that differs from what is expected exits 1
# with a message naming it, and only a check that read everything prints "ok".
#
#   box_admit.sh admit CGROUP INNER_PID MEMORY_MAX
#       The scope's directory exists, its memory.max reads back as MEMORY_MAX bytes (never "max"),
#       its memory.swap.max reads back as 0, and INNER_PID is in the scope: listed in its
#       cgroup.procs and with the scope as its own cgroup.
#   box_admit.sh pidtree CGROUP INNER_PID
#       One sample: INNER_PID and every descendant of it are in the scope's cgroup. A process
#       that exits during the sample is skipped; one that is still there with another cgroup, or
#       with a cgroup that cannot be read, is an escape.
#   box_admit.sh receipts RUN_DIR
#       RUN_DIR has every required receipt, receipts.sha256 lists each of them, every file it
#       lists matches its digest, and the every-caller negative tests report no failure.
#
# CGROUP is the scope's cgroup as /proc/PID/cgroup names it (such as /system.slice/x.scope).
# BOX_ADMIT_CGROUP_ROOT (default /sys/fs/cgroup) and BOX_ADMIT_PROC_ROOT (default /proc) point
# the checks at fixture trees in tests.
set -euo pipefail

cgroot="${BOX_ADMIT_CGROUP_ROOT:-/sys/fs/cgroup}"
proc="${BOX_ADMIT_PROC_ROOT:-/proc}"

# The receipts a finished run must have, relative to its directory.
required_receipts=(record/record.json bind.json replay-receipt.json axioms.out negative_any.out)

die() {
    echo "box_admit: ABORT: $*" >&2
    exit 1
}

# The cgroup v2 path of a pid (the "0::" line of its cgroup file); fails when unreadable.
pid_cgroup() {
    local line
    [[ -r "$proc/$1/cgroup" ]] || return 1
    while IFS= read -r line; do
        if [[ "$line" == 0::* ]]; then
            printf '%s\n' "${line#0::}"
            return 0
        fi
    done < "$proc/$1/cgroup"
    return 1
}

scope_dir() {
    local cg="$1"
    [[ "$cg" == /* ]] || die "cgroup '$cg' is not an absolute cgroup path"
    [[ -d "$cgroot$cg" ]] || die "scope cgroup $cgroot$cg does not exist"
}

admit() {
    [[ $# -eq 3 ]] || die "usage: admit CGROUP INNER_PID MEMORY_MAX"
    local cg="$1" pid="$2" want="$3" dir mm sm icg found=0 p
    [[ "$want" =~ ^[0-9]+$ ]] || die "expected memory.max '$want' is not a byte count"
    [[ "$pid" =~ ^[0-9]+$ ]] || die "inner pid '$pid' is not a pid"
    scope_dir "$cg"
    dir="$cgroot$cg"
    mm="$(cat "$dir/memory.max" 2> /dev/null)" || die "cannot read $dir/memory.max"
    [[ "$mm" != max ]] || die "memory.max is 'max' (unlimited), expected $want"
    [[ "$mm" == "$want" ]] || die "memory.max is '$mm', expected $want"
    sm="$(cat "$dir/memory.swap.max" 2> /dev/null)" || die "cannot read $dir/memory.swap.max"
    [[ "$sm" == 0 ]] || die "memory.swap.max is '$sm', expected 0"
    [[ -r "$dir/cgroup.procs" ]] || die "cannot read $dir/cgroup.procs"
    while IFS= read -r p; do
        [[ "$p" == "$pid" ]] && found=1
    done < "$dir/cgroup.procs"
    [[ $found == 1 ]] || die "inner pid $pid is not in $dir/cgroup.procs"
    icg="$(pid_cgroup "$pid")" || die "cannot read the cgroup of inner pid $pid"
    [[ "$icg" == "$cg" ]] || die "inner pid $pid is in cgroup $icg, not the scope $cg"
    echo "ok admit: memory.max $mm, memory.swap.max $sm, inner pid $pid in $cg"
}

pidtree() {
    [[ $# -eq 2 ]] || die "usage: pidtree CGROUP INNER_PID"
    local cg="$1" root="$2" d p rest ppid c n=0 esc="" changed=1
    local -A parent=() in_tree=()
    [[ "$root" =~ ^[0-9]+$ ]] || die "inner pid '$root' is not a pid"
    scope_dir "$cg"
    [[ -r "$proc/$root/stat" ]] || die "inner pid $root is not running"
    # The parent of each process: field 4 of its stat, after the command's closing paren.
    for d in "$proc"/[0-9]*; do
        p="${d##*/}"
        rest="$(cat "$d/stat" 2> /dev/null)" || continue
        rest="${rest##*) }"
        read -r _ ppid _ <<< "$rest"
        parent[$p]="$ppid"
    done
    # The descendants of the inner pid, to a fixed point.
    in_tree[$root]=1
    while [[ $changed == 1 ]]; do
        changed=0
        for p in "${!parent[@]}"; do
            if [[ -z "${in_tree[$p]:-}" && -n "${in_tree[${parent[$p]}]:-}" ]]; then
                in_tree[$p]=1
                changed=1
            fi
        done
    done
    for p in "${!in_tree[@]}"; do
        n=$((n + 1))
        if c="$(pid_cgroup "$p")"; then
            [[ "$c" == "$cg" ]] || esc="$esc $p:$c"
        elif [[ -e "$proc/$p" ]]; then
            esc="$esc $p:unreadable"
        fi
    done
    [[ -z "$esc" ]] || die "descendants of inner pid $root escaped the scope $cg:$esc"
    echo "ok pidtree: $n processes under inner pid $root, all in $cg"
}

receipts() {
    [[ $# -eq 1 ]] || die "usage: receipts RUN_DIR"
    local run="$1" r line sum path summary
    local -A listed=()
    [[ -d "$run" ]] || die "run directory $run does not exist"
    for r in "${required_receipts[@]}" receipts.sha256; do
        [[ -f "$run/$r" ]] || die "required receipt $r is missing from $run"
    done
    while IFS= read -r line; do
        [[ "$line" =~ ^([0-9a-f]{64})\ [\ *](.+)$ ]] || die "malformed receipts.sha256 line: $line"
        sum="${BASH_REMATCH[1]}"
        path="${BASH_REMATCH[2]}"
        [[ "$path" != /* && "/$path/" != */../* ]] || die "receipts.sha256 names $path outside the run"
        [[ -f "$run/$path" ]] || die "receipt $path listed in receipts.sha256 is missing"
        r="$(sha256sum < "$run/$path")" || die "cannot digest receipt $path"
        r="${r%% *}"
        [[ "$r" == "$sum" ]] || die "receipt $path has sha256 $r, receipts.sha256 says $sum"
        listed[$path]=1
    done < "$run/receipts.sha256"
    for r in "${required_receipts[@]}"; do
        [[ -n "${listed[$r]:-}" ]] || die "required receipt $r is not listed in receipts.sha256"
    done
    summary="$(grep -E 'every-caller negative tests: [0-9]+ cases, [0-9]+ failed' "$run/negative_any.out" | tail -n 1 || true)"
    [[ -n "$summary" ]] || die "negative_any.out has no summary line"
    [[ "$summary" =~ \ [1-9][0-9]*\ cases,\ 0\ failed ]] || die "negative_any.out reports failures: $summary"
    echo "ok receipts: ${#listed[@]} receipts in $run match receipts.sha256"
}

[[ $# -ge 1 ]] || die "usage: box_admit.sh admit|pidtree|receipts ARGS..."
cmd="$1"
shift
case "$cmd" in
    admit | pidtree | receipts) "$cmd" "$@" ;;
    *) die "unknown subcommand '$cmd'" ;;
esac
