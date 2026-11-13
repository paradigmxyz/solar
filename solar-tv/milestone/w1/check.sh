#!/usr/bin/env bash
# Checks w1 end to end on the compiler's side: compiles w1.sol, regenerates and checks the
# exported stages, builds the kernel proofs of every backend stage and runs the negative tests.
#
# 1. gen.sh builds solar, compiles w1.sol with the pinned options, rewrites gen/ with
#    -Zemit-lean-stages, checks the bytecode (with and without the flag, and the pinned 68
#    bytes), the manifest's hashes, and builds the stages and their sanity checks.
# 2. The regenerated stages must be the committed ones the proofs check, byte for byte. The
#    manifest may differ only in the compiler's commit, and both commits must name the
#    checkout's compiler sources; the committed manifest is then restored.
# 3. milestone/certify.py regenerates the proofs, which must be the committed ones, and lake
#    builds W1Proofs: stage0 -> sched -> evm1 -> evm2 -> evm3 -> evm4 composed into
#    w1_mir_to_evmir, and evm4 against the bytecode by the assembly walk, each pinned by
#    #guard_msgs to its axioms (no sorryAx, no Lean.ofReduceBool).
# 4. negative.sh: meaning-changing edits of the bytecode and of stage outputs fail with a
#    mismatch.
#
# CARGO_TARGET_DIR, CARGO_BUILD_JOBS (default 2), W1_SKIP_BUILD and SOLAR apply to gen.sh;
# LEAN_NUM_THREADS defaults to 2 and LEAN_MEMORY (MB, default 3000) caps the negative tests'
# Lean processes, as the W1Proofs library's lakefile entry caps its build's.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
cd "$root"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-2}"
gen=solar-tv/milestone/w1/gen

if ! git diff --quiet HEAD -- "$gen"; then
    echo "error: $gen has uncommitted changes" >&2
    exit 1
fi

echo "== 1. compile and regenerate the stages"
solar-tv/milestone/w1/gen.sh

echo "== 2. the regenerated stages are the committed ones"
changed="$(git diff --name-only HEAD -- "$gen")"
for file in $changed; do
    case "$file" in
        "$gen/Main/Manifest.lean" | "$gen/Main/manifest.json") ;;
        *)
            echo "error: the compiler's $file differs from the committed one the proofs check:" >&2
            git diff --stat HEAD -- "$gen" >&2
            exit 1
            ;;
    esac
done
if [[ -n "$changed" ]]; then
    others="$(git diff -U0 HEAD -- "$gen/Main/Manifest.lean" "$gen/Main/manifest.json" |
        grep '^[-+] ' | grep -v 'compiler' || true)"
    if [[ -n "$others" ]]; then
        echo "error: the manifest differs in more than the compiler's commit:" >&2
        echo "$others" >&2
        exit 1
    fi
    committed="$(sed -n 's/.*"compiler": ".*+commit\.\([0-9a-f]\{8\}\).*/\1/p' \
        <(git show "HEAD:$gen/Main/manifest.json"))"
    if ! git diff --quiet "$committed" HEAD -- crates Cargo.toml Cargo.lock; then
        echo "error: the committed manifest's compiler commit $committed has other sources" >&2
        exit 1
    fi
    git checkout -- "$gen/Main/Manifest.lean" "$gen/Main/manifest.json"
    echo "stages unchanged; the compiler's commit differs from the manifest's $committed only"
    echo "in commits that leave the compiler sources as they are"
else
    echo "stages and manifest unchanged"
fi

echo "== 3. the kernel proofs of the backend stages"
# The proofs are certify.py's, as for any contract; they must regenerate to the committed ones.
python3 solar-tv/milestone/certify.py "$gen/Main" --gen-module gen.Main \
    --proof-dir solar-tv/milestone/w1/proofs/W1Proofs --proof-module W1Proofs \
    --namespace SolarTV.Milestone.W1 --label w1 --axioms "$(cat <<'EOF'
{"stage0_ends": "[propext, Classical.choice, Quot.sound]",
 "sched_run": "[propext, Classical.choice, Quot.sound]",
 "evm1_run": "[propext, Classical.choice, Quot.sound]",
 "evm2_run": "[propext, Classical.choice, Quot.sound]",
 "evm3_run": "[propext, Quot.sound]", "evm4_run": "[propext, Quot.sound]"}
EOF
)" > /dev/null
if ! git diff --quiet HEAD -- solar-tv/milestone/w1/proofs; then
    git diff --stat HEAD -- solar-tv/milestone/w1/proofs >&2
    echo "error: certify.py does not regenerate the committed proofs" >&2
    exit 1
fi
(cd solar-tv && lake build W1Proofs)
cat > solar-tv/.lake/w1-axioms.lean <<'EOF'
import W1Proofs
#check @SolarTV.Milestone.W1.w1_mir_to_evmir
#print axioms SolarTV.Milestone.W1.w1_mir_to_evmir
#print axioms SolarTV.Milestone.W1.walk_eq
#print axioms SolarTV.Milestone.W1.decodes
EOF
(cd solar-tv && lake env lean -M "${LEAN_MEMORY:-3000}" .lake/w1-axioms.lean)

echo "== 4. negative tests"
solar-tv/milestone/w1/proofs/negative.sh
echo "w1: all checks passed"
