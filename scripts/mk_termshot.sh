#!/usr/bin/env bash
# Builds screenshots
#
# Requires: hyperfine, solar, solc, forge, git, termshot
set -eo pipefail

export CLICOLOR=1

git clone https://github.com/vectorized/solady --depth 1
cd solady
REMAPPINGS=$(forge re)
termshot --columns 128 --clip-canvas --no-shadow --filename ../assets/benchmark.png -- "hyperfine -w1 --shell=fish -n solar 'solar $REMAPPINGS {src,test}/**/*.sol --emit abi' -n solc 'solc $REMAPPINGS {src,test}/**/*.sol --combined-json abi'"
cd ..
rm -rf solady
