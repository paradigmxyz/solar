#!/usr/bin/env bash
set -u
export PATH=/home/hand/h2/py314:/home/hand/.cargo/bin:/opt/hand-tools/.cargo/bin:/home/hand/.elan/bin:/usr/local/bin:/usr/bin:/bin
O=/home/hand/h2/hookobl-b; S=/home/hand/h2/hsrc; P=/home/hand/hopack; A=/home/hand/h2/accb
BASE=7dfef8168be7304638e4af73ba8917d437defe6d
SUM=Verify.LoopSource.Equiv.Counter.Summary:Verify.LoopSource.Equiv.CounterSummary.summary
mkdir -p $A
one() { local l=$1 src=$2; shift 2; echo "$(date -u +%FT%TZ) case $l start" >> $O/inner.log
  (cd $S && python3 $P/full.py $O/cmd-$l.jsonl -- $src none --contract Counter --solidity-lean /home/hand/h2/sl --base $BASE \
    --work $A/w-$l --record-dir $O/rec-$l --solc /home/hand/.cache/solc/solc-0.8.35 --probe-cache /home/hand/h2/cache "$@") > $O/out-$l.log 2>&1
  echo "$l $?" >> $O/case-exits.txt
  grep -n -B2 -A6 'exited with code\|error:' $A/w-$l/lake-*.log 2>/dev/null | head -40 >> $O/diag-$l.txt
  mkdir -p $O/keep-$l; cp -r $A/w-$l/loops $O/keep-$l/ 2>/dev/null; cp -r $A/w-$l/solidity-lean/Certified/*/Loops $O/keep-$l/ 2>/dev/null
  rm -rf $A/w-$l; }
one b-otherfn $P/CounterFn.sol --loop-summary bump2=$SUM
one b-mutsrc $P/CounterMut.sol --loop-summary bump=$SUM
echo 0 > $O/EXIT
