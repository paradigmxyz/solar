#!/usr/bin/env bash
set -u
so=/home/hand/h2/flagrun/bind
cg=$(sed -n 's/^0:://p' /proc/self/cgroup); mm=$(cat /sys/fs/cgroup$cg/memory.max); sm=$(cat /sys/fs/cgroup$cg/memory.swap.max)
echo "inside scope: cgroup $cg memory.max $mm swap.max $sm" > $so/cgroup.txt
case "$mm" in max|"") echo 3 > $so/EXIT; exit 3;; esac; [ "$sm" = 0 ] || { echo 3 > $so/EXIT; exit 3; }
( python3 /home/hand/h2/r5isrc/tools/certify/bind.py --record /home/hand/h2/flagrun/record/record.json --dir /home/hand/h2/flagwork/solidity-lean/Certified/flag_4ce5cf9a --replay /home/hand/h2/flagrun/replay-receipt.json --solar /home/hand/h2/r5isrc/target-r5i/debug/solar --driver-root /home/hand/h2/r5isrc --out /home/hand/h2/flagrun/bind/bind-checked.json ) > $so/out.log 2>&1
rc=$?; echo $rc > $so/EXIT
cat /sys/fs/cgroup$cg/memory.peak > $so/memory.peak; cat /sys/fs/cgroup$cg/memory.events > $so/memory.events
