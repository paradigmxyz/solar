#!/usr/bin/env bash
# avrun.sh EXPIRES_EPOCH: the all-value Flag certification, derived from r5jrun.sh (b87fd2ac, the
# accepted v3 runner) with only these changes: solar is the all-value port 6e703c14
# (alpharush/t7-allvalue, 5 commits on afaefd40) staged in ~/avpack, its producer built on this
# box from that commit, and the scope unit is named av-<epoch>. Every other term (solidity-lean
# base 334d1d92 through --base only, shards 1, LEAN_NUM_THREADS 1, 44G scope with no swap and its
# fail-closed read-back, cold start, preflight, gates, hard stop at TTL-10, hatch variables unset)
# is r5j's, unchanged. Everything is recorded under ~/h2/flagrun.
set -u
expires=$1
H=/home/hand; P=$H/avpack; O=$H/h2/flagrun; S=$H/h2/r5isrc
HEADC=6e703c149b3a52885f6d5da317ec85106cc7c8b6; BASE=334d1d924f8f6c29ac827fd81d1cc739721b479a
mkdir -p $O
log() { echo "$(date -u +%FT%TZ) $*" | tee -a $O/r4.log; }
verdict() { echo "$1" > $O/VERDICT; log "VERDICT $1"; exit 0; }
left() { echo $(( expires - $(date +%s) )); }
log "avrun start; expires $(date -u -d @$expires +%FT%TZ); ttl_left_s $(left); nproc $(nproc); mem_kb $(awk '/MemTotal/{print $2}' /proc/meminfo)"
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
(cd $S && rm -rf .git && /usr/bin/git init -q && /usr/bin/git fetch -q $P/src.bundle allvalue:refs/heads/allvalue &&
  /usr/bin/git symbolic-ref HEAD refs/heads/allvalue && /usr/bin/git reset -q) || verdict "ABORTED-staging: solar git restore failed"
[ "$(/usr/bin/git -C $S rev-parse HEAD)" = $HEADC ] || verdict "ABORTED-staging: solar HEAD is not $HEADC"
[ "$(/usr/bin/git -C $S rev-parse HEAD^{tree})" = 8ae6de92959fa38fe4efdf0dbb174ea2cd4ac07d ] || verdict "ABORTED-staging: solar tree is not 8ae6de92"
[ -z "$(/usr/bin/git -C $S status --short --untracked-files=no)" ] || verdict "ABORTED-staging: solar tracked files differ from HEAD"
[ "$(/usr/bin/git -C $H/h2/sl rev-parse $BASE^{tree})" = 59f18cc7e1bcfcde5c1cc99695b89e8dd338e6b3 ] || verdict "ABORTED-staging: solidity-lean base tree is not 59f18cc7"
[ "$(/usr/bin/git -C $H/h2/sl show $BASE:lake-manifest.json | sha256sum | cut -c1-64)" = b2777a1807af2c4bec7481ef1955b5a66b63a18d14062f69ef8a4488a88eca42 ] || verdict "ABORTED-staging: lake-manifest sha256 differs"
[ "$(/usr/bin/git -C $H/h2/sl show $BASE:lean-toolchain | sha256sum | cut -c1-64)" = ca5f1f601d34c49079e38d7e7041eb0f2a9c227fbc1df0f404820069a052330c ] || verdict "ABORTED-staging: lean-toolchain sha256 differs"
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
while [ ! -f $O/go ]; do sleep 0.2; done
echo "$(date -u +%FT%TZ) driver start" >> $O/stages.log
env | LC_ALL=C sort > $O/driver-env.txt
/usr/bin/time -v -o $O/time-v.txt python3 /home/hand/h2/r5isrc/tools/certify/generic.py /home/hand/h2/Flag.sol natspec --contract Flag \
  --solidity-lean /home/hand/h2/sl --base 334d1d924f8f6c29ac827fd81d1cc739721b479a --work /home/hand/h2/flagwork \
  --record-dir $O/record --solc /home/hand/.cache/solc/solc-0.8.35 --packages-from /home/hand/h2/pkgrun --probe-cache /home/hand/h2/cache \
  > $O/driver.log 2>&1
echo $? > $O/EXIT
echo "$(date -u +%FT%TZ) driver exit $(cat $O/EXIT)" >> $O/stages.log
bash /home/hand/avpack/post.sh >> $O/post.log 2>&1
echo $? > $O/POST_EXIT
EOF
chmod +x $O/inner.sh
unit="av-$(date +%s)"
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
  if [ $(left) -lt 600 ]; then sudo systemctl kill "$unit.scope"; log "hard stop at TTL-10"; verdict "ABORTED-infra: hard stop at TTL-10 min"; fi
  sleep 2
done
wait $spid
log "scope ended; EXIT $(cat $O/EXIT 2>/dev/null) POST_EXIT $(cat $O/POST_EXIT 2>/dev/null)"
(cd $O && find . -type f ! -name receipts.sha256 ! -name go | LC_ALL=C sort | xargs sha256sum) > $O/receipts.sha256
echo done > $O/FINISHED
