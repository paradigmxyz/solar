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
#   cx.base.reads = false fails); each block's cost set one below the block's actual cost, and the
#   entry block's cost set to 0 (blocks_ok's decide fails on blocksXL). A block's actual cost is
#   read from the exact bytecode, not from the certificate: the instructions of the block's
#   straight-line run (from its offset to its first STOP, RETURN, REVERT, INVALID or JUMP, or the
#   next block), less a final STOP, RETURN or REVERT, whose base gas is 0 (their memory expansion
#   is the bound's separate memoryGasCost term). A control sets every cost to its actual cost,
#   which blocks_ok must accept, so the bound is tight; a declared cost below the actual one fails
#   the run before any case;
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
# Pattern controls: an unrelated edit (a syntax error appended) to CtxGas and to Lowering must fail
# without the cost and stage0 patterns, so neither pattern matches any error.
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
    # Each file a link, each directory made (cp -rs is not portable here).
    (cd "$built" && find . -type d -exec mkdir -p "$d/{}" \; && find . ! -type d -exec ln -s "$built/{}" "$d/{}" \;)
}

# run CASE MODULE PROOF EXPECT PATTERN: compile the case's edited MODULE (unless it is PROOF) into
# its shadow tree, then elaborate PROOF (a module path under NAME: the edited copy when it is
# MODULE, else the unchanged source). EXPECT is pass, fail with PATTERN in the output, or other: fail
# without PATTERN in it.
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
    elif [[ $expect == other ]]; then
        if tr -s '\n ' '  ' < "$out" | grep -Eq "$pattern"; then
            echo "FAIL $case: an unrelated edit matches the pattern ($pattern):"; sed 's/^/  /' "$out" | head -20
            failures=$((failures + 1))
        else echo "ok   $case: $proof fails without the pattern: $(grep -m1 -E 'error' "$out" | cut -c1-160)"; fi
    elif tr -s '\n ' '  ' < "$out" | grep -Eq "$pattern"; then
        echo "ok   $case: $proof rejects it: $(grep -m1 -E 'error' "$out" | cut -c1-160)"
    else
        echo "FAIL $case: $proof fails without its pattern ($pattern):"; sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1))
    fi
}

# case_ CASE MODULE PROOF PATTERN PYTHON [EXPECT]: one edit, skipped (and counted) when its text is
# absent; EXPECT (default fail) as for run.
case_() {
    local case=$1 module=$2 proof=$3 pattern=$4 code=$5 expect=${6:-fail} rc=0
    edit "$case" "$module" "$code" || rc=$?
    if [[ $rc -eq 2 ]]; then echo "skip $case: no such text in $module"; skips=$((skips + 1)); return; fi
    [[ $rc -eq 0 ]] || { echo "FAIL $case: the edit failed"; failures=$((failures + 1)); return; }
    run "$case" "$module" "$proof" "$expect" "$pattern"
}

# set_costs VALUES: Python setting the certificate's costs to VALUES, a comma-separated list in
# which an empty entry keeps the declared cost.
set_costs() {
    echo 'm = re.search(r"costs := \[([0-9, ]*)\]", s)
c = [int(x) for x in m.group(1).split(",")]
v = "'"$1"'".split(",")
c = [int(x) if x else d for x, d in zip(v, c)] if len(v) == len(c) else None
s = s[:m.start(1)] + ", ".join(map(str, c)) + s[m.end(1):]'
}

bound="$(sed -nE 's/^theorem gas_le : .* ≤ ([0-9]+) := .*/\1/p' "$src/AnyCaller.lean")"
[[ -n $bound ]] || { echo "negative_any: no gas_le bound in AnyCaller.lean" >&2; exit 1; }
below=$((bound - 1))
contract="$(sed -nE 's/^import [^.]+\.gen\.([A-Za-z0-9_]+)\.Stage0$/\1/p' "$src/Lowering.lean")"
[[ -n $contract ]] || { echo "negative_any: no stage0 import in Lowering.lean" >&2; exit 1; }

# Each block's actual cost from the exact bytecode (see the header), against the declared costs.
read -r declared actual < <(python3 -I - "$src/CtxGas.lean" "$src/bytecode.hex" <<'EOF'
import re, sys
s = open(sys.argv[1]).read()
code = bytes.fromhex(open(sys.argv[2]).read().strip())
offs = [int(x, 0) for x in re.search(r"offsets := \[([0-9a-fx, ]*)\]", s).group(1).split(",")]
costs = [int(x) for x in re.search(r"costs := \[([0-9, ]*)\]", s).group(1).split(",")]
if len(offs) != len(costs):
    sys.exit("negative_any: %d offsets but %d costs" % (len(offs), len(costs)))
actual = []
for b, o in enumerate(offs):
    end = offs[b + 1] if b + 1 < len(offs) else len(code)
    n, pc, last = 0, o, None
    while pc < end:
        op = code[pc]
        n, last = n + 1, op
        pc += 1 + (op - 0x5F if 0x60 <= op <= 0x7F else 0)
        if op in (0x00, 0x56, 0xF3, 0xFD, 0xFE):
            break
    actual.append(n - (1 if last in (0x00, 0xF3, 0xFD) else 0))
print(",".join(map(str, costs)), ",".join(map(str, actual)))
EOF
)
[[ -n $actual ]] || { echo "negative_any: no block costs from CtxGas.lean and bytecode.hex" >&2; exit 1; }
echo "block costs: declared $declared, actual $actual"
IFS=, read -ra dcost <<< "$declared"
IFS=, read -ra acost <<< "$actual"
for b in "${!acost[@]}"; do
    (( acost[b] <= dcost[b] )) || { echo "negative_any: block $b declares $((dcost[b])) below its actual cost $((acost[b]))" >&2; exit 1; }
done
cost_pattern='proposition AssembleCtxGas\.blocksXL F \(norm prog\) cx 0 \(norm prog\)\.blocks = true is false'

run control-AnyCaller "" AnyCaller pass
run control-Final "" Final pass
run control-Lowering "" Lowering pass
case_ CtxGas-reads CtxGas AnyCaller 'error' 's = s.replace("reads := false", "reads := true", 1)'
case_ control-CtxGas-actual CtxGas CtxGas '' "$(set_costs "$actual")" pass
for b in "${!acost[@]}"; do
    v=(); for k in "${!acost[@]}"; do if (( k == b )); then v+=($((acost[b] - 1))); else v+=(""); fi; done
    case_ "CtxGas-cost-b$b" CtxGas CtxGas "$cost_pattern" "$(set_costs "$(IFS=,; echo "${v[*]}")")"
done
v=(0); for ((k = 1; k < ${#acost[@]}; k++)); do v+=(""); done
case_ CtxGas-cost-zero CtxGas CtxGas "$cost_pattern" "$(set_costs "$(IFS=,; echo "${v[*]}")")"
case_ control-CtxGas-pattern CtxGas CtxGas "$cost_pattern" 's += "\ntheorem pattern_control : (\n"' other
case_ AnyCaller-gas-below AnyCaller AnyCaller 'gas_le|decide' "s = s.replace('$bound', '$below')"
stage0_pattern="proposition Generic\\.checkSym fuel mirFuel coreLit SolarTV\\.Export\\.$contract\\.stage0 fns = true is false|proposition Generic\\.checkValue mirFuel coreLit SolarTV\\.Export\\.$contract\\.stage0 fns Generic\\.EnvFlags\\.allSym = true is false"
case_ Stage0-value-branch "gen/$contract/Stage0" Lowering "$stage0_pattern" 's = re.sub(r"(\.effect \(some (\d+)\) \.callvalue \[\],\s*\.ne (\d+) \(\.value \2\) \(\.const 0\)\],\s*\.branch \(\.value \3\)) (\d+) (\d+)", r"\1 \5 \4", s, count=1)'
case_ Stage0-callvalue-as-caller "gen/$contract/Stage0" Lowering "$stage0_pattern" 's = re.sub(r"\.effect \(some (\d+)\) \.callvalue \[\]", r".effect (some \1) .caller []", s, count=1)'
case_ control-Lowering-pattern Lowering Lowering "$stage0_pattern" 's += "\ntheorem pattern_control : (\n"' other
case_ Statement-any-gas-below Statement Final 'error' "s = s.replace('(fun _ => $bound)', '(fun _ => $below)', 1)"
case_ Statement-any-no-ctx-ok Statement Final 'error' 's = s.replace("ctx.Ok → ∃ c,", "True → ∃ c,", 1)'
case_ AnyCaller-nonpayable-no-value AnyCaller AnyCaller 'published_nonpayable|error' 's = s.replace("call.callvalue ≠ 0 →\n", "True →\n", 1)'
case_ Statement-nonpayable-no-value Statement Final 'error' 's = s.replace("call.callvalue ≠ 0 →", "True →", 1)'
case_ Statement-nonpayable-success Statement Final 'error' 's = s.replace("some { success := false,", "some { success := true,", 1)'
case_ Statement-value-wide Statement Final 'error' 's = s.replace("∀ v, v < 2 ^ 256 →", "∀ v, v < 2 ^ 257 →", 1)'

echo "$name every-caller negative tests: $cases cases, $failures failed, $skips skipped"
[[ $failures -eq 0 && $skips -eq 0 ]]
