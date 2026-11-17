#!/usr/bin/env bash
# One parallel shard of the ERC20 head-bytes replay, on its own box, from copied artifacts only.
#
#   tools/certify/erc20_any/shard.sh SHARD DEADLINE
#
# SHARD is a shard number from shard_plan.py; DEADLINE is the box's expiry as Unix time. Works under
# ~/w: solar (this branch), solidity-lean checked out at LEAN_COMMIT with its packages fetched
# (lake exe cache get), and ~/w/art holding what the hosted Hand copied here: the candidate binary,
# sl-build.tar (the earlier slot's .lake/build and generated Certified/uv2_erc20), deps-build.tar
# (the package and vendor builds the cache does not hold), check_final.log, the earlier slot's plan
# and shard_plan.py's output. Nothing is built here. Each file is refused (exit 4) unless its sha256
# is the pinned one, and the unpacked tree is refused (exit 4) unless every planned olean has the
# sha256 the plan gives it, before replay.sh runs; replay.sh then rebuilds the plan from
# check_final.log and exits 3 if its sha256 is not PLAN_SHA256.
#
# The replay runs under build_cap.sh (cgroup memory.max at 75% of memory, read back, swap 0;
# exit 95 if refused) with the shard's batches, PAR (default 4) checkers at most, and per-batch
# admission against DEADLINE minus KILL_MARGIN (default 10) minutes: a batch whose projected wall
# (the shard's walls file) would end later is not started, and what still runs then is killed and
# counted unverified. Writes ~/w/shard<N>/: steps, gate.txt, identity, sha256s, precheck and
# out/replay.
set -uo pipefail
k=$1; deadline=$2
: "${LEAN_COMMIT:=1f0db760f3ef35c7ffc974a24bae68d566788109}"
: "${CANDIDATE_SHA256:=15808eeef2f5d5d021372d43a36ef491492cc8b2fc84b2c3b97a666a736381db}"
: "${BUILD_TAR_SHA256:=9391ca02e53e2dce8886289e94ddf66c92ee8ae81e1fc49a891cae9a8b4dbed9}"
: "${DEPS_TAR_SHA256:=3f587b8f90b4da678152f512c893a846a7ab7cf5565f89ac8502045972948687}"
: "${CHECK_LOG_SHA256:=bfd7e31334a3e99efc1b3e25a98526672b18d24731d05c9808269494efe07e4a}"
: "${PLAN_SHA256:=e0c7e9677ed959dfcbcdd72770a578ffc8d7aa6468523585e0892e331aebf13b}"
: "${SHARDS_SHA256:?the sha256 of shards.txt from shard_plan.py}"
source ~/w/env.sh
w=~/w; a=$w/art; o=$w/shard$k; lean=$w/solidity-lean; here=$w/solar/tools/certify/erc20_any
mkdir -p "$o"; sd=$a/slot2plan/shard$k
refuse() { echo "refused: $1" | tee -a "$o/steps"; exit 4; }
check() { [[ $(sha256sum < "$1" | cut -c1-64) == "$2" ]] || refuse "$1 is not $2"; echo "$2  $1" >> "$o/sha256s"; }
: > "$o/sha256s"
check "$a/solar" "$CANDIDATE_SHA256"
check "$a/sl-build.tar" "$BUILD_TAR_SHA256"
check "$a/deps-build.tar" "$DEPS_TAR_SHA256"
check "$a/check_final.log" "$CHECK_LOG_SHA256"
check "$a/plan" "$PLAN_SHA256"
check "$a/slot2plan/shards.txt" "$SHARDS_SHA256"
for f in batches peaks walls; do
  want=$(awk -v s="shard$k" -v f="$f" '$1 == s { for (i = 1; i < NF; i++) if ($i == f && $(i + 1) ~ /^[0-9a-f]+$/ && length($(i + 1)) == 64) print $(i + 1) }' "$a/slot2plan/shards.txt")
  [[ -n $want ]] || refuse "shards.txt has no $f sha256 for shard$k"
  check "$sd/$f" "$want"
done
cd "$lean" && [[ $(git rev-parse HEAD) == "$LEAN_COMMIT" ]] || refuse "solidity-lean is not at $LEAN_COMMIT"
echo "shard $k solar $(git -C $w/solar rev-parse HEAD) lean $LEAN_COMMIT deadline $deadline host $(hostname)" | tee "$o/identity"
tar -xf "$a/sl-build.tar" && tar -xf "$a/deps-build.tar" || refuse "unpacking failed"
mkdir -p "$o/out" && cp "$a/check_final.log" "$o/out/check_final.log"
python3 - "$a/plan" > "$o/precheck" <<'PY' || refuse "the unpacked tree does not match the plan (see precheck)"
import hashlib, sys
lines = open(sys.argv[1]).read().splitlines()
bad = 0
for l in lines:
    f = l.split()
    try:
        ok = hashlib.sha256(open(f[3], "rb").read()).hexdigest() == f[2]
    except OSError:
        ok = False
    if not ok:
        bad += 1
        print("mismatch", f[1], f[3])
print(f"precheck planned {len(lines)} mismatched {bad}")
sys.exit(1 if bad else 0)
PY
tail -1 "$o/precheck" | tee -a "$o/steps"
total=$(awk '/MemTotal/ { print $2 }' /proc/meminfo); budget=$(( total * 75 / 100 ))
kill_at=$(( deadline - ${KILL_MARGIN:-10} * 60 ))
t0=$(date +%s)
/usr/bin/time -v -o "$o/replay.time" env REPLAY_EXPECT_PLAN="$PLAN_SHA256" REPLAY_BATCHES="$sd/batches" \
  SCHED_WALLS="$sd/walls" REPLAY_STOP_AT="$kill_at" REPLAY_KILL_AT="$kill_at" \
  bash "$here/build_cap.sh" "${PAR:-4}" "$budget" "$o/gate.txt" replay -- \
  bash "$here/replay.sh" "$lean" "$o/out" "${PAR:-4}" 25 "$sd/peaks" > "$o/replay.log" 2>&1; rc=$?
echo "replay exit=$rc wall_s=$(( $(date +%s) - t0 )) peak_rss_kb=$(awk '/Maximum resident/{print $NF}' "$o/replay.time") kill_at=$kill_at $(date -u +%FT%TZ)" | tee -a "$o/steps"
[[ -f $o/out/replay/gate.txt ]] && sed 's/^/replay /' "$o/out/replay/gate.txt" >> "$o/gate.txt"
exit $rc
