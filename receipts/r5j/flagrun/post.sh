#!/usr/bin/env bash
# post.sh: r5i's steps after the driver: #print axioms of final, negative_any.sh, the closure replay
# tests and the binding manifest, each only if its budget fits before TTL-10 min.
O=/home/hand/h2/flagrun; L=/home/hand/h2/flagwork/solidity-lean; N=flag_4ce5cf9a
T=/home/hand/.elan/toolchains/leanprover--lean4---v4.35.0
stage() { echo "$(date -u +%FT%TZ) $*" >> $O/stages.log; }
fits() { [ $(( R4_EXPIRES - $(date +%s) - 600 )) -ge $1 ]; }
[ "$(cat $O/EXIT)" = 0 ] || { stage "post skipped: driver exit $(cat $O/EXIT)"; exit 1; }
rc=0
# The per-run package prebuild log, on its own, and the inputs commit and options manifest verbatim.
cp $L/.lake/certify-packages-$N.log $O/prebuild-packages.log 2>/dev/null || stage "no prebuild_packages log"
stage "prebuild_packages log: $(grep -c . $O/prebuild-packages.log 2>/dev/null) lines; $(grep -E 'Build completed|error' $O/prebuild-packages.log 2>/dev/null | tr '\n' ' ' | cut -c1-300)"
{ echo "# r5i inputs commit (git cat-file in $L)"; echo "HEAD $(git -C $L rev-parse HEAD)"; echo "HEAD^{tree} $(git -C $L rev-parse HEAD^{tree})"
  git -C $L cat-file -p HEAD; } > $O/inputs-commit.txt
# Cold start, after the run: each stripped module's olean in the run's package tree, and whether
# the per-run prebuild log (outside the sandbox) is where it was built.
{ echo "# after the run: stripped modules (absent at start) in $L/.lake/packages"
  for m in $(cat $O/strip.txt); do f=$L/.lake/packages/EthLib/.lake/build/lib/lean/$(echo $m | tr . /).olean
    if [ -f "$f" ]; then st=present; else st=ABSENT; fi
    grep -q "$m" $O/prebuild-packages.log 2>/dev/null && b="built by prebuild_packages" || b="not in prebuild log"
    echo "$m $st; $b"; done; } >> $O/coldstart.txt
cp $L/Certified/$N/gen/Flag/manifest.json $O/manifest.json 2>/dev/null || stage "no options manifest.json"
cp -r $L/Certified/$N $O/gen-$N 2>/dev/null
thm=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["theorem"])' $L/.lake/certify-$N.json.replay)
stage "replay receipt $(sha256sum < $L/.lake/certify-$N.json.replay | cut -c1-64) theorem $thm"
cp $L/.lake/certify-$N.json.replay $O/replay-receipt.json; cp $L/.lake/certify-$N.closure.* $O/ 2>/dev/null
if fits 900; then
  printf 'import %s.Final\n#print axioms %s\n' "$N" "$thm" > $O/Axioms.lean
  (cd $L && timeout 900 lake env $T/bin/lean -M 6000 $O/Axioms.lean) > $O/axioms.out 2>&1; a=$?
  stage "axioms exit $a: $(tr '\n' ' ' < $O/axioms.out | cut -c1-400)"
  python3 - $O/axioms.out <<'PY' || { stage "axioms NOT within the whitelist"; rc=1; }
import re, sys
t = open(sys.argv[1]).read()
m = re.search(r"depends on axioms: \[([^\]]*)\]", t)
assert m, "no axioms line"
ax = {x.strip() for x in m.group(1).split(",") if x.strip()}
assert ax <= {"propext", "Classical.choice", "Quot.sound"}, ax
assert "sorry" not in t
print(sorted(ax))
PY
  [ $a = 0 ] || rc=1
else stage "axioms skipped: no budget"; rc=1; fi
# Negatives on Flag's own closure: a tampered Init lemma and an omitted dependency must be refused.
stmt=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["statement"])' $O/replay-receipt.json)
for neg in "--tamper Nat.add_zero Nat.le_refl" "--omit Nat.add_zero"; do
  tag=$(echo $neg | awk '{print substr($1,3)}')
  if fits 900; then
    (cd $L && timeout 900 lake env $T/bin/lean -M 6000 --run /home/hand/h2/r5isrc/tools/certify/replay_closure.lean $neg $N.Final $thm $stmt $O/neg-$tag) > $O/neg-$tag.out 2>&1; a=$?
    if [ $a != 0 ] && grep -q 'the kernel rejects the closure' $O/neg-$tag.out; then stage "flag negative $neg: refused (exit $a)"
    else stage "flag negative $neg NOT refused (exit $a)"; rc=1; fi
  else stage "flag negative $neg skipped: no budget"; rc=1; fi
done
if fits 1800; then
  (cd $L && LEAN_MEMORY=6000 timeout 1800 bash /home/hand/h2/r5isrc/tools/certify/negative_any.sh $L Certified $N) > $O/negative_any.out 2>&1; a=$?
  stage "negative_any exit $a: $(tail -1 $O/negative_any.out)"; [ $a = 0 ] || rc=1
else stage "negative_any skipped: no budget"; rc=1; fi
if fits 600; then
  (PATH=$T/bin:$PATH timeout 600 bash /home/hand/h2/r5isrc/tools/certify/replay_closure_test.sh) > $O/replay_closure_test.out 2>&1; a=$?
  stage "replay_closure_test exit $a: $(tail -1 $O/replay_closure_test.out)"; [ $a = 0 ] || rc=1
else stage "replay_closure_test skipped: no budget"; rc=1; fi
D=$L/Certified/$N
(cd $D && find . -type f | LC_ALL=C sort | xargs sha256sum) > $O/gentree.sha256
python3 - $O $D <<'PY'
import hashlib, json, sys, pathlib
O, D = map(pathlib.Path, sys.argv[1:])
h = lambda p: hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None
g = D / "gen/Flag"
b = {"runtime_bytecode": h(D / "bytecode.hex"),
     "stages": {n: h(g / n) for n in ["Bytecode.lean", "Stage0.lean", "Sched.lean", "Schedule.json"] + [f"Evm{i}.lean" for i in range(1, 6)]},
     "source": h(pathlib.Path("/home/hand/h2/Flag.sol")), "spec": h(g / "Spec.lean"),
     "manifest_lean": h(g / "Manifest.lean"), "options_manifest_json": h(g / "manifest.json"),
     "compiler": h(pathlib.Path("/home/hand/h2/r5isrc/target-r5i/debug/solar")),
     "replay_receipt": h(O / "replay-receipt.json"), "axioms_out": h(O / "axioms.out"),
     "negative_any_out": h(O / "negative_any.out"), "neg_tamper_out": h(O / "neg-tamper.out"), "neg_omit_out": h(O / "neg-omit.out"),
     "toolchain_computed_files": h(O / "toolchain/toolchain.computed.files"), "toolchain_env": h(O / "toolchain/toolchain.env"), "replay_closure_test_out": h(O / "replay_closure_test.out"),
     "driver_log": h(O / "driver.log"), "certify_record": h(O / "record/certify-record.json"),
     "gentree_manifest": h(O / "gentree.sha256")}
rec = json.load(open(O / "record/record.json"))
sl = rec.get("solidity_lean") or {}
b["solidity_lean"] = {k: sl.get(k) for k in ("base", "inputs_commit", "inputs_tree", "provenance")}
b["certify_env"] = (rec.get("scripts_certify") or {}).get("env")
b["caveats"] = rec.get("caveats")
b["hatch_free"] = "CERTIFY_ALLOW_UNPINNED" not in (b["certify_env"] or {}) and not any(
    "not the pinned" in str(c) for c in (rec.get("caveats") or []))
(O / "bind.json").write_text(json.dumps(b, indent=2) + "\n")
PY
stage "bind $(sha256sum < $O/bind.json | cut -c1-64)"
exit $rc
