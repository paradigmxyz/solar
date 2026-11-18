#!/usr/bin/env bash
# run.sh: runs the cases of CASES (lines "NAME SOURCE EXPECT", # comments) on one box, at most
# SLOTS at a time, and copies each case off as soon as it finishes (ship.sh).
#
# Required environment: SOLAR_DIR (a clean solar checkout at SOLAR_SHA), SOLAR_SHA, LEAN_DIR (a
# solidity-lean checkout holding LEAN_SHA), LEAN_SHA, BOX_CREATED (the box's created_at, ISO 8601),
# CASES, OUT (a new directory). Optional: PACKAGES_FROM, SLOTS (default 6), ADMIT_MIN (default 100:
# no case is admitted 1h40m or more after BOX_CREATED), END_MIN (default 115: every run is killed
# by then, 5 minutes before a 2h keep_alive), EST_MIN (default 60: a case is admitted only if one
# run that long ends by END_MIN), SHIP_REMOTE and SHIP_BRANCH.
# A case not admitted is recorded NOT-ADMITTED and shipped, with no verdict.
set -u
here=$(cd "$(dirname "$0")" && pwd)
: "${SOLAR_DIR:?}" "${SOLAR_SHA:?}" "${LEAN_DIR:?}" "${LEAN_SHA:?}" "${BOX_CREATED:?}" "${CASES:?}" "${OUT:?}"
SLOTS=${SLOTS:-6}; ADMIT_MIN=${ADMIT_MIN:-100}; END_MIN=${END_MIN:-115}; EST_MIN=${EST_MIN:-60}
created=$(date -d "$BOX_CREATED" +%s) || { echo "run: bad BOX_CREATED" >&2; exit 2; }
secs() { awk -v m="$1" 'BEGIN { printf "%d", m * 60 }'; }
export DEADLINE=$((created + $(secs "$END_MIN")))
admit_until=$((created + $(secs "$ADMIT_MIN")))
est=$(secs "$EST_MIN")
if [[ -e "$OUT" ]]; then echo "run: $OUT exists" >&2; exit 2; fi
mkdir -p "$OUT/outbox"
export OUT SOLAR_DIR SOLAR_SHA LEAN_DIR LEAN_SHA PACKAGES_FROM="${PACKAGES_FROM:-}" SHIP_REMOTE="${SHIP_REMOTE:-}" SHIP_BRANCH="${SHIP_BRANCH:-}"
export PRODUCER="$OUT/producer.json"
log() { echo "$(date -u +%FT%TZ) $*" | tee -a "$OUT/run.log"; }
log "RUN-START solar=$SOLAR_SHA lean=$LEAN_SHA box_created=$BOX_CREATED admit_until=$(date -u -d @$admit_until +%FT%TZ) deadline=$(date -u -d @$DEADLINE +%FT%TZ) slots=$SLOTS"
# Every case needs its intended stage registered in stages.tsv before anything runs.
missing=$(while read -r name _; do
    [[ -z "${name:-}" || "$name" == \#* ]] && continue
    awk -F'\t' -v n="$name" '$1 == n {f = 1} END {exit !f}' "$here/stages.tsv" || echo "$name"
done < "$CASES")
if [[ -n "$missing" ]]; then log "REFUSED: no registered stage for: $missing"; exit 2; fi
log "STAGES $(sha256sum < "$here/stages.tsv" | cut -c1-64)"
if ! bash "$here/producer.sh" "$PRODUCER" > "$OUT/producer.log" 2>&1; then
    log "RUN-ABORT producer failed (see producer.log)"; exit 1
fi
log "PRODUCER $(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["binary_sha256"], d["binary_path"])' "$PRODUCER")"
class_of() { awk -F'\t' -v n="$1" '$1 == n {print $2}' "$here/stages.tsv"; }
# A control that ended without passing its stage (or was killed or refused) stops the run: no
# more cases are admitted and running ones end with KILLED-STOP, no verdict.
check_controls() {
    local n last
    for n in $(grep -v '^#' "$CASES" | awk '{print $1}'); do
        [[ $(class_of "$n") == control && -f "$OUT/$n/status" ]] || continue
        last=$(tail -1 "$OUT/$n/status")
        case "$last" in
            START*) ;;
            DONE*stage_ok=yes*) ;;
            *) if [[ ! -e "$OUT/STOP" ]]; then
                   echo "$(date -u +%FT%TZ) control $n: $last" > "$OUT/STOP"
                   log "STOP control $n failed: $(cut -c1-160 <<< "$last")"
               fi ;;
        esac
    done
}
# One systemd scope per case, MemoryMax CASE_MEMORY_MAX and no swap; case.sh reads both back from
# its own cgroup and refuses otherwise.
launch() {
    local vars=(HOME PATH OUT SOLAR_DIR SOLAR_SHA LEAN_DIR LEAN_SHA PRODUCER DEADLINE PACKAGES_FROM
                SHIP_REMOTE SHIP_BRANCH PHASE) envs=() v
    for v in "${vars[@]}"; do [[ -z "${!v+x}" ]] || envs+=("$v=${!v}"); done
    if [[ -n "${CASE_MEMORY_MAX:-}" ]]; then
        sudo systemd-run --scope --quiet --unit="t5case-$1-$(date +%s)" -p "MemoryMax=$CASE_MEMORY_MAX" \
            -p MemorySwapMax=0 --uid="$(id -u)" --gid="$(id -g)" -- env -i "${envs[@]}" \
            bash "$here/case.sh" "$@"
    else
        bash "$here/case.sh" "$@"
    fi
}
ordered=$( { grep -v '^#' "$CASES" | while read -r n rest; do [[ -n "$n" && $(class_of "$n") == control ]] && echo "$n $rest"; done
             grep -v '^#' "$CASES" | while read -r n rest; do [[ -n "$n" && $(class_of "$n") != control ]] && echo "$n $rest"; done; } )
log "ORDER $(awk '{print $1}' <<< "$ordered" | tr '\n' ' ')"
running=0
while read -r name src expect; do
    [[ -z "${name:-}" || "$name" == \#* ]] && continue
    while (( running >= SLOTS )); do wait -n; running=$((running - 1)); check_controls; done
    check_controls
    if [[ -e "$OUT/STOP" ]]; then
        mkdir -p "$OUT/$name"
        echo "NOT-RUN $(date -u +%FT%TZ) (a control failed: $(cat "$OUT/STOP"))" > "$OUT/$name/status"
        bash "$here/ship.sh" "$name"
        log "NOT-RUN $name"
        continue
    fi
    now=$(date +%s)
    [[ "$src" == /* ]] || src="$here/$src"
    if (( now >= admit_until || now + est > DEADLINE )); then
        mkdir -p "$OUT/$name"
        echo "NOT-ADMITTED $(date -u +%FT%TZ) (admission closes at $(date -u -d @$admit_until +%FT%TZ); a run of ${EST_MIN}m would end after the deadline)" > "$OUT/$name/status"
        bash "$here/ship.sh" "$name"
        log "NOT-ADMITTED $name"
        continue
    fi
    log "ADMIT $name"
    launch "$name" "$src" "$expect" &
    running=$((running + 1))
done <<< "$ordered"
while (( running > 0 )); do wait -n; running=$((running - 1)); check_controls; done
cg=$(sed -n 's/^0:://p' /proc/self/cgroup | head -1)
{
    echo "cgroup $cg"
    echo "memory.max $(cat "/sys/fs/cgroup$cg/memory.max" 2>/dev/null || echo unreadable)"
    echo "memory.peak $(cat "/sys/fs/cgroup$cg/memory.peak" 2>/dev/null || echo unreadable)"
    echo "memory.events $(tr '\n' ' ' < "/sys/fs/cgroup$cg/memory.events" 2>/dev/null)"
} > "$OUT/run-scope.txt"
python3 "$here/table.py" "$OUT" "$CASES" "$here/stages.tsv" > "$OUT/table.md"
log "RUN-DONE"
