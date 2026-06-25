#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/run_tree_crasher.sh [args...]

# https://github.com/langston-barrett/tree-crasher/
# $ cargo install --git https://github.com/langston-barrett/tree-crasher tree-crasher-solidity

solar=${solar:-solar}
tree_crasher_solidity=${tree_crasher_solidity:-"tree-crasher-solidity"}

$tree_crasher_solidity -v --interesting-exit-code 101 ./corpus/ --output ./tree-crasher.out/ -j8 -- \
    $solar -j1 - "$@"
