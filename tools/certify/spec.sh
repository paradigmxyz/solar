#!/usr/bin/env bash
# Exports a contract's solar-verify verification conditions as Lean and lists what is refused.
#
#   tools/certify/spec.sh SOLAR_VERIFY SOURCE OUT_DIR NAMESPACE
#
# Runs SOLAR_VERIFY --dry-run --dump-lean OUT_DIR/VC.lean --lean-namespace NAMESPACE SOURCE (the
# log goes to OUT_DIR/verify.log) and writes OUT_DIR/refused, one "FUNCTION: reason" line per
# function whose conditions fall outside the fragment solidity-lean's checks model: those solar-verify
# leaves raw ("-- Left raw, outside the Lean fragment: ..."), or, from a solar-verify that refuses
# the whole export instead, one "SOURCE (every function): reason" line. Refused functions are
# unproved; the caller must never count them as passing. Any other failure of solar-verify fails.
set -euo pipefail

die() { echo "certify: error: $*" >&2; exit 1; }
[[ $# -eq 4 ]] || die "usage: spec.sh SOLAR_VERIFY SOURCE OUT_DIR NAMESPACE"
verify=$1 source=$2 out=$3 ns=$4
[[ -x "$verify" ]] || die "no solar-verify binary at $verify"
mkdir -p "$out"
rm -f "$out/VC.lean" "$out/refused"
status=0
"$verify" --dry-run --dump-lean "$out/VC.lean" --lean-namespace "$ns" "$source" > "$out/verify.log" 2>&1 || status=$?
if [[ $status -le 1 ]]; then
    # A dry run proves nothing, so solar-verify may exit 1 for obligations it did not discharge. It
    # then still ends with its summary line, which a run cut short does not print: without it, or
    # with a count of errors its status contradicts, the export is not taken, whatever it wrote.
    errors="$(sed -n -E 's/^verification results:: [0-9]+ verified, ([0-9]+) errors$/\1/p' "$out/verify.log")"
    if [[ -z "$errors" ]]; then
        tail -n 20 "$out/verify.log" >&2
        die "solar-verify exits $status without its summary line: its run on $source did not finish"
    fi
    [[ "$errors" =~ ^[0-9]+$ ]] || die "solar-verify prints more than one summary line for $source"
    if [[ $status -eq 0 && $errors -ne 0 ]] || [[ $status -eq 1 && $errors -eq 0 ]]; then
        die "solar-verify exits $status but reports $errors errors for $source"
    fi
    [[ -s "$out/VC.lean" ]] || { cat "$out/verify.log" >&2; die "solar-verify wrote no Lean for $source"; }
    # The whole module, not a truncated one: its namespace closed on its last line.
    python3 "$(dirname "$0")/export_check.py" vc "$out/VC.lean" "$ns" || die "the exported conditions of $source are not whole"
    sed -n 's/^-- Left raw, outside the Lean fragment: //p' "$out/VC.lean" > "$out/refused"
elif reason="$(grep -m1 -oE -- '--dump-lean: .*outside the Lean fragment.*' "$out/verify.log")"; then
    printf '%s (every function): %s\n' "$source" "${reason#--dump-lean: }" > "$out/refused"
else
    cat "$out/verify.log" >&2
    die "solar-verify fails on $source (exit $status)"
fi
