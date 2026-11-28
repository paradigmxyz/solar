#!/usr/bin/env bash
# Negative controls for the driver's stage gates: each case of testdata/fixplan/stages.tsv is a
# source the driver must REJECT (exit 1) with a last line matching its registered stage, and, with
# --kernel, at its registered declaration: the line names "refuted in the kernel by
# <module>.<DECL>", a theorem tools/certify/spec_connect.py wrote and the run's build checked, with
# its axioms pinned to propext, Classical.choice and Quot.sound. With --kernel the driver runs with
# --refute-in-kernel, so a decided NatSpec refutation (natspec_checks) goes on to that theorem
# instead of stopping before any proof.
#
#   tools/certify/stage_neg.sh LEAN_DIR WORK [--kernel] [CASE...]
#
# LEAN_DIR is a solidity-lean checkout the driver clones (--solidity-lean); WORK a fresh directory.
# A case that checks, or is refused at another stage or declaration, fails this script.
set -euo pipefail

die() { echo "stage_neg: error: $*" >&2; exit 1; }
[[ $# -ge 2 ]] || die "usage: stage_neg.sh LEAN_DIR WORK [--kernel] [CASE...]"
lean_dir=$1 work=$2
shift 2
kernel=0
if [[ ${1:-} == --kernel ]]; then kernel=1; shift; fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fix="$here/testdata/fixplan"
mkdir -p "$work"
fails=0
ran=0
while IFS=$'\t' read -r name source stage decl; do
    [[ -z "$name" || "$name" == \#* ]] && continue
    if [[ $# -gt 0 ]] && ! printf '%s\n' "$@" | grep -qxF "$name"; then continue; fi
    ran=$((ran + 1))
    d="$work/$name"
    rm -rf "$d"
    mkdir -p "$d"
    extra=()
    [[ $kernel -eq 1 ]] && extra=(--refute-in-kernel)
    rc=0
    "$here/certify" "$fix/$source" natspec --solidity-lean "$lean_dir" --work "$d/work" \
        --record-dir "$d/rec" --no-negatives ${extra[@]+"${extra[@]}"} > "$d/out" 2>&1 || rc=$?
    last="$(tail -n 1 "$d/out")"
    why="$(python3 - "$rc" "$last" "$stage" "$decl" "$kernel" <<'EOF'
import re, sys
rc, last, stage, decl, kernel = sys.argv[1:]
if rc != "1":
    print(f"exit {rc}, not 1 (REJECTED)")
elif not re.search(stage, last):
    print(f"not at the registered stage {stage}")
elif kernel == "1" and not re.search(r"refuted in the kernel by \S*\." + re.escape(decl) + r"\b", last):
    print(f"not refuted in the kernel at {decl}")
EOF
)"
    if [[ -n "$why" ]]; then
        echo "$name: FAIL: $why: $last" >&2
        fails=$((fails + 1))
    elif [[ $kernel -eq 1 ]]; then
        echo "$name: refused at its registered stage, at $decl: ${last:0:300}"
    else
        echo "$name: refused at its registered stage: ${last:0:300}"
    fi
done < "$fix/stages.tsv"
[[ $ran -gt 0 ]] || die "no case ran"
[[ $fails -eq 0 ]] || die "$fails of $ran case(s) not refused as registered"
echo "stage_neg: $ran case(s) refused as registered"
