#!/usr/bin/env bash
O=/home/hand/h2/flagrun
while [ ! -f $O/go ]; do sleep 0.2; done
echo "$(date -u +%FT%TZ) driver start" >> $O/stages.log
env | LC_ALL=C sort > $O/driver-env.txt
/usr/bin/time -v -o $O/time-v.txt python3 /home/hand/h2/r5isrc/tools/certify/generic.py /home/hand/h2/Flag.sol natspec --contract Flag \
  --solidity-lean /home/hand/h2/sl --base 334d1d924f8f6c29ac827fd81d1cc739721b479a --work /home/hand/h2/flagwork \
  --record-dir $O/record --solc /home/hand/.cache/solc/solc-0.8.35 --packages-from /home/hand/h2/pkgrun --probe-cache /home/hand/h2/cache \
  > $O/driver.log 2>&1
echo $? > $O/EXIT
echo "$(date -u +%FT%TZ) driver exit $(cat $O/EXIT)" >> $O/stages.log
bash /home/hand/r5jpack/post.sh >> $O/post.log 2>&1
echo $? > $O/POST_EXIT
