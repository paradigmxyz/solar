#!/usr/bin/env bash
# c2.sh: phase c2, the controls' independent cold replay. For each control archive (outbox/NAME.tgz
# of a c1 receipts branch, checked against ARCHIVES, lines "<sha256>  NAME.tgz"), unpack it, then
# run tools/certify/certify --replay of its record in its own systemd scope (CASE_MEMORY_MAX, no
# swap; read back inside, refused otherwise), with this machine's freshly fetched package cache
# (PACKAGES_FROM) and solidity-lean (LEAN_DIR). The replay passes only with exit 0, the driver's
# success line (REPLAY_PASS_LINE), and a fresh record whose axioms are exactly
# propext, Classical.choice and Quot.sound with trusted 0. Each case's directory ships to
# $OUT/outbox as it ends. Needs OUT, SOLAR_DIR, SOLAR_SHA, LEAN_DIR, PACKAGES_FROM, ARCHIVE_DIR,
# ARCHIVES, BOX_CREATED; optional ADMIT_MIN (90), END_MIN (105), CASE_MEMORY_MAX, REPLAY_PASS_LINE
# (the driver's replay success wording; default the driver's "a fresh cold run from the same
# pinned source/build inputs gave identical checked outputs").
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
: "${OUT:?}" "${SOLAR_DIR:?}" "${SOLAR_SHA:?}" "${LEAN_DIR:?}" "${PACKAGES_FROM:?}" "${ARCHIVE_DIR:?}" "${ARCHIVES:?}" "${BOX_CREATED:?}"
unset SOLAR CERTIFY_SKIP_BUILD SOLAR_VERIFY CERTIFY_REPLAY_TRUST_UPSTREAM CERTIFY_ALLOW_FOREIGN_SOLAR CERTIFY_ALLOW_UNPINNED CERTIFY_PIN_BASE
mkdir -p "$OUT/outbox"
export REPLAY_PASS_LINE=${REPLAY_PASS_LINE:-a fresh cold run from the same pinned source/build inputs gave identical checked outputs}
log() { echo "$(date -u +%FT%TZ) $*" | tee -a "$OUT/run.log"; }
created=$(date -u -d "$BOX_CREATED" +%s)
admit_until=$((created + ${ADMIT_MIN:-90} * 60))
export DEADLINE=$((created + ${END_MIN:-105} * 60))
driver=$(git -C "$SOLAR_DIR" rev-parse HEAD)
if [[ "$driver" != "$SOLAR_SHA" || -n "$(git -C "$SOLAR_DIR" status --porcelain --untracked-files=no)" ]]; then
    log "REFUSED-DRIVER $driver"; exit 2
fi
log "C2-START driver=$driver harness=$(git -C "$here" rev-parse HEAD) admit_until=$(date -u -d @$admit_until +%FT%TZ) deadline=$(date -u -d @$DEADLINE +%FT%TZ) CERTIFY_ALLOW_UNPINNED=${CERTIFY_ALLOW_UNPINNED-unset} CERTIFY_ALLOW_FOREIGN_SOLAR=${CERTIFY_ALLOW_FOREIGN_SOLAR-unset}"
(cd "$ARCHIVE_DIR" && sha256sum -c "$ARCHIVES") > "$OUT/archives.check" 2>&1 || { log "REFUSED-ARCHIVES $(tr '\n' ' ' < "$OUT/archives.check")"; exit 2; }
log "PASS-LINE $REPLAY_PASS_LINE"
log "ARCHIVES ok $(sha256sum < "$ARCHIVES" | cut -c1-64)"
pids=()
while read -r _ tgz; do
    name=${tgz%.tgz}
    if (( $(date +%s) >= admit_until )); then
        mkdir -p "$OUT/$name"; echo "NOT-ADMITTED $(date -u +%FT%TZ)" > "$OUT/$name/status"; continue
    fi
    mkdir -p "$OUT/$name"
    tar -xzf "$ARCHIVE_DIR/$tgz" -C "$OUT/$name" "$name/rec" --strip-components=1
    log "ADMIT $name"
    vars=(HOME PATH OUT SOLAR_DIR LEAN_DIR PACKAGES_FROM DEADLINE REPLAY_PASS_LINE)
    envs=(); for v in "${vars[@]}"; do envs+=("$v=${!v}"); done
    if [[ -n "${CASE_MEMORY_MAX:-}" ]]; then
        sudo systemd-run --scope --quiet --unit="t5c2-$name-$(date +%s)" -p "MemoryMax=$CASE_MEMORY_MAX" -p MemorySwapMax=0 \
            --uid="$(id -u)" --gid="$(id -g)" -- env -i "${envs[@]}" bash "$here/c2case.sh" "$name" &
    else
        bash "$here/c2case.sh" "$name" &
    fi
    pids+=($!)
done < "$ARCHIVES"
for p in "${pids[@]}"; do wait "$p"; done
cg=$(sed -n 's/^0:://p' /proc/self/cgroup | head -1)
{ echo "cgroup $cg"; echo "memory.max $(cat "/sys/fs/cgroup$cg/memory.max" 2>/dev/null)"; echo "memory.peak $(cat "/sys/fs/cgroup$cg/memory.peak" 2>/dev/null)"; echo "memory.events $(tr '\n' ' ' < "/sys/fs/cgroup$cg/memory.events" 2>/dev/null)"; } > "$OUT/run-scope.txt"
for d in "$OUT"/*/; do n=$(basename "$d"); [[ $n == outbox ]] && continue; echo "$n $(tail -1 "$d/status")"; done > "$OUT/table.txt"
log "C2-DONE"
