#!/usr/bin/env bash
# Unpack an earlier slot's pack.sh archive and check it against that slot's replay plan.
#
#   tools/certify/erc20_any/unpack.sh SHIP_DIR SHIP_SHA256 PRIOR_PLAN OUT
#
# Refuses (exit 4) unless SHIP_DIR/ship.tar has sha256 SHIP_SHA256, every file it holds matches
# SHIP_DIR/files after unpacking, and every olean PRIOR_PLAN names (the toolchain's included) has
# the sha256 the plan gives it. Copies the archived check_final.log and fresh.json into OUT, so
# replay.sh rebuilds the same plan. Writes OUT/precheck.
set -uo pipefail
ship=$(realpath "$1"); want=$2; plan=$(realpath "$3"); out=$(realpath -m "$4")
refuse() { echo "refused: $1" >&2; exit 4; }
[[ $(sha256sum < "$ship/ship.tar" | cut -c1-64) == "$want" ]] || refuse "ship.tar is not $want"
tar -xPf "$ship/ship.tar" || refuse "unpacking failed"
sha256sum -c --quiet "$ship/files" || refuse "an unpacked file differs from the archive's list"
mkdir -p "$out"
cf=$(awk '$2 ~ /\/check_final\.log$/ { print $2 }' "$ship/files"); fj=$(awk '$2 ~ /\/fresh\.json$/ { print $2 }' "$ship/files")
cp "$cf" "$out/check_final.log" && cp "$fj" "$out/fresh.json" || refuse "no check_final.log or fresh.json in the archive"
python3 - "$plan" > "$out/precheck" <<'PY' || refuse "the unpacked tree does not match the plan (see precheck)"
import hashlib, sys
bad = n = 0
for l in open(sys.argv[1]).read().splitlines():
    f = l.split()
    n += 1
    try:
        ok = hashlib.sha256(open(f[3], "rb").read()).hexdigest() == f[2]
    except OSError:
        ok = False
    if not ok:
        bad += 1
        print("mismatch", f[1], f[3])
print(f"precheck planned {n} mismatched {bad}")
sys.exit(1 if bad else 0)
PY
tail -1 "$out/precheck"
