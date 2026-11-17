#!/usr/bin/env bash
set -u
export PATH=/home/hand/h2/py314:/home/hand/.cargo/bin:/opt/hand-tools/.cargo/bin:/home/hand/.elan/bin:/usr/local/bin:/usr/bin:/bin
O=/home/hand/h2/hookobl-b2; S=/home/hand/h2/hsrc; P=/home/hand/hopack; A=/home/hand/h2/accb2
BASE=7dfef8168be7304638e4af73ba8917d437defe6d
SUM=Verify.LoopSource.Equiv.Counter.Summary:Verify.LoopSource.Equiv.CounterSummary.summary
mkdir -p $A
(cd $S && python3 $P/full.py $O/cmd-b-mutsrc.jsonl -- $P/CounterMut.sol none --contract Counter --solidity-lean /home/hand/h2/sl --base $BASE \
  --work $A/w --record-dir $O/rec --solc /home/hand/.cache/solc/solc-0.8.35 --probe-cache /home/hand/h2/cache --loop-summary bump=$SUM) > $O/out.log 2>&1
echo "case $?" >> $O/exits.txt
N=countermut_022d6958
(cd $A/w/solidity-lean && /usr/bin/time -v -o $O/time-diag.txt lake build -KleanMemory=6000 Verify.LoopSource.Equiv.GenObl $N.gen.Counter.Stage0 $N.Loops.bump.Cut $N.Source Verify.LoopSource.Equiv.Counter.Summary) > $O/diag-lake.log 2>&1
echo "diag $?" >> $O/exits.txt
echo 0 > $O/EXIT
