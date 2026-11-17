#!/usr/bin/env bash
# Negative tests of the every-caller route of a certified contract: meaning-changing edits of the
# modules behind its every-caller claim (NAME/AnyCaller.lean, NAME/CtxGas.lean, the check of
# stage0 for every caller in NAME/Lowering.lean, and the published statement's every-caller
# conjunct) make the generated proofs fail.
#
#   tools/certify/negative_any.sh LEAN_DIR EXAMPLES NAME
#
# Run after scripts/certify built NAME's proofs on the any route in LEAN_DIR, whose per-contract
# directories are under EXAMPLES. Each case writes an edited copy of one generated module,
# compiles it into a separate olean tree that shadows the built one, and elaborates the unchanged
# module that imports it (or the edited module itself, when the edit is to the claim it proves),
# which must fail with its pattern; a control on unedited copies must pass. The edits:
#
# - CtxGas: the checker certificate's environment-reads flag set (AnyCaller's premise
#   cx.base.reads = false fails), and its first block cost lowered by one (check_ok fails);
# - AnyCaller: its gas bound lowered by one (gas_le fails);
# - stage0: the call value check's branch taken the other way, and the call value read as the
#   caller (the checks for every caller and for value calls in Lowering fail);
# - AnyCaller: the every-value claim's premise of a value other than 0 dropped (published_nonpayable
#   fails: a call without value runs the function);
# - Statement: the every-caller conjunct's gas bound lowered by one, and its real-context premise
#   (ctx.Ok: a caller below 2^160 other than the contract) dropped (Final fails); the every-value
#   conjunct's premise of a value other than 0 dropped, its revert stated as a success, and the
#   value range widened past 2^256 (Final fails).
#
# A case whose text is not in the module counts as a skip, and the run fails on any skip, since
# every module of the any route has them. LEAN_MEMORY caps each Lean process (MB, default 6000).
# Exit 0 when every case and control behaves.
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: negative_any.sh LEAN_DIR EXAMPLES NAME" >&2; exit 1; }
tv="$(cd "$1" && pwd)"
examples=$2 name=$3
cd "$tv"
export LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-1}"
memory="${LEAN_MEMORY:-6000}"
src="$tv/$examples/$name"
built="$tv/.lake/build/lib/lean/$name"
for m in AnyCaller CtxGas Lowering Statement Final; do
    [[ -f "$src/$m.lean" && -f "$built/$m.olean" ]] || { echo "negative_any: $name has no built $m (not the any route?)" >&2; exit 1; }
done
lean_path="$(lake env printenv LEAN_PATH)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cases=0 failures=0 skips=0

# edit CASE MODULE PYTHON: the case's copy of MODULE (path under NAME, without .lean) with the
# Python statements PYTHON applied to its text s; exit status 2 when the edit changed nothing.
edit() {
    local case=$1 module=$2 code=$3
    mkdir -p "$tmp/$case/src/$name/$(dirname "$module")"
    python3 -c '
import re, sys
s = open(sys.argv[1]).read()
t = s
exec(sys.argv[3])
if s == t:
    sys.exit(2)
open(sys.argv[2], "w").write(s)
' "$src/$module.lean" "$tmp/$case/src/$name/$module.lean" "$code"
}

# shadow CASE: an olean tree for the case, every built module of NAME linked.
shadow() {
    local d="$tmp/$1/olean/$name"
    mkdir -p "$d"
    cp -rs "$built/." "$d/"
}

# run CASE MODULE PROOF EXPECT PATTERN: compile the case's edited MODULE (unless it is PROOF) into
# its shadow tree, then elaborate PROOF (a module path under NAME: the edited copy when it is
# MODULE, else the unchanged source). EXPECT is pass, or fail with PATTERN in the output.
run() {
    local case=$1 module=$2 proof=$3 expect=$4 pattern=${5:-}
    local dir="$tmp/$case" out="$tmp/$case/out" file status=0
    cases=$((cases + 1))
    shadow "$case"
    if [[ -n $module && $module != "$proof" ]]; then
        rm -f "$dir/olean/$name/$module.olean" "$dir/olean/$name/$module.ilean"
        if ! LEAN_PATH="$dir/olean:$lean_path" lean -M "$memory" -R "$dir/src" \
            -o "$dir/olean/$name/$module.olean" "$dir/src/$name/$module.lean" > "$out" 2>&1; then
            echo "FAIL $case: the edited $module does not compile:"; sed 's/^/  /' "$out" | head -20
            failures=$((failures + 1)); return
        fi
    fi
    if [[ $module == "$proof" ]]; then file="$dir/src/$name/$proof.lean"; else file="$src/$proof.lean"; fi
    LEAN_PATH="$dir/olean:$lean_path" lean -M "$memory" "$file" > "$out" 2>&1 || status=$?
    if [[ $expect == pass ]]; then
        if [[ $status -eq 0 ]]; then echo "ok   $case: $proof accepts it"
        else echo "FAIL $case: $proof rejects the unedited modules:"; sed 's/^/  /' "$out" | head -20; failures=$((failures + 1)); fi
    elif [[ $status -eq 0 ]]; then
        echo "FAIL $case: $proof accepts the edited $module"; failures=$((failures + 1))
    elif tr -s '\n ' '  ' < "$out" | grep -Eq "$pattern"; then
        echo "ok   $case: $proof rejects it: $(grep -m1 -E 'error' "$out" | cut -c1-160)"
    else
        echo "FAIL $case: $proof fails without its pattern ($pattern):"; sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1))
    fi
}

# case_ CASE MODULE PROOF PATTERN PYTHON: one edit, skipped (and counted) when its text is absent.
case_() {
    local case=$1 module=$2 proof=$3 pattern=$4 code=$5 rc=0
    edit "$case" "$module" "$code" || rc=$?
    if [[ $rc -eq 2 ]]; then echo "skip $case: no such text in $module"; skips=$((skips + 1)); return; fi
    [[ $rc -eq 0 ]] || { echo "FAIL $case: the edit failed"; failures=$((failures + 1)); return; }
    run "$case" "$module" "$proof" fail "$pattern"
}

bound="$(sed -nE 's/^theorem gas_le : .* ≤ ([0-9]+) := .*/\1/p' "$src/AnyCaller.lean")"
[[ -n $bound ]] || { echo "negative_any: no gas_le bound in AnyCaller.lean" >&2; exit 1; }
below=$((bound - 1))
contract="$(sed -nE 's/^import [^.]+\.gen\.([A-Za-z0-9_]+)\.Stage0$/\1/p' "$src/Lowering.lean")"
[[ -n $contract ]] || { echo "negative_any: no stage0 import in Lowering.lean" >&2; exit 1; }

run control-AnyCaller "" AnyCaller pass
run control-Final "" Final pass
run control-Lowering "" Lowering pass
case_ CtxGas-reads CtxGas AnyCaller 'error' 's = s.replace("reads := false", "reads := true", 1)'
case_ CtxGas-cost CtxGas CtxGas 'check_ok|blocks_ok|decide' 's = re.sub(r"costs := \[(\d+)", lambda m: "costs := [%d" % (int(m.group(1)) - 1), s, count=1)'
case_ AnyCaller-gas-below AnyCaller AnyCaller 'gas_le|decide' "s = s.replace('$bound', '$below')"
case_ Stage0-value-branch "gen/$contract/Stage0" Lowering 'check_sym_stage0|checkValueSym_stage0' 's = re.sub(r"(\.effect \(some (\d+)\) \.callvalue \[\],\s*\.ne (\d+) \(\.value \2\) \(\.const 0\)\],\s*\.branch \(\.value \3\)) (\d+) (\d+)", r"\1 \5 \4", s, count=1)'
case_ Stage0-callvalue-as-caller "gen/$contract/Stage0" Lowering 'check_sym_stage0|checkValueSym_stage0' 's = re.sub(r"\.effect \(some (\d+)\) \.callvalue \[\]", r".effect (some \1) .caller []", s, count=1)'
case_ Statement-any-gas-below Statement Final 'error' "s = s.replace('(fun _ => $bound)', '(fun _ => $below)', 1)"
case_ Statement-any-no-ctx-ok Statement Final 'error' 's = s.replace("ctx.Ok → ∃ c,", "True → ∃ c,", 1)'
case_ AnyCaller-nonpayable-no-value AnyCaller AnyCaller 'published_nonpayable|error' 's = s.replace("call.callvalue ≠ 0 →\n", "True →\n", 1)'
case_ Statement-nonpayable-no-value Statement Final 'error' 's = s.replace("call.callvalue ≠ 0 →", "True →", 1)'
case_ Statement-nonpayable-success Statement Final 'error' 's = s.replace("some { success := false,", "some { success := true,", 1)'
case_ Statement-value-wide Statement Final 'error' 's = s.replace("∀ v, v < 2 ^ 256 →", "∀ v, v < 2 ^ 257 →", 1)'

echo "$name every-caller negative tests: $cases cases, $failures failed, $skips skipped"
[[ $failures -eq 0 && $skips -eq 0 ]]
