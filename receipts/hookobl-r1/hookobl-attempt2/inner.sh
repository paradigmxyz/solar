#!/usr/bin/env bash
set -u
# sudo resets PATH; the scope gets the runner's own.
export PATH=/home/hand/h2/py314:/home/hand/.cargo/bin:/opt/hand-tools/.cargo/bin:/home/hand/.elan/bin:/usr/local/bin:/usr/bin:/bin
O=/home/hand/h2/hookobl; S=/home/hand/h2/hsrc; P=/home/hand/hopack; A=/home/hand/h2/acc; R1=ea124c8915f454e588024b6d8296023a6950da32
BASE=7dfef8168be7304638e4af73ba8917d437defe6d
SUM=Verify.LoopSource.Equiv.Counter.Summary:Verify.LoopSource.Equiv.CounterSummary.summary
while [ ! -f $O/go ]; do sleep 0.2; done
lg() { echo "$(date -u +%FT%TZ) $*" >> $O/inner.log; }
mkdir -p $A
(cd $S && cargo build --locked --bin solar --bin solar-verify -v) > $O/cargo-build.log 2>&1 || { lg "cargo build failed"; echo 1 > $O/EXIT; exit 1; }
bin=$S/target/debug/solar; $bin --version > $O/producer-version.txt 2>&1
emb=$(sed -n 's/^Commit SHA: //p' $O/producer-version.txt)
printf '{\n  "binary_sha256": "%s",\n  "binary_path": "%s",\n  "embedded_commit": "%s",\n  "driver_commit": "%s",\n  "dirty": %s,\n  "profile": "%s",\n  "features": "%s",\n  "build_command": "cargo build --locked --bin solar --bin solar-verify -v",\n  "built_on": "this box, fresh target",\n  "solar_verify_sha256": "%s",\n  "rustc": "%s"\n}\n' \
  "$(sha256sum < $bin | cut -c1-64)" "$bin" "$emb" "$(git -C $S rev-parse HEAD)" "$([ -z "$(git -C $S status --porcelain -- crates tools/certify Cargo.toml Cargo.lock)" ] && echo false || echo true)" \
  "$(sed -n 's/^Build Profile: //p' $O/producer-version.txt)" "$(sed -n 's/^Build Features: //p' $O/producer-version.txt)" \
  "$(sha256sum < $S/target/debug/solar-verify | cut -c1-64)" "$(rustc --version)" > $O/producer.json
lg "producer $(cat $O/producer.json | tr -d '\n ')"
[ "$emb" = $R1 ] || { lg "embedded $emb is not $R1"; echo 2 > $O/EXIT; exit 2; }
echo producer >> $O/STEPS
one() { local l=$1 src=$2; shift 2; lg "case $l start"
  rm -rf $A/w-$l
  (cd $S && /usr/bin/time -v -o $O/time-$l.txt python3 $P/full.py $O/cmd-$l.jsonl -- $src none --contract Counter \
    --solidity-lean /home/hand/h2/sl --base $BASE --work $A/w-$l --record-dir $O/rec-$l \
    --solc /home/hand/.cache/solc/solc-0.8.35 --probe-cache /home/hand/h2/cache "$@") > $O/out-$l.log 2>&1
  local rc=$?; echo "$l $rc" >> $O/case-exits.txt; lg "case $l exit $rc"
  mkdir -p $O/keep-$l; cp -r $A/w-$l/loops $O/keep-$l/ 2>/dev/null
  cp -r $A/w-$l/solidity-lean/Certified/*/Loops $O/keep-$l/ 2>/dev/null; true
  echo "case-$l" >> $O/STEPS
  [ "$l" = c-own ] || rm -rf $A/w-$l; }
one a-nosummary $P/Counter.sol
one b-otherfn $P/CounterFn.sol --loop-summary bump2=$SUM
one b-mutsrc $P/CounterMut.sol --loop-summary bump=$SUM
one c-own $P/Counter.sol --loop-summary bump=$SUM
W=$A/w-c-own/solidity-lean; N=counter_f1c87a33
if [ -f $W/.lake/build/lib/lean/$N/Loops/bump/Obl.olean ]; then
  cat > $W/Certified/$N/Loops/bump/Ax.lean <<AX
import $N.Loops.bump.Obl
#print axioms Certified.$N.Loops.bump.leaves
#print axioms Certified.$N.Loops.bump.tree
#print axioms Certified.$N.Loops.bump.seqs
#print axioms Certified.$N.Loops.bump.valid
#print axioms Certified.$N.Loops.bump.obl
AX
  (cd $W && lake env lean Certified/$N/Loops/bump/Ax.lean) > $O/axioms.txt 2>&1; lg "axioms exit $?"
  cp $W/Certified/$N/Loops/bump/Ax.lean $O/keep-c-own/
  echo axioms >> $O/STEPS
  mods=$(cd $W/Certified && ls $N/Loops/bump/*.lean | grep -v -E '/(Gen|Ax|Cut)\.lean$' | sed 's/\.lean$//; s|/|.|g')
  for m in $mods; do /usr/bin/time -v -o $O/time-check-$m.txt bash -c "cd $W && lake env leanchecker $m" > $O/check-$m.log 2>&1
    echo "$m $?" >> $O/leanchecker.txt; done
  lg "leanchecker per module: $(tr '\n' ' ' < $O/leanchecker.txt)"
  echo leanchecker >> $O/STEPS
  touch $O/fresh-start
  while [ ! -f $O/fresh-go ] && [ ! -f $O/fresh-skip ]; do sleep 1; done
  if [ -f $O/fresh-go ]; then
    lg "leanchecker --fresh Obl start"
    /usr/bin/time -v -o $O/time-fresh.txt bash -c "cd $W && lake env leanchecker --fresh $N.Loops.bump.Obl" > $O/check-fresh.log 2>&1
    echo "fresh $?" >> $O/leanchecker.txt; lg "leanchecker --fresh exit $(tail -1 $O/leanchecker.txt)"
  fi
fi
echo 0 > $O/EXIT
