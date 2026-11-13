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
    # A dry run proves nothing, so solar-verify may exit 1 for obligations it did not discharge.
    [[ -s "$out/VC.lean" ]] || { cat "$out/verify.log" >&2; die "solar-verify wrote no Lean for $source"; }
    sed -n 's/^-- Left raw, outside the Lean fragment: //p' "$out/VC.lean" > "$out/refused"
elif reason="$(grep -m1 -oE -- '--dump-lean: .*outside the Lean fragment.*' "$out/verify.log")"; then
    printf '%s (every function): %s\n' "$source" "${reason#--dump-lean: }" > "$out/refused"
else
    cat "$out/verify.log" >&2
    die "solar-verify fails on $source (exit $status)"
fi
