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
R="$OUT/$name"
if [[ -e "$R" ]]; then echo "case: $R exists; every run starts fresh" >&2; exit 2; fi
mkdir -p "$R"
cp "$src" "$R/"
sha256sum "$src" | cut -c1-64 > "$R/source.sha256"
unset SOLAR CERTIFY_SKIP_BUILD SOLAR_VERIFY CERTIFY_REPLAY_TRUST_UPSTREAM CERTIFY_ALLOW_FOREIGN_SOLAR
export LEAN_MEMORY=6000 LEAN_NUM_THREADS=1
now=$(date +%s); left=$((DEADLINE - now))
echo "START $(date -u +%FT%TZ) solar=$SOLAR_SHA lean=$LEAN_SHA expect=$expect budget_s=$left" > "$R/status"
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
echo "$kind exit=$rc producer=$pc log_sha256=$(sha256sum < "$R/certify.log" | cut -c1-64) $(date -u +%FT%TZ)" >> "$R/status"
bash "$here/ship.sh" "$name"
