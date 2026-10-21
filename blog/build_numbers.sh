#!/bin/sh
# Regenerates blog/solar-blog-numbers.md from the raw results.
set -e
cd "$(dirname "$0")/.."
E=blog/solar-blog-evidence
X=blog/box
{
  cat blog/numbers-intro.md
  echo
  cat blog/numbers-safe-solady.md
  echo
  echo "# Full tables"
  echo
  uv run -q blog/make_tables.py \
    "B1: box, shared config, solc/solx/oksolc/Solar=$X/shared-all-results.json" \
    "B2: box, shared config, Slang preview 5e6ec5e1/Solar=$X/shared-slang-results.json" \
    "B2-old (superseded): box, shared config, Slang preview 62d94366/Solar=$X/shared-slang-62d94366-results.json" \
    "B3: box, shared config, Solar -j1=$X/shared-solar-j1-results.json" \
    "B4: box, original config, solc/solx/oksolc/Solar=$X/original-all-results.json" \
    "B5: box, original config, Solar -j1, whole projects=$X/original-solar-j1-results.json" \
    "CI: run 39567201442, original config=$E/ci-39567201442-results.json" \
    "L1 (superseded): dev box, original config=$E/local1-results.json" \
    "L2 (superseded): dev box, original config, Slang preview=$E/local2-results.json" \
    "L3 (superseded): dev box, Solar -j1=$E/local3-results.json" \
    "L4 (superseded): dev box, Cancun only, solc/solx 0.1.8/Solar=$E/local4-results.json" \
    "L5 (superseded): dev box, Cancun only, Slang preview/Solar=$E/local5-results.json"
  echo
  echo "## Safe Solady: upstream suites, per suite"
  echo
  cat $E/safe-solady-upstream-suites-gas.md
  echo
  echo "## Safe Solady: differential matrix report"
  echo
  sed 's/^# /### /' $E/safe-solady-matrix-report.md
  echo
  echo "## Safe Solady: differential matrix, per function"
  echo
  sed 's/^# /### /' $E/safe-solady-matrix-api-gas.md
} > blog/solar-blog-numbers.md
