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
running=0
while read -r name src expect; do
    [[ -z "${name:-}" || "$name" == \#* ]] && continue
    while (( running >= SLOTS )); do wait -n; running=$((running - 1)); done
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
    bash "$here/case.sh" "$name" "$src" "$expect" &
    running=$((running + 1))
done < "$CASES"
wait
python3 "$here/table.py" "$OUT" "$CASES" "$here/stages.tsv" > "$OUT/table.md"
log "RUN-DONE"
