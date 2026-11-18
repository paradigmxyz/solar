#!/usr/bin/env bash
set -u
so=/home/hand/h2/flagrun/counter
cg=$(sed -n 's/^0:://p' /proc/self/cgroup); mm=$(cat /sys/fs/cgroup$cg/memory.max); sm=$(cat /sys/fs/cgroup$cg/memory.swap.max)
echo "inside scope: cgroup $cg memory.max $mm swap.max $sm" > $so/cgroup.txt
case "$mm" in max|"") echo 3 > $so/EXIT; exit 3;; esac; [ "$sm" = 0 ] || { echo 3 > $so/EXIT; exit 3; }
( cd /home/hand/h2/r5isrc && python3 tools/certify/generic.py /home/hand/f5pack/Counter.sol none --contract Counter --solidity-lean /home/hand/h2/sl --base f1c19a7f217cdcc1a80ddbf87e6146d83273468f --work /home/hand/h2/counterwork --record-dir /home/hand/h2/flagrun/counter/record --solc /home/hand/.cache/solc/solc-0.8.35 --packages-from /home/hand/h2/pkgrun --probe-cache /home/hand/h2/cache --loop-summary bump=Verify.LoopSource.Equiv.Counter.Summary:Verify.LoopSource.Equiv.CounterSummary.summary; echo generic-exit $?; python3 -c "import json; r=json.load(open('/home/hand/h2/flagrun/counter/record/record.json')); print(r.get('status'), json.dumps(r.get('loop_cuts'))[:2000], r.get('loop_cuts_blocked'))"; cp -r /home/hand/h2/counterwork/loops /home/hand/h2/flagrun/counter/ 2>/dev/null; cp -r /home/hand/h2/counterwork/solidity-lean/Certified/*/Loops /home/hand/h2/flagrun/counter/ 2>/dev/null; true ) > $so/out.log 2>&1
rc=$?; echo $rc > $so/EXIT
cat /sys/fs/cgroup$cg/memory.peak > $so/memory.peak; cat /sys/fs/cgroup$cg/memory.events > $so/memory.events
