#!/usr/bin/env bash
# Proves in the Lean kernel the obligations 'solar-verify --emit-lean DIR' exported: every
# required obligation in DIR/manifest.json (see docs/VERIFY_LEAN.md) must have a proof that the
# kernel accepts and that uses no axiom but propext, Classical.choice and Quot.sound. Writes
# DIR/discharge/results.tsv and DIR/discharge/inputs.sha256, and exits 0 only when every required
# obligation of every unit is proved: a unit that was not exported, a missing, duplicate or changed
# input, an error, a timeout, another axiom, or a library that is not the one the export was
# written for is a failure, never a pass.
#
#   crates/verify/lean/discharge.sh DIR [TIMEOUT_SECONDS]
#
# Environment:
#   DISCHARGE_LOCK      a sha256sum file of the inputs (as inputs.sha256 is written): the run fails
#                       unless the manifest and every exported module match it.
#   DISCHARGE_BACKENDS  proof backends tried in order, as 'name=tactic;name=tactic'
#                       (default 'vc_valid=vc_valid'); results.tsv names the one that proved each
#                       obligation. Every backend's proof passes the same axiom check.
#   DISCHARGE_IMPORTS   modules the backends' tactics need, space separated.
#   DISCHARGE_LEAN_PATH where they are built, prepended to LEAN_PATH.
#   DISCHARGE_LEAN_ARGS extra arguments for lean, such as a --plugin a backend needs.
set -euo pipefail
dir=$(cd "$1" && pwd)
limit=${2:-600}
here=$(cd "$(dirname "$0")" && pwd)
export LEAN_NUM_THREADS=${LEAN_NUM_THREADS:-1}
manifest="$dir/manifest.json"
(cd "$here" && lake build -KleanMemory=3000 -q SolarVerify </dev/null >&2)
lib="$here/.lake/build/lib/lean"
[[ -n ${DISCHARGE_LEAN_PATH:-} ]] && lib="$DISCHARGE_LEAN_PATH:$lib"
read -r -a leanargs <<<"${DISCHARGE_LEAN_ARGS:-}"
allowed="propext Classical.choice Quot.sound"
backends=${DISCHARGE_BACKENDS:-vc_valid=vc_valid}
out="$dir/discharge"
build="$out/build"
rm -rf "$out"
mkdir -p "$build"

fail() { echo "error: $*" >&2; exit 1; }
[[ $(jq -r .format "$manifest") == solar-verify-lean ]] || fail "not a solar-verify-lean manifest"
[[ $(jq -r .version "$manifest") == 1 ]] || fail "format version $(jq -r .version "$manifest"), this checker reads 1"
# The proofs must read the terms with the semantics they were written for.
for f in Smt Vc; do
  key=$(echo "$f" | tr 'A-Z' 'a-z')_sha256
  want=$(jq -r ".lean.$key // empty" "$manifest")
  have=$(sha256sum "$here/SolarVerify/$f.lean" | cut -d' ' -f1)
  [[ -n $want ]] || fail "manifest has no $key"
  [[ $want == "$have" ]] || fail "SolarVerify/$f.lean is not the one the export was written for"
done
# Nothing the proofs run may assume more than the kernel checks.
if grep -rnE '\b(sorry|admit|native_decide|bv_decide|ofReduceBool|ofReduceNat|implemented_by|extern|unsafe|axiom|decide \+native)\b' \
    "$here"/SolarVerify/Discharge.lean "$here"/SolarVerify/Discharge; then
  fail "the discharge library uses a construct the kernel does not check"
fi

# Every list is read whole before any lean runs: Lean's runtime changes the pipe a
# 'while read ... done < <(...)' loop reads from (it inherits the descriptor), and the loop then
# ends early, which once skipped every module after the first and still reported success.
mapfile -t modules < <(jq -r '.units[] | select(.module != null) | .module' "$manifest" | sort -u)
mapfile -t rows < <(jq -r '.units[] | select(.module != null) as $u | $u.obligations[] | select(.required) | [$u.module, .id, .lean, .role, .answer] | @tsv' "$manifest")
mapfile -t unexported < <(jq -r '.units[] | select(.module == null) | [.name, (.status // "not-exported")] | @tsv' "$manifest")
want=$(jq '[.units[] | .obligations[]? | select(.required)] | length' "$manifest")

# The inputs are checked whole before any proof: every module exists, no obligation or constant
# is listed twice, every required obligation is counted, and the inputs match the lock.
dups=$(jq -r '([.units[] | .obligations[]? | .id], [.units[] | .obligations[]? | select(.required) | .lean]) | group_by(.) | map(select(length > 1) | .[0]) | .[]' "$manifest")
[[ -z $dups ]] || fail "listed twice: $dups"
for module in "${modules[@]}"; do
  [[ -f $dir/$module ]] || fail "missing exported module $module"
done
(cd "$dir" && sha256sum manifest.json "${modules[@]}") >"$out/inputs.sha256"
if [[ -n ${DISCHARGE_LOCK:-} ]]; then
  diff <(sort -k2 "$DISCHARGE_LOCK") <(sort -k2 "$out/inputs.sha256") >&2 \
    || fail "the inputs are not the ones $DISCHARGE_LOCK pins"
fi
nunexp=$(jq '[.units[] | select(.module == null) | .obligations[]? | select(.required)] | length' "$manifest")
[[ $(( ${#rows[@]} + nunexp )) -eq $want ]] || fail "$want required obligations, ${#rows[@]} with a module"
echo "discharge: ${#modules[@]} modules, $want required obligations, ${#unexported[@]} units not exported" >&2

results="$out/results.tsv"
printf 'id\trole\tanswer\tstatus\tbackend\tseconds\tpeak_mb\taxioms\n' >"$results"
pass=0 bad=0
# Units that could not be exported cannot be proved.
for row in "${unexported[@]}"; do
  [[ -n $row ]] || continue
  IFS=$'\t' read -r name status <<<"$row"
  printf '%s\t-\t-\tfailed: unit not exported (%s)\t-\t0\t-\t-\n' "$name" "$status" >>"$results"
  bad=$((bad + 1))
done

# Runs a command and prints its wall time and peak memory as 'MEMTIME seconds megabytes'.
measure() {
  python3 -c '
import resource, subprocess, sys, time
t = time.time()
r = subprocess.run(sys.argv[1:], stdin=subprocess.DEVNULL)
mb = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss // 1024
sys.stderr.write("MEMTIME %.1f %d\n" % (time.time() - t, mb))
sys.exit(r.returncode)' "$@"
}

for module in "${modules[@]}"; do
  mod=${module%.lean}
  mkdir -p "$build/$(dirname "$mod")"
  (cd "$dir" && LEAN_PATH="$lib:$build" lean -R "$dir" -o "$build/$mod.olean" "$dir/$module" </dev/null >/dev/null) \
    || fail "could not compile the exported module $module"
done

IFS=';' read -r -a tactics <<<"$backends"
for row in "${rows[@]}"; do
  IFS=$'\t' read -r module id lean role answer <<<"$row"
  modname=${module%.lean}
  modname=${modname//\//.}
  status="failed: no backend" used=- secs=0 peak=- axioms=
  for entry in "${tactics[@]}"; do
    name=${entry%%=*}
    tactic=${entry#*=}
    probe="$out/$(echo "$id.$name" | tr -c 'A-Za-z0-9_\n' '_').lean"
    {
      echo "import SolarVerify"
      for m in ${DISCHARGE_IMPORTS:-}; do echo "import $m"; done
      echo "import $modname"
      echo
      echo "theorem obligation : $lean.Valid := by $tactic"
      echo
      echo "open Lean Elab Command in"
      echo "#eval show CommandElabM Unit from do"
      echo "  let axs ← Lean.collectAxioms (Name.mkSimple \"obligation\")"
      echo "  IO.println s!\"AXIOMS: {String.intercalate \" \" (axs.toList.map toString)}\""
    } >"$probe"
    set +e
    res=$(cd "$out" && LEAN_PATH="$lib:$build" measure timeout "$limit" lean "${leanargs[@]}" -DmaxHeartbeats=0 -DmaxRecDepth=100000 "$probe" 2>&1)
    code=$?
    set -e
    secs=$(printf '%s\n' "$res" | sed -n 's/^MEMTIME \([^ ]*\) .*/\1/p')
    peak=$(printf '%s\n' "$res" | sed -n 's/^MEMTIME [^ ]* //p')
    axioms=$(printf '%s\n' "$res" | sed -n 's/^AXIOMS: *//p')
    used=$name status=proved
    if [[ $code -eq 124 ]]; then
      status="failed: timeout after ${limit}s"
    elif [[ $code -ne 0 ]] || printf '%s\n' "$res" | grep -q 'error'; then
      status="failed: $(printf '%s\n' "$res" | grep -m1 -A2 'error' | tr '\t\n' '  ' | cut -c1-240)"
    elif [[ -z $axioms ]]; then
      status="failed: no axiom report"
    else
      for a in $axioms; do
        [[ " $allowed " == *" $a "* ]] || status="failed: uses axiom $a"
      done
    fi
    [[ $status == proved ]] && break
  done
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$role" "$answer" "$status" "$used" "${secs:-0}" "${peak:--}" "${axioms:-none}" >>"$results"
  if [[ $status == proved ]]; then pass=$((pass + 1)); else bad=$((bad + 1)); fi
  echo "$status  $id  ($used, ${secs:-0}s, ${peak:--} MB)" >&2
done

# A required obligation without a row was skipped: a failure.
if [[ $((pass + bad - ${#unexported[@]})) -ne ${#rows[@]} ]]; then
  echo "error: ${#rows[@]} exported required obligations, $((pass + bad - ${#unexported[@]})) checked" >&2
  bad=$((want - pass))
fi

echo "discharge: $pass of $want required obligations proved, $bad not ($results)" >&2
[[ $bad -eq 0 && $pass -eq $want ]]
