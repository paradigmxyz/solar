#!/usr/bin/env bash
# Negative tests of the cut-point proofs: mutates the loop of a milestone's EVM IR stage and
# requires the generated proof of that stage to fail by mismatch (sym_seg's message), never by
# running out of time or memory, with the unmutated stage checking first.
#
#   solar-tv/milestone/cutpoint-negative.sh NAME CONTRACT STAGE 'OLD=>NEW'...
#
# Each mutation replaces the last occurrence of OLD in the stage's file with NEW.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
name=$1 contract=$2 stage=$3
shift 3
file="$here/$name/gen/$contract/$stage.lean"
backup="$(mktemp)"
cp "$file" "$backup"
trap 'cp "$backup" "$file"; rm -f "$backup"' EXIT
cd "$here/.."
build() { lake -KleanMemory="${LEAN_MEMORY:-3000}" build "$name.Proofs.Runs.$stage" 2>&1; }
build > /dev/null || { echo "error: the unmutated $stage does not check" >&2; exit 1; }
echo "$stage: the unmutated stage checks"
for mutation in "$@"; do
    cp "$backup" "$file"
    python3 - "$file" "$mutation" <<'EOF'
import sys
path, mutation = sys.argv[1], sys.argv[2]
old, new = mutation.split("=>")
text = open(path).read()
i = text.rindex(old)
open(path, "w").write(text[:i] + new + text[i + len(old):])
EOF
    out="$(build || true)"
    if grep -q "sym_seg: the segments end differently" <<<"$out" &&
        ! grep -q -E "heartbeats|timeout|out of memory" <<<"$out"; then
        echo "$stage with $mutation: fails by mismatch"
    else
        echo "$out" | grep -E '^error' | head -5 >&2
        echo "error: $stage with $mutation does not fail by mismatch" >&2
        exit 1
    fi
done
