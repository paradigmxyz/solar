#!/usr/bin/env bash
# One box slot of the ERC20 head-bytes run (sequential slots; never two boxes at once).
#
#   tools/certify/erc20_any/slot.sh SLOT DEADLINE [PRIOR_REPLAY]
#
# SLOT is 1 or 2; DEADLINE is the box's expiry as Unix time. Works under ~/w: solar (this branch),
# solidity-lean at LEAN_COMMIT, the candidate compiler at CANDIDATE_COMMIT built here and refused
# unless its sha256 is CANDIDATE_SHA256. Steps (each one's exit, wall time and peak RSS go to
# ~/w/slot<N>/steps; a machine-wide sampler writes ~/w/slot<N>/mem.tsv throughout):
#   port, candidate build, slice spec (solar-verify at SLICE_COMMIT, source side only), prep.sh,
#   run.sh (build and check_final), then replay.sh. Slot 2 needs PRIOR_REPLAY, slot 1's
#   OUT/replay, and replay.sh refuses (exit 3) unless its plan is byte-identical to it.
# The replay stops admitting batches REPLAY_MARGIN (default 50) minutes before DEADLINE and kills
# what is still running 12 minutes before it, leaving time to copy everything off.
set -uo pipefail
slot=$1; deadline=$2; prior=${3:-}
: "${LEAN_COMMIT:=1f0db760f3ef35c7ffc974a24bae68d566788109}"
: "${CANDIDATE_COMMIT:=f71b7554d6e044cf974ac4f0e5f0b184cd96ff9b}"
: "${CANDIDATE_SHA256:=15808eeef2f5d5d021372d43a36ef491492cc8b2fc84b2c3b97a666a736381db}"
: "${SLICE_COMMIT:=902cccc2cb1213219789780a39caf3c1d4d9f35e}"
source ~/w/env.sh
w=~/w; o=$w/slot$slot; mkdir -p "$o"; here=$w/solar/tools/certify/erc20_any
step() { local n=$1; shift; local t0; t0=$(date +%s)
  /usr/bin/time -v -o "$o/$n.time" "$@" > "$o/$n.log" 2>&1; local rc=$?
  echo "$n exit=$rc wall_s=$(( $(date +%s) - t0 )) peak_rss_kb=$(awk '/Maximum resident/{print $NF}' "$o/$n.time") $(date -u +%FT%TZ)" | tee -a "$o/steps"
  return $rc; }
( while :; do
    echo "$(date +%s) $(awk '/MemTotal/{t=$2} /MemAvailable/{a=$2} END{print t-a, t}' /proc/meminfo) $(ps -eo rss=,comm= | awk '$2 ~ /^(lean|lake|leanchecker|solar|cargo|rustc|solar-verify)$/ {s+=$1; n++; if($1>m)m=$1} END{print n+0, m+0, s+0}')" >> "$o/mem.tsv"
    sleep 2; done ) & sampler=$!
trap 'kill $sampler 2>/dev/null' EXIT
cd $w/solidity-lean && git fetch -q origin "$LEAN_COMMIT" && git checkout -q --detach "$LEAN_COMMIT" || exit 1
echo "slot $slot solar $(git -C $w/solar rev-parse HEAD) lean $(git rev-parse HEAD) deadline $deadline" | tee "$o/identity"
export SOLC=$HOME/.cache/solc/solc-0.8.35
step port bash scripts/verify/uniswap_v2_gen.sh || exit 1
c=$w/candidate
[[ -d $c ]] || git clone -q https://github.com/paradigmxyz/solar.git "$c"
git -C "$c" fetch -q origin "$CANDIDATE_COMMIT" && git -C "$c" checkout -q --detach "$CANDIDATE_COMMIT" || exit 1
CARGO_BUILD_JOBS=4 step candidate cargo build --release --locked --manifest-path "$c/Cargo.toml" --bin solar || exit 1
[[ $(sha256sum < "$c/target/release/solar" | cut -c1-64) == "$CANDIDATE_SHA256" ]] \
  || { echo "candidate binary is not $CANDIDATE_SHA256" | tee -a "$o/steps"; exit 1; }
v=$w/solar-verify
[[ -d $v ]] || git clone -q https://github.com/paradigmxyz/solar.git "$v"
git -C "$v" checkout -q "$SLICE_COMMIT" 2>/dev/null || { git -C "$v" fetch -q origin "$SLICE_COMMIT" && git -C "$v" checkout -q "$SLICE_COMMIT"; } || exit 1
CARGO_BUILD_JOBS=4 step verify cargo build --release --locked --manifest-path "$v/Cargo.toml" -p solar-verify || exit 1
step core lake build -KleanMemory=6000 Contracts.UniswapV2.ERC20.Core || exit 1
SOLAR_DIR=$v SOLAR_VERIFY=$v/target/release/solar-verify step slice bash scripts/verify/uniswap_v2_transfer_slice.sh || exit 1
export EXPECT_SOLAR_SHA256=$CANDIDATE_SHA256 EXPECT_SOLAR_COMMIT=$CANDIDATE_COMMIT
step prep bash "$here/prep.sh" $w/solidity-lean "$c/target/release/solar" $w/solar "$o/prep" || exit 1
step run bash "$here/run.sh" $w/solidity-lean "$c/target/release/solar" "$o/out" || exit 1
git status --short > "$o/tree-status"; git diff > "$o/tree.diff"
export REPLAY_STOP_AT=$(( deadline - ${REPLAY_MARGIN:-50} * 60 )) REPLAY_KILL_AT=$(( deadline - 12 * 60 ))
[[ -n $prior ]] && export PRIOR_REPLAY=$prior
step replay bash "$here/replay.sh" $w/solidity-lean "$o/out" ${PAR:-4} 25
echo "slot $slot replay exit recorded in $o/steps"
