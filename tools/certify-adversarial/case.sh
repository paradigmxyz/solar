#!/usr/bin/env bash
# case.sh NAME SOURCE EXPECT: one case, a full certify run of SOURCE at the certified tier in a
# fresh work directory, then the producer check and shipping. EXPECT is the expected exit code.
#
# The run gets no --probe-cache (no stage cache) and no build overrides. It is killed at the box's
# hard end (DEADLINE, epoch seconds) so nothing is in flight when the box expires; a killed run is
# KILLED-DEADLINE, which is no verdict. Called by run.sh, which sets the environment.
# The verdict line in status reads: KIND exit=N producer=ok|mismatch|no_record log_sha256=H.
set -u
name=$1; src=$2; expect=$3
here=$(cd "$(dirname "$0")" && pwd)
# The case's class and intended stage, registered in stages.tsv before the run (run.sh refuses a
# case without one).
IFS=$'\t' read -r _ cls stage < <(awk -F'\t' -v n="$name" '$1 == n' "$here/stages.tsv")
R="$OUT/$name"
if [[ -e "$R" ]]; then echo "case: $R exists; every run starts fresh" >&2; exit 2; fi
mkdir -p "$R"
cp "$src" "$R/"
sha256sum "$src" | cut -c1-64 > "$R/source.sha256"
unset SOLAR CERTIFY_SKIP_BUILD SOLAR_VERIFY CERTIFY_REPLAY_TRUST_UPSTREAM CERTIFY_ALLOW_FOREIGN_SOLAR \
    CERTIFY_ALLOW_UNPINNED CERTIFY_PIN_BASE
# The driver checkout must still be the clean one the producer was built from.
driver=$(git -C "$SOLAR_DIR" rev-parse HEAD)
if [[ "$driver" != "$SOLAR_SHA" || -n "$(git -C "$SOLAR_DIR" status --porcelain --untracked-files=no)" ]]; then
    echo "REFUSED-DRIVER $(date -u +%FT%TZ) driver checkout at $driver, not a clean $SOLAR_SHA" > "$R/status"
    bash "$here/ship.sh" "$name"
    exit 0
fi
harness=$(git -C "$here" rev-parse HEAD 2>/dev/null || echo unknown)
# The case runs only under a finite memory limit, read back from its own cgroup.
cg=$(sed -n 's/^0:://p' /proc/self/cgroup | head -1)
mmax=$(cat "/sys/fs/cgroup$cg/memory.max" 2>/dev/null || echo unreadable)
smax=$(cat "/sys/fs/cgroup$cg/memory.swap.max" 2>/dev/null || echo unreadable)
if [[ ! "$mmax" =~ ^[0-9]+$ || "$smax" != 0 ]]; then
    echo "REFUSED-SCOPE $(date -u +%FT%TZ) cgroup=$cg memory.max=$mmax memory.swap.max=$smax" > "$R/status"
    bash "$here/ship.sh" "$name"
    exit 0
fi
# Both escape hatches are unset above; the status line records this case's own environment.
hatches="CERTIFY_ALLOW_UNPINNED=${CERTIFY_ALLOW_UNPINNED-unset} CERTIFY_ALLOW_FOREIGN_SOLAR=${CERTIFY_ALLOW_FOREIGN_SOLAR-unset}"
export LEAN_MEMORY=6000 LEAN_NUM_THREADS=1
now=$(date +%s); left=$((DEADLINE - now))
echo "START $(date -u +%FT%TZ) driver=$driver harness=$harness lean=$LEAN_SHA expect=$expect budget_s=$left $hatches cgroup=$cg memory.max=$mmax memory.swap.max=$smax" > "$R/status"
args=(tools/certify/certify "$src" natspec --solidity-lean "$LEAN_DIR" --base "$LEAN_SHA"
      --work "$R/work" --record-dir "$R/rec")
[[ -z "${PACKAGES_FROM:-}" ]] || args+=(--packages-from "$PACKAGES_FROM")
printf '%q ' "${args[@]}" > "$R/command"; echo >> "$R/command"
# The run gets its own session, so that at the deadline the whole tree (lake, lean, the
# sandbox's namespaces) is signalled at once; anything left that still names this case's
# directory is killed after it.
# setsid -w waits for the run and returns its exit code; the session leader writes its own pid,
# which is the session's id.
(cd "$SOLAR_DIR" && exec setsid -w bash -c 'echo $$ > "$0"; exec "$@"' "$R/session" \
    python3 "$here/receipt.py" --out "$R/receipt.json" -- "${args[@]}") > "$R/certify.log" 2>&1 &
pid=$!
for _ in $(seq 1 20); do [[ -s "$R/session" ]] && break; sleep 0.5; done
sid=$(cat "$R/session" 2>/dev/null)
kind=DONE
while kill -0 "$pid" 2>/dev/null; do
    if (( $(date +%s) >= DEADLINE )); then
        kind=KILLED-DEADLINE
        [[ -n "$sid" ]] && pkill -TERM -s "$sid" 2>/dev/null
        for _ in $(seq 1 30); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
        [[ -n "$sid" ]] && pkill -KILL -s "$sid" 2>/dev/null
        pkill -KILL -f -- "$R/" 2>/dev/null
        break
    fi
    sleep 5
done
wait "$pid"; rc=$?
if [[ $kind == KILLED-DEADLINE ]] && pgrep -f -- "$R/" > /dev/null; then kind=KILLED-DEADLINE-LEFTOVER; fi
python3 "$here/producer_check.py" "$PRODUCER" "$R" > "$R/producer-check.json"
pc=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"])' "$R/producer-check.json" 2>/dev/null || echo unreadable)
# match: the actual exit is one of the expected ones (EXPECT may list several, as 1|3). A killed
# run has no verdict, so it never matches.
match=no
if [[ $kind == DONE ]]; then
    IFS='|' read -ra ok <<< "$expect"
    for e in "${ok[@]}"; do [[ "$rc" == "$e" ]] && match=yes; done
fi
# A control that certified is replayed independently (--replay: a fresh cold run checked against
# its record), within the same deadline; its exit is part of the control's gate.
if [[ $cls == control && $kind == DONE && $rc == 0 ]]; then
    left=$((DEADLINE - $(date +%s)))
    if (( left > 60 )); then
        (cd "$SOLAR_DIR" && exec setsid -w timeout -k 30 "$left" tools/certify/certify --replay "$R/rec") \
            > "$R/replay.log" 2>&1
        echo $? > "$R/replay.exit"
    else
        echo no-time > "$R/replay.exit"
    fi
fi
gate=$(python3 "$here/gate.py" "$R" "$cls" "$stage" "$kind" 2>/dev/null || echo stage_ok=unreadable)
echo "$kind exit=$rc expect=$expect match=$match class=$cls $gate replay=$(cat "$R/replay.exit" 2>/dev/null || echo none) producer=$pc log_sha256=$(sha256sum < "$R/certify.log" | cut -c1-64) $(date -u +%FT%TZ)" >> "$R/status"
{
    echo "memory.events $(tr '\n' ' ' < "/sys/fs/cgroup$cg/memory.events" 2>/dev/null)"
    echo "memory.peak $(cat "/sys/fs/cgroup$cg/memory.peak" 2>/dev/null || echo unreadable)"
} > "$R/scope.txt"
bash "$here/ship.sh" "$name"
