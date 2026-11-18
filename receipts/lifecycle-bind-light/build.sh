#!/usr/bin/env bash
set -u
L=/home/hand/h2/lbbuild; cd $L/sl
export PATH=$HOME/h2/py314:$HOME/.elan/bin:$PATH LEAN_NUM_THREADS=1
echo "base $(git rev-parse HEAD)" > $L/summary
peak() { local m=0 v; while kill -0 $1 2>/dev/null; do v=$(ps -o rss= --ppid $1 -o rss= 2>/dev/null | sort -n | tail -1); v=${v:-0}; [ ${v// /} -gt $m ] 2>/dev/null && m=${v// /}; for p in $(pgrep -f 'lean ' ); do r=$(awk '/VmHWM/{print $2}' /proc/$p/status 2>/dev/null); [ -n "$r" ] && [ $r -gt $m ] && m=$r; done; sleep 2; done; echo $m; }
for m in Lifecycle SourceSame; do
  s=$(date +%s); timeout 4800 lake build -KleanMemory=6000 counter_f1c87a33.$m > $L/build-$m.log 2>&1 & bp=$!
  pk=$(peak $bp); wait $bp; rc=$?
  echo "$m rc=$rc max_lean_rss_kb=$pk elapsed_s=$(( $(date +%s) - s ))" >> $L/summary
done
printf 'import counter_f1c87a33.Lifecycle\n#print axioms Certified.counter_f1c87a33.Lifecycle.statement\n#print axioms Certified.counter_f1c87a33.Lifecycle.code_same\n#print axioms Certified.counter_f1c87a33.Lifecycle.init_same\n#print axioms Certified.counter_f1c87a33.Lifecycle.contract_same\n#print axioms Certified.counter_f1c87a33.Lifecycle.fuel_same\n' > $L/Ax.lean
lake env lean -M 6000 $L/Ax.lean > $L/axioms.txt 2>&1; echo "axioms rc=$?" >> $L/summary
echo DONE >> $L/summary
