#!/usr/bin/env bash
# Independent clean build of solar f71b7554 on a replay box, run only after its shard.sh exited.
# Clean env: libc6-dev and build-essential from apt, a fresh rustup toolchain 1.100.0 and
# CARGO_HOME, and a fresh clone. It builds at the same absolute paths as the pinned binary
# (/home/hand/work/solar-f71, CARGO_HOME /home/hand/.cargo, target
# /home/hand/work/solar-f71-target) because the binary embeds them.
# Writes ~/w/indep/: steps, env-receipt, apt.log, rustup.log, rustc-vV, build.log, sha256, solar.
# Exit codes: 0 means a binary was built and compared (MATCH or DIFFERS); 5 means refused (not
# enough time, replay still running, or no /home/hand); 6 means BUILD-FAILED (a setup or build
# step failed or the binary is missing), which prints no MATCH or DIFFERS line.
# Setup counts against the time gate: DEADLINE (Unix time) minus 10 min must leave at least
# 15 min at start, and every later step is cut off at DEADLINE minus 10 min.
set -uo pipefail
deadline=$1; o=~/w/indep; mkdir -p "$o"; log() { echo "$* $(date -u +%FT%TZ)" | tee -a "$o/steps"; }
fail() { log "BUILD-FAILED: $*"; exit 6; }
left() { echo $(( deadline - 600 - $(date +%s) )); }
(( $(left) >= 900 )) || { log "refused: not enough time before expiry minus 10 min"; exit 5; }
pgrep -f 'shard.sh|sched.py|leanchecker' >/dev/null && { log "refused: replay still running"; exit 5; }
sudo -n mkdir -p /home/hand/work && sudo -n chown -R "$(id -u):$(id -g)" /home/hand || { log "refused: cannot create /home/hand"; exit 5; }

# Toolchain prerequisites, with the environment recorded in the receipt.
{ sudo -n DEBIAN_FRONTEND=noninteractive timeout "$(left)" apt-get update -q &&
  sudo -n DEBIAN_FRONTEND=noninteractive timeout "$(left)" apt-get install -y -q libc6-dev build-essential; } > "$o/apt.log" 2>&1 ||
  fail "apt-get install libc6-dev build-essential"
{
  echo "== os-release"; cat /etc/os-release
  echo "== uname"; uname -srm
  echo "== dpkg"; dpkg-query -W -f='${Package} ${Version} ${Architecture}\n' libc6 libc6-dev build-essential gcc gcc-13 binutils make 2>&1
  echo "== cc"; cc --version | head -1; ld --version | head -1
} > "$o/env-receipt"
log "env-receipt sha256 $(sha256sum < "$o/env-receipt" | cut -c1-64)"

export RUSTUP_HOME=/home/hand/.rustup CARGO_HOME=/home/hand/.cargo
rm -rf /home/hand/.cargo /home/hand/.rustup /home/hand/work/solar-f71 /home/hand/work/solar-f71-target
curl -sSfL https://sh.rustup.rs | timeout "$(left)" sh -s -- -y -q --profile minimal --default-toolchain 1.100.0 --no-modify-path > "$o/rustup.log" 2>&1; rc=$?
log "rustup exit=$rc"; (( rc == 0 )) || fail "rustup exit $rc"
export PATH=$CARGO_HOME/bin:$PATH
{ rustc -vV > "$o/rustc-vV" && sha256sum "$(rustup which rustc)" >> "$o/rustc-vV"; } || fail "rustc missing"
log "rustc $(rustc -V)"
timeout "$(left)" git clone -q https://github.com/paradigmxyz/solar.git /home/hand/work/solar-f71 || fail "git clone"
cd /home/hand/work/solar-f71 && git checkout -q f71b7554d6e044cf974ac4f0e5f0b184cd96ff9b || fail "git checkout f71b7554"
log "src $(git rev-parse HEAD) tree $(git rev-parse 'HEAD^{tree}') lock $(sha256sum < Cargo.lock | cut -c1-64) dirty $(git status --porcelain | wc -l)"
(( $(left) > 0 )) || fail "no time left before expiry minus 10 min"
t0=$(date +%s)
env -u RUSTFLAGS CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=/home/hand/work/solar-f71-target timeout "$(left)" nice -n 5 cargo build --release --locked --bin solar > "$o/build.log" 2>&1; rc=$?
b=/home/hand/work/solar-f71-target/release/solar
log "build exit=$rc wall_s=$(( $(date +%s) - t0 )) left_s=$(left)"
(( rc == 0 )) || fail "cargo exit $rc (124 = cut off at expiry minus 10 min)"
[[ -f $b ]] || fail "binary missing at $b"
sha256sum "$b" | tee "$o/sha256"
cp "$b" "$o/solar"; "$b" --version >> "$o/steps"
[[ $(cut -c1-64 "$o/sha256") == 15808eeef2f5d5d021372d43a36ef491492cc8b2fc84b2c3b97a666a736381db ]] && log "MATCH 15808eee" || log "DIFFERS from 15808eee"
