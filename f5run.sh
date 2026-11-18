#!/usr/bin/env bash
# f5run.sh EXPIRES_EPOCH CREATED_EPOCH: the freeze5 candidate box run, derived from avrun.sh (the
# accepted all-value runner, 65977680). Solar is 085fbaa8 (alpharush/t7-freeze5-cand) staged in
# ~/f5pack, its producer built here from that commit (embedded == driver, checked in full). Step 1 is
# avrun's Flag run unchanged in its terms (base 334d1d92, cold start with --packages-from, shards 1,
# LEAN_NUM_THREADS 1, 44G no-swap scope with fail-closed read-back, replay gate, post.sh negatives),
# now with the manifest-bound expected statement. Then, each in its own 44G no-swap scope whose
# memory.max the scope's own process reads from /proc/self/cgroup and refuses when not finite, with
# memory.peak and memory.events written from inside before it exits:
#   2 bind     bind.py on step 1's run (bind-checked.json; four-module replay accounting)
#   3 open     the Resources-premise route on Flag: final.py --manifest-json with open_premises
#              ["resources"] and kind functional, the proof Shapes.Certified.functional of step 1's,
#              written over step 1's Statement/Final in its work tree and built (lake build N.Final),
#              #print axioms, record not strongest
#   4 counter  the loop cut hook on Counter at base f1c19a7f with its own summary, through the full
#              Obl build (expected verdict REJECTED from loop_resources; loop_cuts.bump recorded)
#   5 erc20    only when ~/f5pack/erc20/ is staged (source, contract, args); otherwise OPEN
# Every step stops at a 1h50 gate from CREATED_EPOCH (and at TTL-10); a step starts only if its budget
# fits before the gate. Each step's end is marked in STEPS for copy-off.
set -u
expires=$1; created=$2; gate=$(( created + 6600 ))
H=/home/hand; P=$H/f5pack; O=$H/h2/flagrun; S=$H/h2/r5isrc
HEADC=085fbaa853a32f1a9a4cab77cb8d892fd2eb6e2f; BASE=334d1d924f8f6c29ac827fd81d1cc739721b479a
mkdir -p $O
log() { echo "$(date -u +%FT%TZ) $*" | tee -a $O/r4.log; }
verdict() { echo "$1" > $O/VERDICT; log "VERDICT $1"; exit 0; }
left() { echo $(( expires - $(date +%s) )); }
log "f5run start; gate $(date -u -d @$gate +%FT%TZ); expires $(date -u -d @$expires +%FT%TZ); ttl_left_s $(left); nproc $(nproc); mem_kb $(awk '/MemTotal/{print $2}' /proc/meminfo)"
log "locale: $(locale 2>/dev/null | tr '\n' ' ')"
cd $P && sha256sum -c pack.sha256 > $O/pack.verify 2>&1 || { cat $O/pack.verify >> $O/r4.log; verdict "ABORTED-staging: a pack file's sha256 differs"; }
log "pack verified: $(sha256sum pack.sha256)"
sudo tar --zstd -xpf $P/tools.tar.zst -C / && sudo chown -R $(id -u):$(id -g) /opt/hand-tools $H/.local || verdict "ABORTED-staging: tools unpack failed"
# The image lacks git and GNU time: installed from the distribution, versions recorded.
for b in git time; do [ -x /usr/bin/$b ] || need="${need:-} $b"; done
[ -e /usr/lib/x86_64-linux-gnu/Scrt1.o ] || need="${need:-} libc6-dev"
[ -z "${need:-}" ] || sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q $need > $O/apt.log 2>&1 || verdict "ABORTED-staging: apt install$need failed"
{ /usr/bin/git --version; dpkg-query -W -f='${Package} ${Version}\n' git time libc6-dev 2>/dev/null; } > $O/apt-versions.txt
log "apt installed:${need:- none}; $(/usr/bin/git --version); $(dpkg-query -W -f='time ${Version}' time 2>/dev/null)"
cd $H
tar --zstd -xf $P/elan.tar.zst && tar --zstd -xf $P/small.tar.zst && tar --zstd -xf $P/cache-src.tar.zst &&
  tar --zstd -xf $P/src.tar.zst && (mkdir -p $H/h2/cache/.lake && cd $H/h2/cache/.lake && tar --zstd -xf $P/cache-lake-lite.tar.zst) || verdict "ABORTED-staging: unpack failed"
cp $P/Flag.sol $H/h2/Flag.sol
# Mathlib's oleans are not staged: they come from its build cache, fetched here at the pinned revision.
[ -d $H/h2/cache/.lake/packages/mathlib/.lake/build ] && verdict "ABORTED-staging: Mathlib build outputs were staged"
(cd $H/h2/cache && PATH=$H/.elan/bin:$PATH timeout 1200 lake exe cache get) > $O/mathlib-cache-get.log 2>&1 || verdict "ABORTED-staging: lake exe cache get failed"
log "mathlib: lake exe cache get exit 0; $(tail -n 2 $O/mathlib-cache-get.log | tr '\n' ' ' | cut -c1-200)"
# Preflight: every external command this runner, inner.sh, scripts/certify, tools/certify, post.sh
# and the lake shim invoke must resolve, on the PATH the driver uses, before anything else runs.
# A missing one stops the run here, before the overlay and admission; nothing is installed after.
PF=$H/h2/shim:$H/h2/py314:$H/.cargo/bin:/opt/hand-tools/.cargo/bin:$H/.elan/bin:/usr/local/bin:/usr/bin:/bin
miss=""; : > $O/preflight.txt
for c in /usr/bin/git git /usr/bin/time cc ld rust-lld cargo rustc elan lake lean sha256sum tar zstd \
  bash env sudo systemd-run systemctl pgrep awk sed grep find sort xargs cut tr head tail wc cmp diff od \
  cat cp mv rm ln mkdir mktemp touch chmod readlink realpath dirname basename date printf timeout unshare \
  kill sleep seq id tee uniq printenv ps; do
  w=$(PATH=$PF command -v "$c" 2>/dev/null)
  [ -z "$w" ] && [ "$c" = rust-lld ] && w=$(ls /opt/hand-tools/.rustup/toolchains/*/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld 2>/dev/null | head -1)
  [ -z "$w" ] && [ "$c" = lean ] && w=$(ls $H/.elan/toolchains/*/bin/lean 2>/dev/null | head -1)
  [ -z "$w" ] && [ "$c" = lake ] && w=$(ls $H/.elan/toolchains/*/bin/lake 2>/dev/null | head -1)
  [ -n "$w" ] || w=MISSING
  echo "$c $w" >> $O/preflight.txt; [ "$w" = MISSING ] && miss="$miss $c"
done
for f in $H/.cache/solc/solc-0.8.35 $H/.local/share/uv/python/cpython-3.14.8-linux-x86_64-gnu/bin/python3.14 \
  /usr/lib/x86_64-linux-gnu/Scrt1.o /usr/lib/x86_64-linux-gnu/crti.o /usr/lib/x86_64-linux-gnu/libc.so; do
  [ -e "$f" ] && echo "$f ok" >> $O/preflight.txt || { echo "$f MISSING" >> $O/preflight.txt; miss="$miss $f"; }
done
[ -z "$miss" ] || verdict "ABORTED-preflight: missing$miss"
log "preflight ok: $(grep -c . $O/preflight.txt) commands and files resolved; sha256 $(sha256sum < $O/preflight.txt | cut -c1-16)"
# The solar checkout's git: a standalone repository from the bundle, HEAD at the v2 commit, index
# refreshed without touching the working tree (its mtimes keep the staged build fresh).
(cd $S && rm -rf .git && /usr/bin/git init -q && /usr/bin/git fetch -q $P/src.bundle f5cand:refs/heads/f5cand &&
  /usr/bin/git symbolic-ref HEAD refs/heads/f5cand && /usr/bin/git reset -q) || verdict "ABORTED-staging: solar git restore failed"
[ "$(/usr/bin/git -C $S rev-parse HEAD)" = $HEADC ] || verdict "ABORTED-staging: solar HEAD is not $HEADC"
[ "$(/usr/bin/git -C $S rev-parse HEAD^{tree})" = b83000e2d90a33f029feb0292453783effbe83cf ] || verdict "ABORTED-staging: solar tree is not b83000e2d90a33f029feb0292453783effbe83cf"
[ -z "$(/usr/bin/git -C $S status --short --untracked-files=no)" ] || verdict "ABORTED-staging: solar tracked files differ from HEAD"
[ "$(/usr/bin/git -C $H/h2/sl rev-parse $BASE^{tree})" = 59f18cc7e1bcfcde5c1cc99695b89e8dd338e6b3 ] || verdict "ABORTED-staging: solidity-lean base tree is not 59f18cc7"
[ "$(/usr/bin/git -C $H/h2/sl show $BASE:lake-manifest.json | sha256sum | cut -c1-64)" = b2777a1807af2c4bec7481ef1955b5a66b63a18d14062f69ef8a4488a88eca42 ] || verdict "ABORTED-staging: lake-manifest sha256 differs"
[ "$(/usr/bin/git -C $H/h2/sl show $BASE:lean-toolchain | sha256sum | cut -c1-64)" = ca5f1f601d34c49079e38d7e7041eb0f2a9c227fbc1df0f404820069a052330c ] || verdict "ABORTED-staging: lean-toolchain sha256 differs"
# The Counter step's base: solidity-lean f1c19a7f (334d1d92 + Verify/LoopSource/** only), from the pack.
/usr/bin/git -C $H/h2/sl fetch -q $P/sl.bundle f5-sl:refs/heads/f5-sl || verdict "ABORTED-staging: solidity-lean bundle fetch failed"
[ "$(/usr/bin/git -C $H/h2/sl rev-parse f5-sl)" = f1c19a7f217cdcc1a80ddbf87e6146d83273468f ] || verdict "ABORTED-staging: solidity-lean f1c19a7f missing"
mkdir -p $H/h2/shim $H/h2/py314; cp $P/lake-shim $H/h2/shim/lake; chmod +x $H/h2/shim/lake
ln -sf $H/.local/share/uv/python/cpython-3.14.8-linux-x86_64-gnu/bin/python3.14 $H/h2/py314/python3
# Cold start: the run's package copy is the staged cache minus the stripped modules' build outputs.
count() { for p in "$1"/*; do echo "$(basename $p) $(find $p/.lake/build/lib -name '*.olean' 2>/dev/null | wc -l)"; done; }
{ echo "# staged package cache $H/h2/cache/.lake/packages (probe cache), oleans per package"; count $H/h2/cache/.lake/packages
  echo "# mathlib build/lib from lake exe cache get on this box (mathlib-cache-get.log): $([ -d $H/h2/cache/.lake/packages/mathlib/.lake/build/lib ] && echo present || echo absent)"; } > $O/coldstart.txt
rm -rf $H/h2/pkgrun && mkdir -p $H/h2/pkgrun/.lake && cp -a $H/h2/cache/.lake/packages $H/h2/pkgrun/.lake/packages || verdict "ABORTED-staging: package copy failed"
cp $P/strip.txt $O/strip.txt; E=$H/h2/pkgrun/.lake/packages/EthLib/.lake/build
n=0; for m in $(cat $P/strip.txt); do f=$(echo $m | tr . /)
  ls $E/lib/lean/$f.olean > /dev/null 2>&1 || verdict "ABORTED-staging: $m has no olean to strip"
  rm -f $E/lib/lean/$f.* $E/ir/$f.*; n=$((n + 1)); done
{ echo "# run package copy $H/h2/pkgrun/.lake/packages (--packages-from) after stripping $n modules (strip.txt)"; count $H/h2/pkgrun/.lake/packages
  for m in $(cat $P/strip.txt); do [ -e $E/lib/lean/$(echo $m | tr . /).olean ] && echo "STILL PRESENT $m"; done; true; } >> $O/coldstart.txt
grep -q "STILL PRESENT" $O/coldstart.txt && verdict "ABORTED-staging: a stripped olean is still present"
log "cold start: stripped $n EthLib modules; $(grep EthLib $O/coldstart.txt | tr '\n' ' ')"
unset LC_ALL LANGUAGE
export HOME=$H RUSTUP_HOME=/opt/hand-tools/.rustup CARGO_HOME=$H/.cargo CARGO_NET_OFFLINE=true \
  PATH=$H/h2/shim:$H/h2/py314:$H/.cargo/bin:/opt/hand-tools/.cargo/bin:$H/.elan/bin:/usr/local/bin:/usr/bin:/bin \
  CARGO_TARGET_DIR=$S/target-r5i VERGEN_BUILD_TIMESTAMP=2026-11-17T16:30:00Z \
  LEAN_NUM_THREADS=1 CERTIFY_REPLAY_SHARDS=1 LANG=C.UTF-8
env | grep -q -E "^(CERTIFY_ALLOW_|SOLAR=|CERTIFY_SKIP_BUILD=|CERTIFY_REPLAY_TRUST_UPSTREAM=)" && verdict "ABORTED-staging: a hatch variable is set"
log "python $(python3 --version 2>&1); rustc $(rustc --version); git $(/usr/bin/git -C $S rev-parse HEAD)"
[ -e $S/target-r5i ] && verdict "ABORTED-staging: build outputs were staged"
t_b=$(date +%s)
(cd $S && cargo build --locked --bin solar -v) > $O/cargo-build.log 2>&1 || verdict "ABORTED-staging: cargo build failed"
log "producer built on the box in $(( $(date +%s) - t_b )) s: $(grep -c -E '^ *Compiling' $O/cargo-build.log) crates compiled"
(cd $S && cargo build --locked --bin solar -v) > $O/cargo-fresh.log 2>&1 || verdict "ABORTED-staging: second cargo build failed"
grep -q -E '^ *(Compiling|Running)' $O/cargo-fresh.log && verdict "ABORTED-staging: the producer is not fresh (a second cargo build recompiled)"
bin=$S/target-r5i/debug/solar
got=$(sha256sum < $bin | cut -d' ' -f1)
$bin --version > $O/producer-version.txt 2>&1
emb=$(sed -n 's/^Commit SHA: //p' $O/producer-version.txt)
[ "$emb" = $HEADC ] || verdict "ABORTED-staging: the producer embeds commit $emb, not $HEADC"
drv=$(/usr/bin/git -C $S rev-parse HEAD)
[ "$drv" = $HEADC ] || verdict "ABORTED-staging: the driver checkout is $drv, not $HEADC"
dirt=$(/usr/bin/git -C $S status --porcelain -- crates scripts tools/certify Cargo.toml Cargo.lock)
[ -z "$dirt" ] || verdict "ABORTED-staging: the compiler or driver sources are dirty: $(echo $dirt | cut -c1-200)"
prof=$(sed -n 's/^Build Profile: //p' $O/producer-version.txt); feat=$(sed -n 's/^Build Features: //p' $O/producer-version.txt)
printf '{\n  "binary_sha256": "%s",\n  "binary_path": "%s",\n  "embedded_commit": "%s",\n  "driver_commit": "%s",\n  "dirty": false,\n  "profile": "%s",\n  "features": "%s",\n  "build_command": "cargo build --locked --bin solar -v",\n  "built_on": "this box",\n  "cargo_target_dir": "%s",\n  "vergen_build_timestamp": "%s",\n  "rustc": "%s"\n}\n' \
  "$got" "$bin" "$emb" "$drv" "$prof" "$feat" "$CARGO_TARGET_DIR" "$VERGEN_BUILD_TIMESTAMP" "$(rustc --version)" > $O/producer.json
log "producer: $bin sha256 $got embedded $emb driver $drv dirty false profile $prof features $feat flags: cargo build --locked --bin solar (dev profile) CARGO_TARGET_DIR=$CARGO_TARGET_DIR VERGEN_BUILD_TIMESTAMP=$VERGEN_BUILD_TIMESTAMP"
# The run: one scope, 44G, no swap; the inner script waits for the go file.
cat > $O/inner.sh <<'EOF'
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
EOF
chmod +x $O/inner.sh
unit="f5-$(date +%s)"
sudo -E systemd-run --quiet --scope --unit="$unit" -p MemoryMax=44G -p MemorySwapMax=0 \
  --uid=$(id -u) --gid=$(id -g) --setenv=HOME=$HOME --setenv=PATH=$PATH \
  env RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=$CARGO_TARGET_DIR \
  VERGEN_BUILD_TIMESTAMP=$VERGEN_BUILD_TIMESTAMP LEAN_NUM_THREADS=1 CERTIFY_REPLAY_SHARDS=1 LANG=C.UTF-8 CERTIFY_RECEIPTS_DIR=$O/toolchain \
  R4_EXPIRES=$expires bash $O/inner.sh &
spid=$!
cg=""; for _ in $(seq 50); do cg=$(systemctl show -p ControlGroup --value "$unit.scope" 2>/dev/null); [ -n "$cg" ] && break; sleep 0.2; done
cgp=/sys/fs/cgroup$cg
mm=$(cat $cgp/memory.max 2>/dev/null); sm=$(cat $cgp/memory.swap.max 2>/dev/null)
echo "$mm" > $O/memory.max.readback; echo "$sm" > $O/memory.swap.max.readback
# The inner script's process is the one in the scope whose argv is bash $O/inner.sh.
ipid=""; for p in $(cat $cgp/cgroup.procs 2>/dev/null); do
  [ "$(tr '\0' ' ' < /proc/$p/cmdline 2>/dev/null)" = "bash $O/inner.sh " ] && ipid=$p && break
done
icg=$(sed -n 's/^0:://p' /proc/$ipid/cgroup 2>/dev/null)
printf 'unit %s.scope\ncgroup %s\nmemory.max %s (read back from %s/memory.max)\nmemory.swap.max %s\ninner_pid %s\ninner_pid_cgroup %s\n' "$unit" "$cg" "$mm" "$cgp" "$sm" "$ipid" "$icg" > $O/cgroup.txt
shards=$(tr '\0' '\n' < /proc/$ipid/environ 2>/dev/null | sed -n 's/^CERTIFY_REPLAY_SHARDS=//p')
echo "$shards" > $O/shards.readback
log "admission: memory.max $mm swap.max $sm CERTIFY_REPLAY_SHARDS $shards (inner pid $ipid) ttl_left_s $(left)"
if [ -z "$cg" ] || [ -z "$ipid" ] || [ "$icg" != "$cg" ] || [ "$mm" != 47244640256 ] || [ "$sm" != 0 ] || [ "$shards" != 1 ] || [ $(left) -lt 4275 ]; then
  sudo systemctl kill "$unit.scope"; verdict "ABORTED-admission: read-back differs or under 4275 s TTL at driver start (1800 s replay gate + 2475 s projected driver-to-replay)"
fi
touch $O/go; t0=$(date +%s); projected=0
printf 'utc\tndesc\tnprocs_cg\tescaped\tforeign\tescaped_pids\n' > $O/pidtree.tsv
# pidtree: the inner process's descendants (from ps), each one's cgroup against the scope's.
pidtree() {
  local all desc esc="" nesc=0 nfor=0 p c
  all=$(ps -e -o pid=,ppid= 2>/dev/null)
  desc=$(echo "$all" | awk -v r=$ipid '{par[$1]=$2} END {d[r]=1; ch=1; while (ch) {ch=0; for (p in par) if (!(p in d) && (par[p] in d)) {d[p]=1; ch=1}} for (p in d) print p}')
  for p in $desc; do
    c=$(sed -n 's/^0:://p' /proc/$p/cgroup 2>/dev/null) || continue
    [ -z "$c" ] && continue
    [ "$c" = "$cg" ] || { esc="$esc $p:$c"; nesc=$((nesc + 1)); }
  done
  for p in $(cat $cgp/cgroup.procs 2>/dev/null); do echo "$desc" | grep -qx "$p" || nfor=$((nfor + 1)); done
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$(echo "$desc" | grep -c .)" "$(wc -l < $cgp/cgroup.procs 2>/dev/null || echo 0)" "$nesc" "$nfor" "$esc" >> $O/pidtree.tsv
  [ $nesc = 0 ]
}
printf 'utc\tcg_current_kb\tnprocs\ttop_pid\ttop_rss_kb\ttop_argv\thost_used_kb\n' > $O/memtrace.tsv
seen_replay=0
while kill -0 $spid 2>/dev/null; do
  cur=$(( $(cat $cgp/memory.current 2>/dev/null || echo 0) / 1024 ))
  n=$(wc -l < $cgp/cgroup.procs 2>/dev/null || echo 0)
  top=$(for p in $(cat $cgp/cgroup.procs 2>/dev/null); do r=$(awk '/VmRSS/{print $2}' /proc/$p/status 2>/dev/null); [ -n "$r" ] && echo "$r $p"; done | sort -n | tail -1)
  tr_=$(echo "$top" | cut -d' ' -f1); tp=$(echo "$top" | cut -d' ' -f2)
  ta=$(tr '\0' ' ' < /proc/$tp/cmdline 2>/dev/null | cut -c1-300 | tr '\t' ' ')
  used=$(awk '/MemTotal/{t=$2}/MemAvailable/{a=$2}END{print t-a}' /proc/meminfo)
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$cur" "$n" "$tp" "$tr_" "$ta" "$used" >> $O/memtrace.tsv
  cp $cgp/memory.events $O/memory.events 2>/dev/null; cat $cgp/memory.peak > $O/memory.peak 2>/dev/null
  if [ $seen_replay = 0 ] && pgrep -f 'replay_closure.lean' > /dev/null; then
    seen_replay=1; l=$(left); log "replay start; ttl_left_s $l"
    if [ $l -lt 1800 ]; then sudo systemctl kill "$unit.scope"; verdict "ABORTED-gate: replay would start with $l s left (< 1800)"; fi
  fi
  pidtree || { sudo systemctl kill "$unit.scope"; verdict "ABORTED-escape: a descendant of the inner process is outside $unit.scope"; }
  if [ $projected = 0 ] && [ $seen_replay = 0 ] && [ $(( $(date +%s) - t0 )) -ge 600 ]; then
    projected=1; pl=$(( expires - t0 - 2475 )); log "minute-10 projection: replay start with $pl s left"
    if [ $pl -lt 1800 ]; then sudo systemctl kill "$unit.scope"; verdict "ABORTED-projection: replay projected to start with $pl s left (< 1800)"; fi
  fi
  if [ $(left) -lt 600 ] || [ $(date +%s) -ge $gate ]; then sudo systemctl kill "$unit.scope"; log "hard stop (TTL-10 or 1h50 gate)"; verdict "ABORTED-infra: hard stop (TTL-10 min or the 1h50 gate)"; fi
  sleep 2
done
wait $spid
log "scope ended; EXIT $(cat $O/EXIT 2>/dev/null) POST_EXIT $(cat $O/POST_EXIT 2>/dev/null)"
(cd $O && find . -type f ! -name receipts.sha256 ! -name go | LC_ALL=C sort | xargs sha256sum) > $O/receipts.sha256
echo "step1 flag EXIT $(cat $O/EXIT 2>/dev/null) POST_EXIT $(cat $O/POST_EXIT 2>/dev/null) $(date -u +%FT%TZ)" >> $O/STEPS
N=flag_4ce5cf9a; W=$H/h2/flagwork/solidity-lean; D=$W/Certified/$N
# step_scope NAME BUDGET_S SCRIPT: run SCRIPT in its own 44G no-swap scope. The script's first act
# (wrap) reads its own cgroup's memory.max and swap.max and exits 3 unless finite and 0; its last
# writes memory.peak and memory.events. Skipped when BUDGET does not fit before the gate.
step_scope() {
  local name=$1 budget=$2 body=$3 so=$O/$1
  mkdir -p $so
  if [ $(( gate - $(date +%s) )) -lt $budget ] || [ $(left) -lt $(( budget + 600 )) ]; then
    echo "$name SKIPPED: budget $budget s does not fit before the gate" > $so/RESULT; echo "step $name SKIPPED (gate)" >> $O/STEPS; return 1; fi
  cat > $so/run.sh <<EOF
#!/usr/bin/env bash
set -u
so=$so
cg=\$(sed -n 's/^0:://p' /proc/self/cgroup); mm=\$(cat /sys/fs/cgroup\$cg/memory.max); sm=\$(cat /sys/fs/cgroup\$cg/memory.swap.max)
echo "inside scope: cgroup \$cg memory.max \$mm swap.max \$sm" > \$so/cgroup.txt
case "\$mm" in max|"") echo 3 > \$so/EXIT; exit 3;; esac; [ "\$sm" = 0 ] || { echo 3 > \$so/EXIT; exit 3; }
( $body ) > \$so/out.log 2>&1
rc=\$?; echo \$rc > \$so/EXIT
cat /sys/fs/cgroup\$cg/memory.peak > \$so/memory.peak; cat /sys/fs/cgroup\$cg/memory.events > \$so/memory.events
EOF
  local u="f5-$name-$(date +%s)"
  sudo -E systemd-run --quiet --scope --unit="$u" -p MemoryMax=44G -p MemorySwapMax=0 --uid=$(id -u) --gid=$(id -g) \
    --setenv=HOME=$HOME --setenv=PATH=$PATH env RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME CARGO_TARGET_DIR=$CARGO_TARGET_DIR \
    VERGEN_BUILD_TIMESTAMP=$VERGEN_BUILD_TIMESTAMP LEAN_NUM_THREADS=1 LANG=C.UTF-8 SOLAR_VERIFY_Z3=$H/h2/z3/z3 \
    timeout $budget bash $so/run.sh &
  local sp=$!
  while kill -0 $sp 2>/dev/null; do
    if [ $(left) -lt 600 ] || [ $(date +%s) -ge $gate ]; then sudo systemctl kill "$u.scope"; echo "$name killed at the gate" >> $so/RESULT; fi
    sleep 2; done
  wait $sp
  log "step $name exit $(cat $so/EXIT 2>/dev/null) peak $(cat $so/memory.peak 2>/dev/null)"
  (cd $so && find . -type f ! -name receipts.sha256 | LC_ALL=C sort | xargs sha256sum) > $so/receipts.sha256
  echo "step $name EXIT $(cat $so/EXIT 2>/dev/null) $(date -u +%FT%TZ)" >> $O/STEPS
}
mkdir -p $H/h2/z3; cp $P/z3 $H/h2/z3/z3; chmod +x $H/h2/z3/z3
if [ "$(cat $O/EXIT 2>/dev/null)" = 0 ]; then
  step_scope bind 300 "python3 $S/tools/certify/bind.py --record $O/record/record.json --dir $D --replay $O/replay-receipt.json --solar $bin --driver-root $S --out $O/bind/bind-checked.json"
  # step 3: the Resources-premise route on Flag's own run, with the manifest binding.
  cp $D/Statement.lean $O/certified-Statement.lean; cp $D/Final.lean $O/certified-Final.lean
  python3 - $D/shape.json $O/shape-open.json <<'PY'
import json, sys
s = json.load(open(sys.argv[1]))
s["kind"] = "functional"; s["open_premises"] = ["resources"]
s["proof"] = "(" + s["proof"] + ").imp fun _ h => ⟨h.1, SolidCore.Refinement.Shapes.Certified.functional h.2⟩"
json.dump(s, open(sys.argv[2], "w"), indent=2)
PY
  step_scope open 1500 "cd $W && python3 $S/tools/certify/final.py $O/shape-open.json --name $N --contract Flag --hex \$(tr -d '[:space:]' < $D/bytecode.hex) --out $D/Final.lean --statement-out $D/Statement.lean --record $O/open/final-open.json --manifest-json $D/gen/Flag/manifest.json && cp $D/Statement.lean $D/Final.lean $O/open/ && lake build -KleanMemory=6000 $N.Final && printf 'import $N.Final\n#print axioms Certified.$N.Final.final\n' > $O/open/Ax.lean && lake env lean -M 6000 $O/open/Ax.lean > $O/open/axioms.out 2>&1 && python3 -c \"import json,sys; r=json.load(open('$O/open/final-open.json')); assert r['strongest'] is False and r['open_premises']==['resources'] and r['statuses']['closure']=='not closure', r\""
  cp $O/certified-Statement.lean $D/Statement.lean; cp $O/certified-Final.lean $D/Final.lean
fi
step_scope counter 2700 "cd $S && env -u CARGO_NET_OFFLINE python3 tools/certify/generic.py $P/Counter.sol none --contract Counter --solidity-lean $H/h2/sl --base f1c19a7f217cdcc1a80ddbf87e6146d83273468f --work $H/h2/counterwork --record-dir $O/counter/record --solc $H/.cache/solc/solc-0.8.35 --packages-from $H/h2/pkgrun --probe-cache $H/h2/cache --loop-summary bump=Verify.LoopSource.Equiv.Counter.Summary:Verify.LoopSource.Equiv.CounterSummary.summary; echo generic-exit \$?; python3 -c \"import json; r=json.load(open('$O/counter/record/record.json')); print(r.get('status'), json.dumps(r.get('loop_cuts'))[:2000], r.get('loop_cuts_blocked'))\"; cp -r $H/h2/counterwork/loops $O/counter/ 2>/dev/null; cp -r $H/h2/counterwork/solidity-lean/Certified/*/Loops $O/counter/ 2>/dev/null; true"
if [ -f $P/erc20/args ]; then
  step_scope erc20 1800 "cd $S && python3 tools/certify/generic.py $P/erc20/\$(sed -n 1p $P/erc20/args) natspec --contract \$(sed -n 2p $P/erc20/args) --solidity-lean $H/h2/sl --base 334d1d924f8f6c29ac827fd81d1cc739721b479a --work $H/h2/erc20work --record-dir $O/erc20/record --solc $H/.cache/solc/solc-0.8.35 --packages-from $H/h2/pkgrun --probe-cache $H/h2/cache"
else
  mkdir -p $O/erc20; echo "erc20 OPEN: no ERC20 source staged in f5pack/erc20" > $O/erc20/RESULT; echo "step erc20 OPEN (not staged)" >> $O/STEPS
fi
(cd $O && find . -type f ! -name receipts.sha256 ! -name go | LC_ALL=C sort | xargs sha256sum) > $O/receipts.sha256
echo done > $O/FINISHED
verdict "FINISHED step1 $(cat $O/EXIT 2>/dev/null)/$(cat $O/POST_EXIT 2>/dev/null); $(grep '^step ' $O/STEPS | tr '\n' ';')"
