#!/usr/bin/env bash
O=/home/hand/h2/flagrun
cg=$(sed -n 's/^0:://p' /proc/self/cgroup); mm=$(cat /sys/fs/cgroup$cg/memory.max)
echo "inside scope: cgroup $cg memory.max $mm swap.max $(cat /sys/fs/cgroup$cg/memory.swap.max)" > $O/inner-cgroup.txt
case "$mm" in max|"") echo 3 > $O/EXIT; exit 3;; esac
while [ ! -f $O/go ]; do sleep 0.2; done
echo "$(date -u +%FT%TZ) driver start" >> $O/stages.log
env | LC_ALL=C sort > $O/driver-env.txt
/usr/bin/time -v -o $O/time-v.txt python3 /home/hand/h2/r5isrc/tools/certify/generic.py /home/hand/h2/Flag.sol natspec --contract Flag \
  --solidity-lean /home/hand/h2/sl --base 334d1d924f8f6c29ac827fd81d1cc739721b479a --work /home/hand/h2/flagwork \
  --record-dir $O/record --solc /home/hand/.cache/solc/solc-0.8.35 --packages-from /home/hand/h2/pkgrun --probe-cache /home/hand/h2/cache \
  > $O/driver.log 2>&1
echo $? > $O/EXIT
echo "$(date -u +%FT%TZ) driver exit $(cat $O/EXIT)" >> $O/stages.log
bash /home/hand/f5pack/post.sh >> $O/post.log 2>&1
echo $? > $O/POST_EXIT
cat /sys/fs/cgroup$cg/memory.peak > $O/memory.peak.inner; cat /sys/fs/cgroup$cg/memory.events > $O/memory.events.inner
