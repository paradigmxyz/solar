#!/usr/bin/env bash
# Proves the obligations `solar-verify --emit-lean DIR` exported from cvc5 proofs reconstructed in
# Lean (`vc_smt`, see SolarVerifyReconstruct/Tactic.lean). Like ../discharge.sh, a proof counts only
# when the kernel accepts it and it uses no axiom but propext, Classical.choice and Quot.sound: a
# step lean-smt cannot reconstruct is a `sorryAx` and fails it. Writes DIR/reconstruct/results.tsv;
# exits 0 only when every required obligation is proved.
#
#   bash crates/verify/lean/reconstruct/reconstruct.sh DIR [TIMEOUT_SECONDS] [ID_REGEX]
set -euo pipefail
dir=$(cd "$1" && pwd)
limit=${2:-600}
only=${3:-.}
here=$(cd "$(dirname "$0")" && pwd)
export LEAN_NUM_THREADS=${LEAN_NUM_THREADS:-1}
manifest="$dir/manifest.json"
(cd "$here" && lake build -KleanMemory=3000 -q >&2)
lpath=$(cd "$here" && lake env printenv LEAN_PATH)
plugin="$here/.lake/packages/cvc5/.lake/build/lib/libcvc5_cvc5.so"
allowed="propext Classical.choice Quot.sound"
out="$dir/reconstruct"
build="$out/build"
mkdir -p "$build"

fail() { echo "error: $*" >&2; exit 1; }
[[ $(jq -r .format "$manifest") == solar-verify-lean ]] || fail "not a solar-verify-lean manifest"
[[ $(jq -r .version "$manifest") == 1 ]] || fail "format version $(jq -r .version "$manifest"), this checker reads 1"
for f in Smt Vc; do
  key=$(echo "$f" | tr 'A-Z' 'a-z')_sha256
  want=$(jq -r ".lean.$key // empty" "$manifest")
  have=$(sha256sum "$here/../SolarVerify/$f.lean" | cut -d' ' -f1)
  [[ $want == "$have" ]] || fail "SolarVerify/$f.lean is not the one the export was written for"
done
if grep -rnE '\b(sorry|admit|native_decide|bv_decide|ofReduceBool|ofReduceNat|implemented_by|extern|unsafe|axiom|decide \+native)\b' \
    "$here"/SolarVerifyReconstruct.lean "$here"/SolarVerifyReconstruct; then
  fail "the reconstruction library uses a construct the kernel does not check"
fi

results="$out/results.tsv"
printf 'id\trole\tanswer\tstatus\tseconds\taxioms\n' >"$results"
pass=0 bad=0
while IFS=$'\t' read -r name status; do
  printf '%s\t-\t-\tfailed: unit not exported (%s)\t0\t-\n' "$name" "$status" >>"$results"
  bad=$((bad + 1))
done < <(jq -r '.units[] | select(.status == "not-exported" or .status == "unsupported") | [.name, .status] | @tsv' "$manifest")

mapfile -t modules < <(jq -r '.units[] | select(.module != null) | .module' "$manifest" | sort -u)
for module in "${modules[@]}"; do
  mod=${module%.lean}
  modname=${mod//\//.}
  mkdir -p "$build/$(dirname "$mod")"
  (cd "$dir" && LEAN_PATH="$lpath:$build" lean -R "$dir" -o "$build/$mod.olean" "$dir/$module" >/dev/null) \
    || fail "could not compile the exported module $module"
  mapfile -t obls < <(jq -r --arg m "$module" '.units[] | select(.module == $m) | .obligations[] | select(.required) | [.id, .lean, .role, .answer] | @tsv' "$manifest")
  for line in "${obls[@]}"; do
    IFS=$'\t' read -r id lean role answer <<<"$line"
    [[ $id =~ $only ]] || continue
    probe="$out/$(echo "$id" | tr -c 'A-Za-z0-9_\n' '_').lean"
    {
      echo "import SolarVerifyReconstruct"
      echo "import $modname"
      echo
      echo "theorem obligation : $lean.Valid := by vc_smt"
      echo
      echo "open Lean Elab Command in"
      echo "#eval show CommandElabM Unit from do"
      echo "  let axs ← Lean.collectAxioms (Name.mkSimple \"obligation\")"
      echo "  IO.println s!\"AXIOMS: {String.intercalate \" \" (axs.toList.map toString)}\""
    } >"$probe"
    start=$(date +%s%N)
    set +e
    res=$(cd "$out" && LEAN_PATH="$lpath:$build" timeout "$limit" lean --plugin="$plugin" -DmaxHeartbeats=0 -DmaxRecDepth=100000 "$probe" 2>&1 </dev/null)
    code=$?
    set -e
    ms=$(( ($(date +%s%N) - start) / 1000000 ))
    secs="$((ms / 1000)).$(( (ms % 1000) / 100 ))"
    axioms=$(printf '%s\n' "$res" | sed -n 's/^AXIOMS: *//p')
    status=proved
    if [[ $code -eq 124 ]]; then
      status="failed: timeout after ${limit}s"
    elif [[ $code -ne 0 ]] || printf '%s\n' "$res" | grep -q 'error'; then
      status="failed: $(printf '%s\n' "$res" | grep -m1 -A2 'error' | tr '\t\n' '  ' | cut -c1-240)"
    else
      for a in $axioms; do
        [[ " $allowed " == *" $a "* ]] || status="failed: uses axiom $a"
      done
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$role" "$answer" "$status" "$secs" "${axioms:-none}" >>"$results"
    if [[ $status == proved ]]; then pass=$((pass + 1)); else bad=$((bad + 1)); fi
    echo "$status  $id  (${secs}s)" >&2
  done
done

echo "reconstruct: $pass required obligations proved, $bad not ($results)" >&2
[[ $bad -eq 0 ]]
