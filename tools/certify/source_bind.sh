#!/usr/bin/env bash
# Binds a checked-in SolidCore source AST to the run's Solidity source (scripts/certify step 6b).
#
#   tools/certify/source_bind.sh LEAN_DIR SOURCE.sol FILE NAMESPACE CONTRACT
#
# Regenerates, with the solidity-lean checkout's own front end (scripts/solc_ast_to_lean_source.py)
# and its pinned solc (the version and SHA-256 scripts/refinement/gen.sh names; SOLC, else the
# checkout's .cache, else ~/.cache/solc), the Lean AST of CONTRACT in SOURCE.sol, and fails unless
# FILE (a path in LEAN_DIR) is exactly that output: the same text after its first comment line
# and with the importedSourceName label (the path it was generated from) set aside, under
# "import SolidCore.Solidity.Checked" and "open SolidCore.Solidity". The front end stays trusted
# (the source model's trust boundary); what this removes is a stale or hand-edited AST.
set -euo pipefail
die() { echo "source_bind: error: $*" >&2; exit 1; }
[[ $# -eq 5 ]] || die "usage: source_bind.sh LEAN_DIR SOURCE.sol FILE NAMESPACE CONTRACT"
lean_dir=$1 sol=$2 file=$3 ns=$4 contract=$5
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
[[ -f "$lean_dir/$file" ]] || die "$file is not in $lean_dir"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
python3 "$lean_dir/scripts/solc_ast_to_lean_source.py" "$sol" --contract "$contract" --solc "$solc" \
    --namespace "$ns" --body-only > "$tmp/body" || die "the front end cannot read $sol"
head -n 1 "$lean_dir/$file" | grep -q '^-- Generated' || die "$file does not start with its generator's comment"
tail -n +2 "$lean_dir/$file" | grep -v '^def importedSourceName : String := ' > "$tmp/checked" || true
{ printf '\nimport SolidCore.Solidity.Checked\n\nopen SolidCore.Solidity\n\n'
  grep -v '^def importedSourceName : String := ' "$tmp/body" || true; } > "$tmp/fresh"
if ! cmp -s "$tmp/checked" "$tmp/fresh"; then
    diff "$tmp/checked" "$tmp/fresh" | head -20 >&2
    die "$file is not the front end's AST of $contract in $sol"
fi
echo "source: $file is the front end's AST of $contract in $sol (solc $version)"
