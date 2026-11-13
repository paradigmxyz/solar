#!/usr/bin/env bash
# Writes solidity-lean's Contracts/ArrayMax/Inferred.lean: the invariant and measure of
# ArrayMax.max's loop that solar-verify infers for crates/verify/examples/ArrayMax.sol, carried to
# the loop's header in ArrayMaxEntry's first lowered MIR (where solar inlines the library
# function) as the exporter wrote it (Certified/arraymax's Stage0.lean) by ../inferred.py. The
# cut-point proof of ArrayMaxEntry.max (Contracts/ArrayMax/Entry.lean) checks it.
#
# Usage: inferred.sh SOLAR_VERIFY STAGE0.lean OUT.lean
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
src=$root/crates/verify/examples/ArrayMax.sol
inferred=$(mktemp)
trap 'rm -f "$inferred"' EXIT
"$1" --show-inferred "$src" >"$inferred"
python3 "$here/../inferred.py" "$2" "$src" "$inferred" max "$3" \
    SolarTV.Milestone.ArrayMaxEntry.Inferred arraymax.gen.ArrayMaxEntry.Stage0
