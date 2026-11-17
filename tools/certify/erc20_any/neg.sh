#!/usr/bin/env bash
# Mutation negatives for erc20_any: each case must fail (nonzero exit), the control must pass.
#
#   tools/certify/erc20_any/neg.sh LEAN_DIR OUT
#
# Byte and source-digest cases are kernel refutations of the mutated binding. The property cases
# are harness cases: the proof does not certify the mutated specification (they do not show the
# mutated statement false). The fresh-byte case reruns gen.py on a mutated emit.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
lean=$(realpath "$1"); out=$(realpath "$2"); name=erc20_any
d="$out/neg"; mkdir -p "$d"; : > "$d/results"
cd "$lean"
F="Certified.$name.Final"; P="Certified.$name.Proof"
hdr="import $name.Final
open SolarTV SolidCore.Refinement SolidCore.Spec SolidCore.Verify.Generic $F
open SolidCore.Spec.UniswapV2.ERC20.Slice (Valid transferCall balKey balanceSlot)
set_option maxRecDepth 100000"
case_() { local n=$1 want=$2 body=$3
  printf '%s\n\n%s\n' "$hdr" "$body" > "$d/$n.lean"
  /usr/bin/time -v -o "$d/$n.time" lake env lean "$d/$n.lean" > "$d/$n.log" 2>&1; local rc=$?
  local ok=FAIL; { [[ $want == pass && $rc == 0 ]] || [[ $want == fail && $rc != 0 ]]; } && ok=OK
  # A failure only counts when it is the mutation's: never a name, syntax or elaboration-setup error.
  if [[ $want == fail ]] && grep -qE "unknownIdentifier|unknown constant|Unknown constant|unexpected token|unknown namespace" "$d/$n.log"; then ok=FAIL; fi
  local why=$(grep -m1 -oE "error[^:]*: .{0,90}" "$d/$n.log" | tr '\t' ' ')
  echo "$n want=$want exit=$rc $ok peak_rss_kb=$(awk '/Maximum resident/{print $NF}' "$d/$n.time") why=${why:-none}" | tee -a "$d/results"; }
case_ control pass "example : Statement := $P.statement"
case_ byte_first fail "example : SolarTV.Export.UniswapV2ERC20.bytecode = (runtimeBytes.head! + 1) :: runtimeBytes.tail := by decide +kernel"
case_ byte_last fail "example : SolidCore.Spec.UniswapV2.ERC20.Certified.code.data.toList.map (·.toNat) = runtimeBytes.dropLast ++ [runtimeBytes.getLast! ^^^ 1] := by decide +kernel"
case_ source_pins fail "example : SolarTV.Export.UniswapV2ERC20.Manifest.sources = pinnedSources.reverse := by decide"
case_ source_contract fail "example : Expected ⟨SolidCore.Verify.VC.Source.UniswapV2ERC20.importedSourceUnit, \"IUniswapV2ERC20\", 4096⟩ specAny runtimeBytes := $P.any"
case_ prop_credit_plus_one fail "def mutPost (a : Nat) (call : Call) (obs : Observable) : Prop := TargetPostAt a call obs ∧ ∀ σ dst v, call = transferCall σ dst v → a ≠ dst → obs.success = true → obs.storage (balKey dst) = slotValue σ (balKey dst) + v + 1
example : Expected src { specScoped with Post := fun ctx => mutPost ctx.caller } runtimeBytes := $P.scopedExpected"
case_ prop_self_no_overflow fail "def mutPost (a : Nat) (call : Call) (obs : Observable) : Prop := TargetPostAt a call obs ∧ ∀ σ dst v, call = transferCall σ dst v → a = dst → obs.success = true → slotValue σ (balKey dst) + v < 2 ^ 256
example : Expected src { specScoped with Post := fun ctx => mutPost ctx.caller } runtimeBytes := $P.scopedExpected"
case_ prop_unchecked_overflow fail "def mutPost (a : Nat) (call : Call) (obs : Observable) : Prop := TargetPostAt a call obs ∧ ∀ σ dst v, call = transferCall σ dst v → v ≤ slotValue σ (balKey a) → obs.success = true
example : Expected src { specScoped with Post := fun ctx => mutPost ctx.caller } runtimeBytes := $P.scopedExpected"
case_ ctx_self_caller fail "example : Expected src { specAny with Contexts := fun ctx => ctx.caller < 2 ^ 160 } runtimeBytes := $P.any"
case_ gas_minus_one fail "example : Expected src { specAny with gas := fun _ _ => 8199111 } runtimeBytes := $P.any"
case_ footprint_dropped fail "example : Expected src { specScoped with Valid := fun _ call => Valid deployed call } runtimeBytes := $P.scopedExpected"
# Fresh-byte case: gen.py refuses an emit whose bytes differ from the stage export's.
python3 - "$out/fresh.json" "$d/fresh-mut.json" <<'EOF'
import json, sys
j = json.load(open(sys.argv[1]))
for k, v in j["contracts"].items():
    if k.endswith(":UniswapV2ERC20"):
        h = v["bin-runtime"]; v["bin-runtime"] = ("61" if h[:2] != "61" else "60") + h[2:]
json.dump(j, open(sys.argv[2], "w"))
EOF
tmp=$(mktemp -d); python3 "$here/gen.py" "$lean" "$d/fresh-mut.json" negfresh > "$d/gen_mut.log" 2>&1; rc=$?
rm -rf Certified/negfresh
ok=FAIL; [[ $rc != 0 ]] && ok=OK; echo "gen_mutated_emit want=fail exit=$rc $ok" | tee -a "$d/results"
# Source-text cases: one byte of a stage source changed on disk; gen.py
# must refuse each (the manifest's digest no longer matches), and the file is restored after.
mut_src() { local n=$1 f=$2
  cp "$f" "$d/$n.orig"
  python3 -c "import sys;p=sys.argv[1];b=bytearray(open(p,'rb').read());b[len(b)//2]^=1;open(p,'wb').write(b)" "$f"
  python3 "$here/gen.py" "$lean" "$out/fresh.json" negsrc > "$d/$n.log" 2>&1; local rc=$?
  cp "$d/$n.orig" "$f"; rm -rf Certified/negsrc
  local ok=FAIL; [[ $rc != 0 ]] && ok=OK; echo "$n want=fail exit=$rc $ok" | tee -a "$d/results"; }
mut_src src_stage_safemath .cache/uniswap-v2/src/libraries/SafeMath.sol
mut_src src_stage_erc20 .cache/uniswap-v2/src/UniswapV2ERC20.sol
! grep -q -e ' FAIL' -e 'not restored' "$d/results"
