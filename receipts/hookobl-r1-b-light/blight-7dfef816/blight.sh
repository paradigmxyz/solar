#!/usr/bin/env bash
# Light (b1)/(b2) at r1 ea124c89 on the hosted Hand: the r1 worktree builds its own solar and
# solar-verify (fresh target), the Obl build is refused by light-r1.py. Base 7dfef816.
set -u
A=/home/hand/h2/acc; H=/home/hand/h2/r1; BASE=$1; O=$A/blight-${BASE:0:8}
SUM=Verify.LoopSource.Equiv.Counter.Summary:Verify.LoopSource.Equiv.CounterSummary.summary
export SOLAR_VERIFY_Z3=/home/hand/h2/z3venv/bin/z3 LEAN_NUM_THREADS=1 PATH=$HOME/h2/py314:$HOME/.elan/bin:$PATH
unset CERTIFY_ALLOW_UNPINNED CERTIFY_ALLOW_FOREIGN_SOLAR
rm -rf $O; mkdir -p $O; [ "${FRESH:-0}" = 1 ] && rm -rf $H/target
(cd $H && cargo build --locked --bin solar --bin solar-verify) > $O/cargo-build.log 2>&1 || { echo "cargo failed"; exit 1; }
S=$H/target/debug/solar; $S --version > $O/producer-version.txt
emb=$(sed -n 's/^Commit SHA: //p' $O/producer-version.txt)
printf '{"binary_sha256": "%s", "binary_path": "%s", "embedded_commit": "%s", "driver_commit": "%s", "dirty": %s, "profile": "%s", "features": "%s", "build_command": "cargo build --locked --bin solar --bin solar-verify", "built_on": "hosted Hand, fresh target in the r1 worktree", "solar_verify_sha256": "%s", "rustc": "%s"}
'   "$(sha256sum < $S | cut -c1-64)" "$S" "$emb" "$(git -C $H rev-parse HEAD)" "$([ -z "$(git -C $H status --porcelain -- crates tools/certify Cargo.toml Cargo.lock)" ] && echo false || echo true)"   "$(sed -n 's/^Build Profile: //p' $O/producer-version.txt)" "$(sed -n 's/^Build Features: //p' $O/producer-version.txt)" "$(sha256sum < $H/target/debug/solar-verify | cut -c1-64)" "$(rustc --version)" > $O/producer.json
for c in "b-otherfn CounterFn.sol bump2" "b-mutsrc CounterMut.sol bump"; do set -- $c
  rm -rf $A/wb-$1
  (cd $H && ~/h2/py314/python3 $A/light-r1.py $O/cmd-$1.jsonl -- $A/$2 none --contract Counter --solidity-lean /home/hand/h2/sl --base $BASE     --work $A/wb-$1 --record-dir $O/rec-$1 --solc /home/hand/.cache/solc/solc-0.8.35 --probe-cache /home/hand/h2/cache --loop-summary $3=$SUM) > $O/out-$1.log 2>&1
  echo "$1 $?" >> $O/case-exits.txt
  mkdir -p $O/keep-$1; cp -r $A/wb-$1/loops $O/keep-$1/ 2>/dev/null; cp -r $A/wb-$1/solidity-lean/Certified/*/Loops $O/keep-$1/ 2>/dev/null; rm -rf $A/wb-$1
done
cp $A/Counter*.sol $A/light-r1.py $A/blight.sh $O/
echo "base $BASE" > $O/lean_base.txt; echo BLIGHT_DONE
