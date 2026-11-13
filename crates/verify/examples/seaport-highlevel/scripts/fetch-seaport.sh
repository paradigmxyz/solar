#!/usr/bin/env bash
# Fetches Seaport 1.6 at the pinned commit into lib/seaport and builds its
# optimized (deployed) and reference profiles.
set -euo pipefail
cd "$(dirname "$0")/.."
REV=1170198
if [ ! -d lib/seaport ]; then
  git clone -q https://github.com/ProjectOpenSea/seaport.git lib/seaport
fi
cd lib/seaport
git checkout -q "$REV"
git submodule update --init --recursive -q
FOUNDRY_PROFILE=optimized forge build
FOUNDRY_PROFILE=reference forge build
