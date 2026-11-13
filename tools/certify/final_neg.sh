#!/usr/bin/env bash
# Negative controls for the published statement (scripts/certify step 6b). Each variant of
# CERTIFIED/NAME/Final.lean, written by tools/certify/final.py from a mutated shape.json or bytecode,
# must fail to reach any status: by a kernel mismatch (the binding's decide, or the proof's type
# against the fixed statement) or by the axiom audit. A variant that checks fails this script.
#
#   tools/certify/final_neg.sh LEAN_DIR EXAMPLES NAME CONTRACT HEX
#
# Variants: a stale or substituted artifact (one byte of the bytecode changed; another
# contract's pinned bytecode.hex), the strongest status claimed without a proof of resources, a
# wrong postcondition, no allowed status, and a proof term that is sorryAx.
# CERTIFY_LEAN is the Lean command (default: lake env lean -M 3000); CERTIFY_WRAP a prefix.
set -euo pipefail

die() { echo "final_neg: error: $*" >&2; exit 1; }
[[ $# -eq 5 ]] || die "usage: final_neg.sh LEAN_DIR EXAMPLES NAME CONTRACT HEX"
lean_dir=$1 examples=$2 name=$3 contract=$4 hex=$5
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
read -r -a wrap <<<"${CERTIFY_WRAP:-}"
read -r -a lean <<<"${CERTIFY_LEAN:-lake env lean -M 3000}"
shape="$lean_dir/$examples/$name/shape.json"
[[ -f "$shape" ]] || die "$shape is missing"
work="$lean_dir/.lake/certify-final-neg-$name"
mkdir -p "$work"
fails=0

# variant LABEL HEX JQ_PYTHON: write the variant and require Lean (or the axiom filter) to refuse it.
variant() {
    local label=$1 vhex=$2 edit=$3
    local vshape="$work/$label.json" vlean="$work/$label.lean" vout="$work/$label.out"
    python3 - "$shape" "$vshape" "$edit" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
exec(sys.argv[3])
json.dump(d, open(sys.argv[2], "w"))
EOF
    python3 "$here/final.py" "$vshape" --name "$name" --contract "$contract" --hex "$vhex" --out "$vlean" > /dev/null ||
        { echo "$label: refused by final.py"; return; }
    local status=0
    (cd "$lean_dir" && "${wrap[@]}" "${lean[@]}" "$vlean") > "$vout" 2>&1 || status=$?
    local bad
    bad="$(grep -o "depends on axioms: \[.*\]" "$vout" | sed -E 's/.*\[(.*)\]/\1/' | tr ',' '\n' | tr -d ' ' |
        grep -v -x -E 'propext|Classical\.choice|Quot\.sound' | sort -u | paste -sd' ' || true)"
    if [[ $status -ne 0 ]]; then
        echo "$label: refused by the kernel ($(grep -m1 -E 'error' "$vout" | cut -c1-160))"
    elif [[ -n "$bad" ]]; then
        echo "$label: refused by the axiom audit ($bad)"
    else
        echo "$label: ACCEPTED (it must not be)" >&2
        fails=$((fails + 1))
    fi
}

flip="$(python3 -c 'import sys; h=sys.argv[1]; i=len(h)//2 & ~1; print(h[:i] + format(int(h[i:i+2],16) ^ 1, "02x") + h[i+2:])' "$hex")"
variant stale-byte "$flip" "pass"
other="$(ls "$lean_dir/$examples"/*/bytecode.hex | grep -v "/$name/bytecode.hex" | head -n 1 || true)"
if [[ -n "$other" ]]; then
    variant substituted-artifact "$(tr -d '[:space:]' < "$other")" "pass"
fi
variant strongest-without-resources "$hex" 'd["kind"] = "certified"; d["gas"] = "(fun _ => 0)"'
variant wrong-post "$hex" 'd["post"] = "fun call obs => obs.success = true"'
variant no-allowed-status "$hex" 'd["allowed"] = "(fun _ _ => False)"'
variant sorry-proof "$hex" 'd["proof"] = "sorryAx _ false"'
[[ $fails -eq 0 ]] || die "$fails variant(s) of the published statement checked"
echo "final_neg: every variant refused"
