#!/usr/bin/env bash
# Negative controls for the published statement (scripts/certify step 7). Each variant of
# CERTIFIED/NAME/{Statement,Final}.lean, written by tools/certify/final.py from a mutated
# shape.json or bytecode, must fail to reach the strongest status: final.py refuses it, the kernel
# refuses the statement or the proof (a mismatch: the binding's decide, or the proof's type against
# the fixed statement), the checker (tools/certify/check_final.lean) refuses it, or the axiom audit
# does. A variant that checks fails this script, except one labelled *-below-strongest (a declared
# vacuity), which must be recorded below the strongest status.
#
#   tools/certify/final_neg.sh LEAN_DIR EXAMPLES NAME CONTRACT HEX
#
# Variants: a stale or substituted artifact (one byte of the bytecode changed; another
# contract's pinned bytecode.hex), the strongest status claimed without a proof of resources (or,
# for a certified contract, a wrong gas bound), a wrong postcondition, no allowed status, a dropped
# environment premise, a proof term that is sorryAx, a vacuous specification (allowed and post
# true: with no witness, with a sorry witness, with the real proof's witness, or with a declared
# vacuity), and two hijacks by the proof module: declarations under Certified.NAME.Final standing
# in for the statement's names, an alias that makes the statement's name ambiguous, and a macro
# that rewrites the final theorem's command, and a declaration added without the kernel check
# (refused by the kernel replay). For a
# contract with no specification (kind compiled): the strongest status claimed with a vacuous
# one, a gas bound below the proof's, and a postcondition on a compiled statement.
# CERTIFY_LEAN_MEMORY caps each Lean process (MB, default 3000); CERTIFY_WRAP is a command prefix.
set -euo pipefail

die() { echo "final_neg: error: $*" >&2; exit 1; }
[[ $# -eq 5 ]] || die "usage: final_neg.sh LEAN_DIR EXAMPLES NAME CONTRACT HEX"
lean_dir=$1 examples=$2 name=$3 contract=$4 hex=$5
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
read -r -a wrap <<<"${CERTIFY_WRAP:-}"
mem="${CERTIFY_LEAN_MEMORY:-3000}"
shape="$lean_dir/$examples/$name/shape.json"
[[ -f "$shape" ]] || die "$shape is missing"
work="$lean_dir/.lake/certify-final-neg-$name"
rm -rf "$work"
mkdir -p "$work"
fails=0
proof_mod="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["proof_module"])' "$shape")"

# lean_in DIR ARGS...: run ARGS inside the package, with DIR's oleans ahead of the package's.
lean_in() {
    local olean=$1; shift
    # The variants import proof modules, whose initializers run: in the sandbox, writable only
    # in the work directory.
    (cd "$lean_dir" && ${wrap[@]+"${wrap[@]}"} bash "$here/sandbox.sh" "$work" lake env bash -c 'LEAN_PATH="$0:$LEAN_PATH" LEAN_NUM_THREADS=1 "$@"' "$olean" "$@")
}

# compile DIR MODULE FILE: FILE as MODULE's olean under DIR.
compile() {
    local olean=$1 module=$2 file=$3
    local out="$olean/${module//.//}"
    mkdir -p "$(dirname "$out")"
    rm -f "$out.olean" "$out.ilean"
    lean_in "$olean" lean -M "$mem" -o "$out.olean" -i "$out.ilean" "$file"
}

# variant LABEL HEX EDIT [PROOF_MODULE_FILE]: write the variant and require it to be refused.
# PROOF_MODULE_FILE, when given, is compiled first as the module NAME.NegProof (a hostile proof
# module the edit names).
variant() {
    local label=$1 vhex=$2 edit=$3 extra=${4:-}
    local d="$work/$label" olean="$work/$label/olean"
    # Lean finds a module under the first search path entry holding its first component: the
    # overlay mirrors the package's NAME tree by symbolic links, and each module it compiles
    # replaces its link (compile removes it first, so nothing writes through to the package).
    mkdir -p "$olean"
    cp -rs "$lean_dir/.lake/build/lib/lean/$name" "$olean/$name"
    python3 - "$shape" "$d/shape.json" "$edit" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
exec(sys.argv[3])
json.dump(d, open(sys.argv[2], "w"))
EOF
    if ! python3 "$here/final.py" "$d/shape.json" --name "$name" --contract "$contract" --hex "$vhex" \
        --out "$d/Final.lean" --statement-out "$d/Statement.lean" --record "$d/record.json" > /dev/null 2> "$d/final.err"; then
        echo "$label: refused by final.py ($(head -c 160 "$d/final.err"))"
        return
    fi
    if [[ -n "$extra" ]]; then
        compile "$olean" "$name.NegProof" "$extra" > "$d/extra.out" 2>&1 ||
            { cat "$d/extra.out" >&2; die "$label: the hostile proof module does not build"; }
    fi
    if ! compile "$olean" "$name.Statement" "$d/Statement.lean" > "$d/statement.out" 2>&1; then
        echo "$label: refused by the kernel, statement ($(grep -m1 -E 'error' "$d/statement.out" | cut -c1-160))"
        return
    fi
    if ! compile "$olean" "$name.Final" "$d/Final.lean" > "$d/final.out" 2>&1; then
        echo "$label: refused by the kernel ($(grep -m1 -E 'error' "$d/final.out" | cut -c1-160))"
        return
    fi
    local vproof
    vproof="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["proof_module"])' "$d/shape.json")"
    if ! lean_in "$olean" lean --run "$here/check_final.lean" "$name" "$vproof" > "$d/check.out" 2>&1; then
        echo "$label: refused by the checker ($(grep -m1 -E 'check_final|error|exception' "$d/check.out" | cut -c1-160))"
        return
    fi
    # The variant's own modules (those compiled into its overlay) through the kernel; the rest of
    # the closure is the package's, replayed by the main run.
    local vlist vmods=()
    vlist="$(awk -v w="$olean/" '$1 == "MODULE" && index($3, w) == 1 { print $2 }' "$d/check.out")" ||
        die "cannot list the modules of the variant $label"
    [[ -z "$vlist" ]] || mapfile -t vmods <<<"$vlist"
    if [[ ${#vmods[@]} -gt 0 ]] && ! lean_in "$olean" leanchecker "${vmods[@]}" > "$d/replay.out" 2>&1; then
        echo "$label: refused by the kernel replay ($(grep -m1 -E 'kernel|error' "$d/replay.out" | cut -c1-160))"
        return
    fi
    if python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("strongest") else 1)' "$d/record.json"; then
        echo "$label: ACCEPTED at the strongest status (it must not be)" >&2
        fails=$((fails + 1))
    elif [[ "$label" == *-below-strongest ]]; then
        echo "$label: checks, recorded below the strongest status (as it must be)"
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
vac='d["kind"] = "certified"; d["allowed"] = "fun _ _ => True"; d["post"] = "fun _ _ => True"; d.pop("spec_export", None); d.pop("nonvacuous", None); d.pop("vacuity", None); d.setdefault("gas", "fun _ => 0")'
if grep -q '"kind": "compiled"' "$shape"; then
    # No specification: the strongest status cannot be claimed with a made-up one, and the gas
    # bound is the proof's.
    variant strongest-with-vacuous-spec "$hex" "$vac"
    variant strongest-with-vacuous-spec-witness "$hex" "$vac"'; d["vacuity"] = "declared by the variant"'
    variant wrong-gas-bound "$hex" 'd["gas"] = "fun _ => 1"'
    variant spec-on-compiled "$hex" 'd["post"] = "fun call obs => obs.success = true"'
else
    if grep -q '"kind": "certified"' "$shape"; then
        variant wrong-gas-bound "$hex" 'd["gas"] = "fun _ => 0"'
    else
        variant strongest-without-resources "$hex" 'd["kind"] = "certified"; d["gas"] = "fun _ => 0"'
    fi
    variant wrong-post "$hex" 'd.pop("spec_export", None); d["post"] = "fun call obs => obs.success = true"'
    variant no-allowed-status "$hex" 'd["allowed"] = "(fun _ _ => False)"'
    if python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("env") else 1)' "$shape"; then
        variant dropped-env-premise "$hex" 'd["env"] = []'
    fi
    variant vacuous-spec "$hex" "$vac"
    # True of everything, but not written as True: only the kernel's NonVacuous refuses these.
    vac2="$vac"'; d["allowed"] = "fun call obs => call = call"; d["post"] = "fun call obs => obs = obs"'
    variant vacuous-spec-sorry-witness "$hex" "$vac2"'; d["nonvacuous"] = "sorryAx _ false"'
    if grep -q '"nonvacuous"' "$shape"; then
        variant vacuous-spec-real-witness "$hex" "$vac2"'; d["nonvacuous"] = '"$(python3 -c 'import json,sys; print(repr(json.load(open(sys.argv[1]))["nonvacuous"]))' "$shape")"
    fi
    variant vacuous-spec-below-strongest "$hex" 'd["vacuity"] = "declared by the variant"; d.pop("nonvacuous", None)'
fi
# A vacuous specification with a proof the kernel accepts (the published proof with every result
# allowed and meeting the specification), as a negative test writes it: final.py refuses it without a
# non-vacuity witness, and with a sorry witness the checker's axiom audit does.
real="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["proof"])' "$shape")"
vac2='d["kind"] = "certified"; d["allowed"] = "fun call obs => call = call"; d["post"] = "fun call obs => obs = obs"; d.pop("spec_export", None); d.pop("nonvacuous", None); d.pop("vacuity", None); d.setdefault("gas", "fun _ => 0")'
if grep -q '"kind": "compiled"' "$shape"; then
    export VP="Exists.imp (fun _ h => ⟨h.1, _root_.SolidCore.Refinement.Shapes.Compiled.certified h.2 (fun _ _ _ _ => rfl) (fun _ _ _ _ => rfl)⟩) ($real)"
elif ! python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("env") else 1)' "$shape"; then
    export VP="Exists.imp (fun _ h => ⟨h.1, ⟨h.2.inhabited, h.2.equiv, fun call hc => (h.2.terminates call hc).imp (fun _ x => ⟨x.1, rfl⟩), fun _ _ _ _ => rfl, h.2.resources⟩⟩) ($real)"
else
    export VP=
fi
if [[ -n "$VP" ]]; then
    variant vacuous-proof-no-witness "$hex" "$vac2"'; import os; d["proof"] = os.environ["VP"]'
    variant vacuous-proof-sorry-witness "$hex" "$vac2"'; import os; d["proof"] = os.environ["VP"]; d["nonvacuous"] = "sorryAx _ false"'
fi
# A hijack: the proof module declares, under Certified.NAME.Final, names the
# statement uses (and a Statement of its own), each true of everything, and proves only those.
cat > "$work/hijack.lean" <<EOF
import $proof_mod
namespace Certified.$name.Final
def Shapes.Certified {α β γ δ ε ζ η : Sort _} (_ : α) (_ : β) (_ : γ) (_ : δ) (_ : ε) (_ : ζ) (_ : η) : Prop := True
theorem hijack : True := trivial
end Certified.$name.Final
EOF
variant hijack-names "$hex" 'd["proof_module"] = "'"$name"'.NegProof"; d["proof"] = "Certified.'"$name"'.Final.hijack"' "$work/hijack.lean"
# An alias in the proof module makes the statement's own name ambiguous: Lean takes whichever
# reading elaborates, here a Statement that is True.
cat > "$work/alias.lean" <<EOF
import $proof_mod
namespace NegAlias
def Statement : Prop := True
end NegAlias
namespace Certified.$name.Final
export NegAlias (Statement)
end Certified.$name.Final
EOF
variant hijack-alias "$hex" 'd["proof_module"] = "'"$name"'.NegProof"; d["proof"] = "trivial"' "$work/alias.lean"
# A macro in the proof module rewrites the final theorem's own command to state True.
{
    echo "import $proof_mod"
    cat <<'EOF'
open Lean in
macro_rules
  | `(command| theorem $n:ident : $t:term := $_v:term) => do
    if t.raw.isOfKind ``Lean.Parser.Term.paren then Macro.throwUnsupported
    `(command| theorem $n : (True) := trivial)
EOF
} > "$work/macro.lean"
variant hijack-macro "$hex" 'd["proof_module"] = "'"$name"'.NegProof"' "$work/macro.lean"
# A declaration added with the kernel check switched off (no banned word in the text): the
# checker reads its type, and only the kernel replay refuses it.
{
    echo "import $proof_mod"
    cat <<'EOF'
open Lean Elab Command Term Meta in
run_cmd liftTermElabM do
  let opt := Name.mkStr (Name.mkStr .anonymous "debug") "skipKernelTC"
  withOptions (·.setBool opt true) do
    addDecl (.thmDecl { name := `NegProof.fake, levelParams := [], type := mkConst ``False, value := mkConst ``True.intro })
EOF
} > "$work/skipkernel.lean"
variant hijack-skip-kernel "$hex" 'd["proof_module"] = "'"$name"'.NegProof"; d["proof"] = "False.elim NegProof.fake"; d.get("nonvacuous") and d.__setitem__("nonvacuous", "False.elim NegProof.fake")' "$work/skipkernel.lean"
variant sorry-proof "$hex" 'd["proof"] = "sorryAx _ false"'
[[ $fails -eq 0 ]] || die "$fails variant(s) of the published statement checked"
echo "final_neg: every variant refused"
