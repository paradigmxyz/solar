#!/usr/bin/env bash
# hookobl.sh EXPIRES_EPOCH: the loop cut hook at solar r1 (ea124c89) against solidity-lean 7dfef816
# on a boxctl box: cases (a) no summary, (b1) another function's summary, (b2) a mutated source
# with the old summary, (c) Counter with its own summary through the full Obl build, then
# leanchecker on the generated modules and #print axioms of Obl's theorems. The staging is r5j's
# (tools, apt git/time/libc6-dev, elan, small, cache-src, cache-lake-lite, Mathlib cache get); the
# producer and solar-verify are built in one 44G no-swap scope whose memory.max and swap.max are
# read back before any build. Cargo may fetch (CARGO_NET_OFFLINE unset) for solar-verify's crates.
set -u
expires=$1
H=/home/hand; P=$H/hopack; R=$H/r5jpack; O=$H/h2/hookobl; S=$H/h2/hsrc
R1=ea124c8915f454e588024b6d8296023a6950da32; BASE=7dfef8168be7304638e4af73ba8917d437defe6d
mkdir -p $O
log() { echo "$(date -u +%FT%TZ) $*" | tee -a $O/run.log; }
verdict() { echo "$1" > $O/VERDICT; log "VERDICT $1"; exit 0; }
left() { echo $(( expires - $(date +%s) )); }
step() { echo "$1" >> $O/STEPS; log "STEP $1"; }
log "hookobl start; expires $(date -u -d @$expires +%FT%TZ); ttl_left_s $(left); nproc $(nproc); mem_kb $(awk '/MemTotal/{print $2}' /proc/meminfo)"
(cd $R && sha256sum -c pack.sha256 > $O/r5jpack.verify 2>&1) || verdict "ABORTED-staging: an r5j pack file differs"
(cd $P && sha256sum -c pack.sha256 > $O/hopack.verify 2>&1) || verdict "ABORTED-staging: a hookobl pack file differs"
sudo tar --zstd -xpf $R/tools.tar.zst -C / && sudo chown -R $(id -u):$(id -g) /opt/hand-tools $H/.local || verdict "ABORTED-staging: tools"
for b in git time; do [ -x /usr/bin/$b ] || need="${need:-} $b"; done
[ -e /usr/lib/x86_64-linux-gnu/Scrt1.o ] || need="${need:-} libc6-dev"
[ -z "${need:-}" ] || sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q $need > $O/apt.log 2>&1 || verdict "ABORTED-staging: apt"
log "apt installed:${need:- none}"
cd $H
tar --zstd -xf $R/elan.tar.zst && tar --zstd -xf $R/small.tar.zst && tar --zstd -xf $R/cache-src.tar.zst &&
  (mkdir -p $H/h2/cache/.lake && cd $H/h2/cache/.lake && tar --zstd -xf $R/cache-lake-lite.tar.zst) || verdict "ABORTED-staging: unpack"
(cd $H/h2/cache && PATH=$H/.elan/bin:$PATH timeout 1200 lake exe cache get) > $O/mathlib-cache-get.log 2>&1 || verdict "ABORTED-staging: cache get"
/usr/bin/git -C $H/h2/sl fetch -q $P/sl.bundle hookobl-base:refs/heads/hookobl-base || verdict "ABORTED-staging: sl fetch"
[ "$(/usr/bin/git -C $H/h2/sl rev-parse hookobl-base)" = $BASE ] || verdict "ABORTED-staging: sl base"
rm -rf $S && /usr/bin/git init -q $S && /usr/bin/git -C $S fetch -q $P/src.bundle hookobl-r1:refs/heads/hookobl-r1 &&
  /usr/bin/git -C $S checkout -q hookobl-r1 || verdict "ABORTED-staging: solar checkout"
[ "$(/usr/bin/git -C $S rev-parse HEAD)" = $R1 ] || verdict "ABORTED-staging: solar HEAD"
mkdir -p $H/h2/py314 $H/h2/z3; ln -sf $H/.local/share/uv/python/cpython-3.14.8-linux-x86_64-gnu/bin/python3.14 $H/h2/py314/python3
cp $P/z3 $H/h2/z3/z3; chmod +x $H/h2/z3/z3; $H/h2/z3/z3 --version > $O/z3-version.txt 2>&1 || verdict "ABORTED-staging: z3"
step staged
cat > $O/inner.sh <<'EOF'
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
EOF
chmod +x $O/inner.sh
unset LC_ALL LANGUAGE
export HOME=$H RUSTUP_HOME=/opt/hand-tools/.rustup CARGO_HOME=$H/.cargo \
  PATH=$H/h2/py314:$H/.cargo/bin:/opt/hand-tools/.cargo/bin:$H/.elan/bin:/usr/local/bin:/usr/bin:/bin \
  LEAN_NUM_THREADS=1 LANG=C.UTF-8 SOLAR_VERIFY_Z3=$H/h2/z3/z3
env | grep -q -E "^(CERTIFY_ALLOW_|SOLAR=|CERTIFY_SKIP_BUILD=|CERTIFY_REPLAY_TRUST_UPSTREAM=)" && verdict "ABORTED-staging: a hatch variable is set"
[ -e $S/target ] && verdict "ABORTED-staging: build outputs present"
unit="hookobl-$(date +%s)"
sudo -E systemd-run --quiet --scope --unit=$unit -p MemoryMax=44G -p MemorySwapMax=0 --uid=$(id -u) --gid=$(id -g) \
  --setenv=HOME --setenv=PATH env RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME LEAN_NUM_THREADS=1 LANG=C.UTF-8 \
  SOLAR_VERIFY_Z3=$SOLAR_VERIFY_Z3 bash $O/inner.sh > $O/inner.out 2>&1 &
for i in $(seq 1 100); do cg=$(systemctl show -p ControlGroup --value $unit.scope 2>/dev/null); [ -n "$cg" ] && [ -e /sys/fs/cgroup$cg/memory.max ] && break; sleep 0.1; done
mm=$(cat /sys/fs/cgroup$cg/memory.max); sm=$(cat /sys/fs/cgroup$cg/memory.swap.max)
{ echo "unit $unit.scope"; echo "cgroup $cg"; echo "memory.max $mm"; echo "memory.swap.max $sm"; } > $O/cgroup.txt
log "scope $unit: memory.max $mm swap.max $sm"
case "$mm" in max|"") sudo systemctl kill $unit.scope; verdict "ABORTED-scope: memory.max is not finite";; esac
[ "$sm" = 0 ] || { sudo systemctl kill $unit.scope; verdict "ABORTED-scope: swap.max is $sm"; }
step scope
touch $O/go
peak() { cat /sys/fs/cgroup$cg/memory.peak > $O/memory.peak 2>/dev/null; cat /sys/fs/cgroup$cg/memory.events > $O/memory.events 2>/dev/null; }
while systemctl is-active --quiet $unit.scope; do
  peak
  if [ -f $O/fresh-start ] && [ ! -f $O/fresh-go ] && [ ! -f $O/fresh-skip ]; then
    touch $O/fresh-go; log "fresh check started with $(left) s left; it is stopped at TTL-15min and then recorded OPEN"
  fi
  if [ $(left) -lt 900 ]; then sudo systemctl kill $unit.scope; log "killed at TTL-15min"; break; fi
  sleep 5
done
peak
grep -q '^fresh ' $O/leanchecker.txt 2>/dev/null || echo "fresh OPEN (not finished before the TTL-15min stop)" >> $O/leanchecker.txt
sol() { sha256sum < "$1" | cut -c1-64; }
{ echo "{"
  echo "  \"lean_base\": {\"selected\": \"$BASE\", \"default_pin\": \"$(cat $S/tools/certify/solidity-lean.rev)\", \"selected_tree\": \"$(/usr/bin/git -C $H/h2/sl rev-parse $BASE^{tree})\"},"
  echo "  \"solar\": {\"r1\": \"$R1\", \"tree\": \"$(/usr/bin/git -C $S rev-parse HEAD^{tree})\", \"worktree\": \"$S (a fresh clone of the r1 bundle on this box; this runner is its only writer)\"},"
  echo "  \"sources\": {\"Counter.sol\": \"$(sol $P/Counter.sol)\", \"CounterFn.sol\": \"$(sol $P/CounterFn.sol)\", \"CounterMut.sol\": \"$(sol $P/CounterMut.sol)\"},"
  echo "  \"spec\": \"none (generic.py spec argument none; Expected not applicable to these BLOCKED/REJECTED runs)\","
  echo "  \"options\": \"$(sol $S/tools/certify/generic.py) generic.py; per-case options in rec-*/record.json\","
  echo "  \"solc\": \"$(sol $H/.cache/solc/solc-0.8.35)\","
  echo "  \"z3\": \"$(sol $H/h2/z3/z3) $(head -1 $O/z3-version.txt)\","
  echo "  \"producer\": $(cat $O/producer.json 2>/dev/null || echo null),"
  echo "  \"edge_open\": \"the published statement does not apply loop_equiv yet (loop_cuts.py); an Obl build is not final equivalence, termination, property or resources\","
  echo "  \"resources\": \"OPEN\", \"lifecycle\": \"OPEN\""
  echo "}"; } > $O/manifest.json
log "scope ended; EXIT $(cat $O/EXIT 2>/dev/null)"
step done
verdict "FINISHED $(cat $O/EXIT 2>/dev/null)"
