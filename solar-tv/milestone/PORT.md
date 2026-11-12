# Lean 4.35.0 port

solar-tv (`SolarTV`) and scripts/evm-rules/lean (`EvmRules`, `evm_check`) build on
`leanprover/lean4:v4.35.0`, the toolchain solidity-lean pins, so that solidity-lean can
require `solar_tv` and use `SolarTV.Sem` as its contract semantics interface.

The port changes only the toolchain pins and the checksums in
`.github/scripts/install_lean.sh` (the sha256 digests GitHub publishes for the
`lean-4.35.0-linux{,_aarch64}.tar.zst` release assets). Every source builds unchanged,
and every `#guard_msgs` axiom check in `SolarTV.lean` passes with the same axiom sets.

Port commit: see `git log --format=%H -1 -- solar-tv/lean-toolchain` on branch
ms-bytecode (recorded below once committed).
