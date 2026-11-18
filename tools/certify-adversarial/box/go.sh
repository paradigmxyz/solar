#!/usr/bin/env bash
# box/go.sh MEMORY_MAX_BYTES: starts run.sh in a transient systemd scope whose memory.max is
# MEMORY_MAX_BYTES and whose memory.swap.max is 0, and starts nothing unless the scope reads both
# back (the driver's own tools/certify/box_admit.sh admit, which never accepts "max"). The read-back
# is written to $OUT.admission. Every other variable run.sh needs comes from the environment.
set -euo pipefail
want=$1
here=$(cd "$(dirname "$0")/.." && pwd)
: "${OUT:?}" "${SOLAR_DIR:?}"
[[ "$want" =~ ^[1-9][0-9]*$ ]] || { echo "go: MEMORY_MAX_BYTES must be a byte count" >&2; exit 2; }
vars=(HOME PATH OUT SOLAR_DIR SOLAR_SHA LEAN_DIR LEAN_SHA BOX_CREATED CASES PACKAGES_FROM SLOTS ADMIT_MIN
      END_MIN EST_MIN CARGO_BUILD_JOBS SHIP_BRANCH CASE_MEMORY_MAX)
envs=()
for v in "${vars[@]}"; do [[ -z "${!v+x}" ]] || envs+=("$v=${!v}"); done
exec sudo systemd-run --scope --quiet --unit="t5neg-$(date +%s)" -p "MemoryMax=$want" -p MemorySwapMax=0 \
    --uid="$(id -u)" --gid="$(id -g)" -- env -i "${envs[@]}" ADMIT_WANT="$want" bash "$here/box/inscope.sh"
