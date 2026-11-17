#!/usr/bin/env bash
# The any-caller acceptance of the pinned UniswapV2ERC20.transfer chain on fresh bytes.
#
#   tools/certify/erc20_any/run.sh LEAN_DIR SOLAR_STAGES_BIN OUT
#
# LEAN_DIR: solidity-lean at its erc20-gates commit after scripts/verify/uniswap_v2_gen.sh,
# uniswap_v2_transfer_slice.sh and prep.sh. SOLAR_STAGES_BIN: the candidate compiler prep.sh
# pinned (EXPECT_SOLAR_SHA256, EXPECT_SOLAR_COMMIT). OUT: logs and receipts.
# Steps: fresh emit; the dependency module at its pinned digest; gen.py; build; check_final. Each
# step's exit and peak RSS go to OUT/steps. The kernel replay and negatives are replay.sh and neg.sh.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
certify=$(cd "$here/.." && pwd)
lean=$(realpath "$1"); solar=$(realpath "$2"); out=$(realpath -m "$3"); name=erc20_any
mkdir -p "$out"
step() { local n=$1; shift
  /usr/bin/time -v -o "$out/$n.time" "$@" > "$out/$n.log" 2>&1; local rc=$?
  echo "$n exit=$rc peak_rss_kb=$(awk '/Maximum resident/{print $NF}' "$out/$n.time") $(date -u +%FT%TZ)" | tee -a "$out/steps"
  return $rc; }
cd "$lean"
# The producer: only the pinned candidate binary (prep.sh's EXPECT_SOLAR_SHA256 and
# EXPECT_SOLAR_COMMIT) may emit, and its commit must be the stage export's.
: "${EXPECT_SOLAR_SHA256:?pin the candidate binary}" "${EXPECT_SOLAR_COMMIT:?pin the candidate commit}"
[[ $(sha256sum < "$solar" | cut -c1-64) == "$EXPECT_SOLAR_SHA256" ]] || { echo "refused: $solar is not the pinned binary" >&2; exit 1; }
[[ $("$solar" --version | head -1) == *"commit.${EXPECT_SOLAR_COMMIT:0:8}"* ]] || { echo "refused: $solar is not commit $EXPECT_SOLAR_COMMIT" >&2; exit 1; }
grep -q "commit.${EXPECT_SOLAR_COMMIT:0:8}" Certified/uv2_erc20/gen/UniswapV2ERC20/manifest.json || { echo "refused: the stage export is another compiler's" >&2; exit 1; }
mapfile -t opts < Certified/uv2_erc20/options
(cd scripts/verify/uniswap-v2 && "$solar" "${opts[@]}" --emit bin-runtime UniswapV2ERC20.sol) > "$out/fresh.json" || exit 1
echo "solar $(sha256sum < "$solar" | cut -c1-64) $("$solar" --version | head -1)" > "$out/identities"
te=Contracts/UniswapV2/ERC20/Certified/TransferExpected.lean
git cat-file -e 4332b47d3495790581a8d0d4f18f690bd8f3d5a6 2>/dev/null || git fetch -q origin 4332b47d3495790581a8d0d4f18f690bd8f3d5a6
git show 4332b47d3495790581a8d0d4f18f690bd8f3d5a6:$te > $te
echo "8573c6944c24c3d578aaf1f52f127de5053c8e461318dfb3ba71c8fc767c988c  $te" | sha256sum -c --quiet || exit 1
python3 "$here/gen.py" "$lean" "$out/fresh.json" "$name" || exit 1
for f in Certified/$name/*.lean $te scripts/verify/uniswap-v2/UniswapV2ERC20.sol Certified/uv2_erc20/options \
    Certified/uv2_erc20/gen/UniswapV2ERC20/manifest.json Certified/uv2_erc20/gen/UniswapV2ERC20/Manifest.lean; do
  sha256sum "$f"; done > "$out/artifacts.sha256"
LEAN_NUM_THREADS=${LEAN_NUM_THREADS:-1} step build lake build -KleanMemory=12000 "$name" || exit 1
step check_final lake env lean --run "$certify/check_final.lean" "$name" "$name.Proof" || exit 1
echo done
