#!/usr/bin/env bash
# Compares every deployable contract of the runtime corpora with solsymdiff and
# fails only when a replayed mismatch is found.
set -euo pipefail

workspace="${GITHUB_WORKSPACE:-$(pwd)}"
solc="${SOLC:-solc}"
solar="${SOLAR:-$workspace/target/debug/solar}"
forge="${FORGE:-forge}"
solver="${SOLVER:-z3}"
out="${SOLSYMDIFF_OUT:-$workspace/target/solsymdiff-campaign}"
jobs="${SOLSYMDIFF_JOBS:-4}"

cd "$workspace"
shopt -s nullglob
sources=(
  testdata/Arithmetic.sol
  testdata/Counter.sol
  testdata/Factorial.sol
  testdata/MinimalProxy.sol
  testdata/Solady.sol
  testdata/SumArray.sol
  # Algorithms, Encoding, and P256 import library sources that are only
  # available as project snapshots, so they do not compile on their own.
  testdata/runtime/CalldataLoops.sol
  testdata/runtime/CompilerOptimizations.sol
  testdata/runtime/CounterLoop.sol
  testdata/runtime/DivisionWords.sol
  testdata/runtime/SeededWords.sol
  testdata/runtime/VerifiedWords.sol
  testdata/runtime/WordRecipes.sol
  fuzz/fandango/runtime-corpus/*.sol
  fuzz/fandango/runtime-regressions/*.sol
  fuzz/fandango/symbolic-fixtures/*.sol
)

python3 fuzz/fandango/run_symbolic_campaign.py "${sources[@]}" \
  --out "$out" --solc "$solc" --jobs "$jobs" -- \
  --solar "$solar" --forge "$forge" --solver "$solver" --symbolic-caller
