#!/usr/bin/env bash
# Checks the bytes behind every certified milestone, and that they rebuild, with a summary.
#
#   scripts/verify-repro.sh [--clean-builds] [--lean] [--solidity-lean DIR] [--skip-corpus] [--keep]
#                           [MILESTONE...]
#
# Builds solar and solar-tv-replay in release mode (unless SOLAR and SOLAR_TV_REPLAY name
# binaries), then runs scripts/verify_repro.py, whose docstring lists the checks:
#
# - neutrality: -Zemit-lean-stages leaves the bin and bin-runtime output of every codegen UI
#   test, milestone and example as it is at -Ogas and -Osize, a rejected export writes no
#   output and no manifest, and an accepted export is the same with one thread;
# - pin, manifest, literal, deploy: each milestone's pinned bytecode is what solar emits, what
#   its manifests bind with the source, options and stage hashes, the literal its assembly
#   theorem reads, and the code evm2 installs when it runs solar's creation code;
# - commit: the manifest's compiler commit exists and has the checkout's compiler sources;
# - regen: certify.sh's export and certify.py rerun in a clean worktree regenerate every
#   committed file byte for byte, but for the compiler's version and commit.
#
# --clean-builds builds solar twice, in two new target directories under target/verify-repro,
# and runs the corpus and the regeneration with both: the two builds must emit the same bytes
# and regenerate the same files. --lean also builds each milestone's proofs with lake (one Lean
# process at a time, LEAN_MEMORY MB each, default 3000) and checks with Lean that the assembly
# theorem reads the exported bytecode, that #eval of it is the pin, and the theorems' axioms.
# --solidity-lean DIR checks that each milestone the solidity-lean checkout DIR imports was pinned,
# at the solar commit its lake manifest names, to the bytecode solar emits now. --keep keeps the
# regenerated milestones in target/verify-repro/regen-N.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}" LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-1}"

clean=
args=()
for arg in "$@"; do
    case "$arg" in
        --clean-builds) clean=1 ;;
        *) args+=("$arg") ;;
    esac
done

if ! git diff --quiet HEAD -- crates Cargo.toml Cargo.lock; then
    echo "error: the compiler sources have uncommitted changes" >&2
    exit 2
fi

target="${CARGO_TARGET_DIR:-$root/target}"
solars=()
if [[ -n "$clean" ]]; then
    for build in a b; do
        dir="$root/target/verify-repro/build-$build"
        rm -rf "$dir"
        echo "== clean build $build in $dir"
        CARGO_TARGET_DIR="$dir" cargo build --locked --release --bin solar 2>&1 | tail -n 1
        solars+=("$dir/release/solar")
    done
elif [[ -n "${SOLAR:-}" ]]; then
    solars+=("$SOLAR")
else
    echo "== build solar"
    cargo build --locked --release --bin solar 2>&1 | tail -n 1
    solars+=("$target/release/solar")
fi
replay="${SOLAR_TV_REPLAY:-}"
if [[ -z "$replay" ]]; then
    echo "== build solar-tv-replay"
    cargo build --locked --release -p solar-tv-replay 2>&1 | tail -n 1
    replay="$target/release/solar-tv-replay"
fi

# Each binary must report a commit with the checkout's compiler sources, as certify.sh requires.
for solar in "${solars[@]}"; do
    commit="$("$solar" --version | sed -n 's/^Commit SHA: //p')"
    git diff --quiet "$commit" HEAD -- crates Cargo.toml Cargo.lock 2>/dev/null || {
        echo "error: $solar reports commit $commit, whose compiler sources are not HEAD's;" >&2
        echo "       rebuild it (touch crates/config/build.rs)" >&2
        exit 2
    }
done

flags=()
for solar in "${solars[@]}"; do flags+=(--solar "$solar"); done
exec python3 scripts/verify_repro.py "${flags[@]}" --replay "$replay" \
    --lean-memory "${LEAN_MEMORY:-3000}" ${args[@]+"${args[@]}"}
