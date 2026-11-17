#!/usr/bin/env bash
# Regenerate the candidate-dependent inputs of the pinned UniswapV2ERC20.transfer chain from one
# candidate compiler, and refuse any other producer.
#
#   tools/certify/erc20_any/prep.sh LEAN_DIR SOLAR_BIN TOOLS_DIR OUT
#
# LEAN_DIR: solidity-lean at the erc20-gates commit, after scripts/verify/uniswap_v2_gen.sh (the
# port and the flattened source). SOLAR_BIN: the candidate compiler. TOOLS_DIR: the solar checkout
# whose tools/certify generates the stage proofs (it needs solar-tv/milestone/cutpoints.py).
# EXPECT_SOLAR_SHA256 and EXPECT_SOLAR_COMMIT (both required) pin the candidate: the binary's
# sha256 and the commit its --version names must equal them, before and after every step.
# Writes the stage export (Certified/uv2_erc20/gen), CtxGas.lean, ProofsT/, Ctx.lean and
# CtxNeg.lean, then OUT/producer (path, sha256, commit, version, flags, tools commit),
# OUT/inputs.sha256 (every generated or regenerated file the build reads) and OUT/prep-steps.
set -uo pipefail
lean=$(realpath "$1"); solar=$(realpath "$2"); tools=$(realpath "$3"); out=$(realpath -m "$4")
: "${EXPECT_SOLAR_SHA256:?pin the candidate binary}" "${EXPECT_SOLAR_COMMIT:?pin the candidate commit}"
mkdir -p "$out"
producer() {
  local sha ver
  sha=$(sha256sum < "$solar" | cut -c1-64); ver=$("$solar" --version | head -1)
  [[ $sha == "$EXPECT_SOLAR_SHA256" ]] || { echo "refused: $solar is $sha, not $EXPECT_SOLAR_SHA256" >&2; return 1; }
  [[ $ver == *"commit.${EXPECT_SOLAR_COMMIT:0:8}"* ]] || { echo "refused: $solar reports '$ver', not commit $EXPECT_SOLAR_COMMIT" >&2; return 1; }
}
step() { local n=$1; shift; "$@" > "$out/$n.log" 2>&1; local rc=$?; echo "$n exit=$rc $(date -u +%FT%TZ)" | tee -a "$out/prep-steps"; return $rc; }
producer || exit 1
[[ -s $tools/solar-tv/milestone/cutpoints.py ]] || { echo "error: $tools has no solar-tv/milestone/cutpoints.py" >&2; exit 1; }
cd "$lean"
mapfile -t opts < Certified/uv2_erc20/options
{ echo "path $solar"; echo "sha256 $EXPECT_SOLAR_SHA256"; echo "commit $EXPECT_SOLAR_COMMIT"
  echo "version $("$solar" --version | head -1)"; echo "flags ${opts[*]}"
  echo "tools $(git -C "$tools" rev-parse HEAD) $(git -C "$tools" status --porcelain | wc -l) uncommitted"; } > "$out/producer"
od=Certified/uv2_erc20; gen=$od/gen/UniswapV2ERC20
rm -rf "$od/gen" "$od/ProofsT" "$od/ProofsT.lean" "$od/Ctx.lean" "$od/CtxNeg.lean" "$od/CtxGas.lean"
step gas env SOLAR_DIR="$tools" SOLAR="$solar" scripts/verify/uniswap_v2_gas.sh || exit 1
excluded=(06fdde03='name()' 95d89b41='symbol()' 313ce567='decimals()' 18160ddd='totalSupply()'
  70a08231='balanceOf(address)' dd62ed3e='allowance(address,address)' 3644e515='DOMAIN_SEPARATOR()'
  30adf81f='PERMIT_TYPEHASH()' 7ecebe00='nonces(address)'
  d505accf='permit(address,address,uint256,uint256,uint8,bytes32,bytes32)'
  095ea7b3='approve(address,uint256)' 23b872dd='transferFrom(address,address,uint256)')
args=(); for e in "${excluded[@]}"; do args+=(--exclude "$e"); done
step certify python3 "$tools/tools/certify/certify.py" "$gen" --gen-module uv2_erc20.gen.UniswapV2ERC20 \
  --proof-dir "$od/ProofsT" --proof-module uv2_erc20.ProofsT --namespace SolarTV.Milestone.Uv2Erc20T \
  --label uv2_erc20_transfer --cut --joins --no-mir-joins --segment 8 "${args[@]}" || exit 1
step asm_ctx python3 "$tools/tools/certify/asm_ctx.py" "$gen" --gen-module uv2_erc20.gen.UniswapV2ERC20 \
  --out "$od/Ctx.lean" --neg-out "$od/CtxNeg.lean" --module uv2_erc20.Ctx || exit 1
python3 -c 'import json,sys; m=json.load(open(sys.argv[1])); sys.exit(0 if m["compiler"].split("commit.")[1][:8]==sys.argv[2][:8] else 1)' \
  "$gen/manifest.json" "$EXPECT_SOLAR_COMMIT" || { echo "refused: the stage export names another compiler" >&2; exit 1; }
producer || exit 1
{ find Certified/uv2_erc20 -type f \( -name '*.lean' -o -name '*.json' -o -name options \)
  echo scripts/verify/uniswap-v2/UniswapV2ERC20.sol; find .cache/uniswap-v2/src -name '*.sol'; } \
  | sort -u | xargs sha256sum > "$out/inputs.sha256"
echo "prep done: $(wc -l < "$out/inputs.sha256") input files"
