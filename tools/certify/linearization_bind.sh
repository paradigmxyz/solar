#!/usr/bin/env bash
# Binds Certified/NAME/Linearization.lean, the C3 theorem of an inherited contract, to the run's
# Solidity source (scripts/certify step 6b, after source_bind.sh).
#
#   tools/certify/linearization_bind.sh LEAN_DIR SOURCE.sol DIR NAME CONTRACT
#
# Regenerates, with the solidity-lean checkout's pinned solc (as source_bind.sh finds it), the
# module generic.py writes for CONTRACT of SOURCE.sol (tools/certify/linearization.py: its
# theorem linearization_eq states that the Lean frontend's C3 order of CONTRACT over NAME.Source,
# which source_bind.sh binds to SOURCE.sol, is solc's linearizedBaseContracts). It fails closed:
# a contract with bases needs DIR/Linearization.lean (DIR a path in LEAN_DIR) to be exactly the
# fresh text, a contract without bases needs it absent, and a contract with bases whose order the
# driver does not certify, or that solc cannot read, fails. On success it prints one line,
# "linearization: ..." with the module to build, or "linearization: none: ..." when there is none.
set -euo pipefail
die() { echo "linearization_bind: error: $*" >&2; exit 1; }
[[ $# -eq 5 ]] || die "usage: linearization_bind.sh LEAN_DIR SOURCE.sol DIR NAME CONTRACT"
lean_dir=$1 sol=$2 dir=$3 name=$4 contract=$5
gen="$lean_dir/scripts/refinement/gen.sh"
[[ -f "$gen" ]] || die "$gen is missing"
version="$(sed -n 's/^solc_version=//p' "$gen")"
sha="$(sed -n 's/^solc_sha256=//p' "$gen")"
[[ -n "$version" && -n "$sha" ]] || die "cannot read the pinned solc from $gen"
solc="$(printenv SOLC || true)"
if [[ -z "$solc" ]]; then
    for c in "$lean_dir/.cache/solc-$version" "$HOME/.cache/solc/solc-$version"; do
        if [[ -x "$c" ]]; then solc=$c; break; fi
    done
fi
[[ -n "$solc" && -x "$solc" ]] || die "no solc $version (set SOLC)"
echo "$sha  $solc" | sha256sum -c --quiet - || die "$solc is not the pinned solc $version"
file="$dir/Linearization.lean"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
python3 "$(dirname "$0")/linearization.py" "$sol" "$name" "$contract" "$solc" > "$tmp/fresh" ||
    die "cannot regenerate the C3 theorem of $contract in $sol (above)"
if [[ ! -s "$tmp/fresh" ]]; then
    [[ ! -e "$lean_dir/$file" ]] || die "$file exists, but $contract has no bases"
    echo "linearization: none: $contract has no bases"
    exit 0
fi
[[ -f "$lean_dir/$file" ]] || die "$contract has bases, and $file (its C3 theorem) is missing"
if ! cmp -s "$lean_dir/$file" "$tmp/fresh"; then
    { diff "$lean_dir/$file" "$tmp/fresh" || true; } | head -20 >&2
    die "$file is not the C3 theorem of $contract in $sol (solc $version)"
fi
echo "linearization: $file is the C3 theorem of $contract in $sol (solc $version): $name.Linearization"
