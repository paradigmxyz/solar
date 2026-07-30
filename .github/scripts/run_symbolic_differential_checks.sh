#!/usr/bin/env bash
set -euo pipefail

solc_version="${SOLC_VERSION:?SOLC_VERSION must be set}"
workspace="${GITHUB_WORKSPACE:-$(pwd)}"
runner_temp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
solc_path="$runner_temp/solc/solc-$solc_version"
solar_path="$workspace/target/debug/solar"
artifact_dir="${FANDANGO_SYMBOLIC_ARTIFACT_DIR:-$workspace/target/symbolic-differential}"
case "$solc_version" in
  0.8.36)
    solc_sha256="c8d35afdddc3cd2743ee88b8f25e0fecd16e2bdd5f2120f37e52cd9cc45ae0e6"
    ;;
  *)
    echo "no pinned solc checksum for version $solc_version" >&2
    exit 1
    ;;
esac

cd "$workspace"
mkdir -p "$runner_temp/solc" "$artifact_dir"

curl -fsSL \
  --retry 3 \
  --retry-delay 2 \
  "https://github.com/ethereum/solidity/releases/download/v${solc_version}/solc-static-linux" \
  -o "$solc_path"
printf '%s  %s\n' "$solc_sha256" "$solc_path" | sha256sum --check -
chmod +x "$solc_path"

"$solc_path" --version
"$solar_path" --version
forge --version
anvil --version
z3 --version
forge test --help > "$runner_temp/forge-test-help.txt"
grep -q -- "--symbolic" "$runner_temp/forge-test-help.txt"
grep -q -- "--replay-symbolic-artifact" "$runner_temp/forge-test-help.txt"

FANDANGO_SYMBOLIC_E2E=1 \
  FANDANGO_SOLC="$solc_path" \
  FANDANGO_SOLAR="$solar_path" \
  FANDANGO_FORGE="$(command -v forge)" \
  FANDANGO_ANVIL="$(command -v anvil)" \
  FANDANGO_Z3="$(command -v z3)" \
  FANDANGO_SYMBOLIC_ARTIFACT_DIR="$artifact_dir" \
  python3 -m unittest fuzz/fandango/test_symbolic_differential.py -v
