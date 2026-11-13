#!/usr/bin/env bash
# Certifies a contract whose backend loops: compiles it with -Zemit-lean-stages, checks that the
# export leaves the bytecode as it is, writes the cut-point proofs of every stage with
# cutpoints.py (the relations at loop headers derived from the compiler's records, never
# written by hand) and builds them one module at a time.
#
#   solar-tv/milestone/certify-loops.sh NAME CONTRACT [-- SOLAR_OPTIONS...]
#
# NAME/NAME.sol is the source and NAME a [[lean_lib]] of solar-tv/lakefile.toml; the generated
# gen/ and Proofs/ are not committed.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
name=$1 contract=$2
shift 2
[[ "${1:-}" == "--" ]] && shift
options=("$@")
[[ ${#options[@]} -gt 0 ]] || options=(-Ogas --evm-version osaka)
export LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-1}"
solar="${SOLAR:-${CARGO_TARGET_DIR:-$root/target}/debug/solar}"
cd "$here/$name"
plain="$("$solar" "$name.sol" "${options[@]}" --emit=bin-runtime 2>/dev/null)"
rm -rf gen
exported="$("$solar" "$name.sol" "${options[@]}" --emit=bin-runtime -Zemit-lean-stages=gen 2>/dev/null)"
[[ "$plain" == "$exported" ]] || { echo "error: -Zemit-lean-stages changed the bytecode" >&2; exit 1; }
final="$(python3 "$here/cutpoints.py" "$name" "$contract")"
cd "$here/.."
for module in $(cd milestone && ls "$name"/gen/"$contract"/*.lean "$name"/Proofs/Runs/*.lean | sed 's|\.lean$||; s|/|.|g') "$name.Proofs"; do
    printf '%s: ' "$module"
    lake -KleanMemory="${LEAN_MEMORY:-3000}" build "$module" > .lake/certify-loops.log 2>&1 ||
        { grep -E '^error' -A12 .lake/certify-loops.log | head -40 >&2; echo "error: $module does not check" >&2; exit 1; }
    tail -n 1 .lake/certify-loops.log
done
echo "$final proved, its axioms pinned to propext, Classical.choice and Quot.sound"
