#!/usr/bin/env bash
# Regression check, not part of certification: after scripts/certify NAME has run in LEAN_DIR,
# the files it regenerated for CERTIFIED/NAME must be the ones solidity-lean commits, up to the
# compiler's commit in the manifest. A difference means the exporter's or the generator's
# output moved: the stages and proofs are proof inputs, so certification still stands on what
# the run built, and this check only reports that the committed copy is stale. The run's
# record (record.json) is a report, not a regenerated input, and is not compared.
#
#   tools/certify/regenerated.sh LEAN_DIR NAME [EXAMPLES]
set -euo pipefail

die() { echo "regenerated: error: $*" >&2; exit 1; }
[[ $# -ge 2 && $# -le 3 ]] || die "usage: regenerated.sh LEAN_DIR NAME [EXAMPLES]"
lean_dir=$1 name=$2 examples=${3:-${CERTIFY_EXAMPLES:-Certified}}
cd "$lean_dir"
dir="$examples/$name"
record=".lake/certify-$name.json"
[[ -f "$record" ]] || die "no $record: run scripts/certify on $name first"
contract="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["contract"])' "$record")"
gen="$dir/gen/$contract"
if ! git ls-files --error-unmatch "$dir/bytecode.hex" > /dev/null 2>&1; then
    echo "$dir is not committed in solidity-lean"
    exit 0
fi
if [[ -f lakefile.lean ]]; then lakefile=lakefile.lean; else lakefile=lakefile.toml; fi
# Porcelain paths start at the repository's root, which a package in a subdirectory is not.
prefix="$(git rev-parse --show-prefix)"
others="$(git status --porcelain -- "$dir" "$lakefile" |
    grep -v -E " $prefix($gen/(Manifest\.lean|manifest\.json)|$dir/record\.json)$" || true)"
[[ -z "$others" ]] || { echo "$others" >&2; git diff --stat -- "$dir" "$lakefile" >&2; die "regenerated files differ from solidity-lean's committed ones"; }
# Every line but the compiler's version, in either file, must be the committed one; the options'
# thread count, which solar takes from the machine and which changes no output, is compared as N.
norm() { grep -v -E '^(def compiler : String := "[^"]*"|  "compiler": "[^"]*",)$' |
    sed -E 's/threads: [0-9]+/threads: N/'; }
changed=
for f in "$gen/Manifest.lean" "$gen/manifest.json"; do
    git show "HEAD:./$f" | norm > .lake/certify-committed
    norm < "$f" > .lake/certify-regenerated
    changed+="$(diff .lake/certify-committed .lake/certify-regenerated || true)"
done
rm -f .lake/certify-committed .lake/certify-regenerated
[[ -z "$changed" ]] || { echo "$changed" >&2; die "the manifest differs in more than the compiler's commit"; }
echo "every file of $dir regenerates to the committed one"
