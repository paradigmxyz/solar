#!/usr/bin/env bash
# Writes solidity-lean's Contracts/VerifyCounter/Inferred.lean: the invariant and measure of
# bump's loop that solar-verify infers for crates/verify/examples/Counter.sol, carried to the loop's
# header in the first lowered MIR the exporter wrote (verify_counter's Stage0.lean) by
# ../inferred.py. The cut-point proof of bump (Contracts/VerifyCounter/Bump.lean) checks it.
#
# Usage: inferred.sh SOLAR_VERIFY STAGE0.lean OUT.lean
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
src=$root/crates/verify/examples/Counter.sol
inferred=$(mktemp)
trap 'rm -f "$inferred"' EXIT
"$1" --show-inferred "$src" >"$inferred"
python3 "$here/../inferred.py" "$2" "$src" "$inferred" bump "$3" \
    SolarTV.Milestone.VerifyCounter.Inferred verify_counter.gen.Counter.Stage0
