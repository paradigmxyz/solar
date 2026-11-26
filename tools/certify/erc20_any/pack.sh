#!/usr/bin/env bash
# Package one slot's built closure so a later slot replays the same oleans instead of rebuilding.
#
#   tools/certify/erc20_any/pack.sh LEAN_DIR OUT SHIP_DIR
#
# Lean's elaboration is not byte-deterministic: on identical inputs and dependencies, rebuilding
# SolidCore.Refinement.Solar.AssembleCallWorld gives one of two oleans (2e474eef or cebc9de8, the
# same declarations in the same order, laid out differently), with any LEAN_NUM_THREADS, including
# 1. Two slots that each build can therefore never be required to have byte-identical plans. A
# later slot unpacks this archive instead (slot.sh with PRIOR_SHIP) and checks every planned olean
# against the earlier slot's plan before replaying.
#
# Reads OUT/check_final.log's MODULE lines and archives, under their absolute paths, every olean of
# the closure outside the Lean toolchain (with its .ilean and any .olean.* sibling), the generated
# sources neg.sh and gen.py read (Certified/uv2_erc20, Certified/erc20_any, the TransferExpected
# overlay, Slice/Gen, the port), and OUT/fresh.json and OUT/check_final.log. Writes
# SHIP_DIR/ship.tar, SHIP_DIR/files (each archived path with its sha256) and SHIP_DIR/ship.sha256.
set -uo pipefail
lean=$(realpath "$1"); out=$(realpath "$2"); ship=$(realpath -m "$3")
mkdir -p "$ship"
[[ -s $out/check_final.log ]] || { echo "pack: no $out/check_final.log" >&2; exit 1; }
tc=$(dirname "$(dirname "$(cd "$lean" && lake env which lean 2>/dev/null || command -v lean)")")
list=$ship/list; : > "$list"
awk '$1 == "MODULE" { print $3 }' "$out/check_final.log" | while read -r p; do
  [[ $p == "$tc"/* || $p == "$HOME"/.elan/* ]] && continue
  [[ -f $p ]] || { echo "pack: missing olean $p" >&2; exit 1; }
  b=${p%.olean}
  for f in "$p" "$b.ilean" "$p".*; do [[ -f $f ]] && echo "$f"; done
done >> "$list" || exit 1
for d in Certified/uv2_erc20 Certified/erc20_any Contracts/UniswapV2/ERC20/Slice/Gen .cache/uniswap-v2/src; do
  [[ -d $lean/$d ]] && find "$lean/$d" -type f ! -name '*.olean*' ! -name '*.ilean' >> "$list"
done
for f in Contracts/UniswapV2/ERC20/Certified/TransferExpected.lean scripts/verify/uniswap-v2/UniswapV2ERC20.sol; do
  [[ -f $lean/$f ]] && echo "$lean/$f" >> "$list"
done
echo "$out/fresh.json" >> "$list"; echo "$out/check_final.log" >> "$list"
sort -u -o "$list" "$list"
xargs -d '\n' sha256sum < "$list" > "$ship/files" || exit 1
tar -cf "$ship/ship.tar" -P -T "$list" || exit 1
sha256sum "$ship/ship.tar" | cut -c1-64 > "$ship/ship.sha256"
echo "pack: $(wc -l < "$list") files, $(awk '$1 == "MODULE"' "$out/check_final.log" | wc -l) closure modules, ship.tar $(cat "$ship/ship.sha256")"
