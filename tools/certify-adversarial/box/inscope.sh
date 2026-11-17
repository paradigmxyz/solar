#!/usr/bin/env bash
# box/inscope.sh: run by go.sh inside its scope. Admits the scope with the driver's box_admit.sh
# (memory.max reads back as ADMIT_WANT, swap.max as 0, this shell in the scope) and only then runs
# run.sh; anything it cannot read refuses.
set -uo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
adm="$OUT.admission"
cg=$(sed -n 's/^0:://p' /proc/self/cgroup | head -1)
{
    echo "cgroup $cg"
    echo "pid $$"
    echo "want_memory_max $ADMIT_WANT"
    echo "memory.max $(cat "/sys/fs/cgroup$cg/memory.max" 2>&1)"
    echo "memory.swap.max $(cat "/sys/fs/cgroup$cg/memory.swap.max" 2>&1)"
} > "$adm"
if ! bash "$SOLAR_DIR/tools/certify/box_admit.sh" admit "$cg" "$$" "$ADMIT_WANT" >> "$adm" 2>&1; then
    echo "ADMISSION-REFUSED $(date -u +%FT%TZ)" >> "$adm"
    exit 1
fi
echo "ADMITTED $(date -u +%FT%TZ)" >> "$adm"
exec bash "$here/run.sh"
