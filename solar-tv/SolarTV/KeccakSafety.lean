import SolarTV.Keccak

/-! Kernel-evaluated Keccak vectors cover empty input, byte/lane ordering,
the one-byte pad boundary and multi-block absorption. Positive checks never
use native_decide, an external hash function or a cryptographic axiom. -/

set_option maxRecDepth 8192
set_option maxHeartbeats 2000000

namespace SolarTV.Keccak

@[simp] theorem empty : hash [] = 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470 := by
  decide +kernel

example : hash [97, 98, 99] =
    0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45 := by
  decide +kernel

example : (pad (List.replicate 135 0)).length = 136 := by decide
example : (pad (List.replicate 136 0)).length = 272 := by decide

-- These boundary values are also checked by the Rust interpreter tests.
example : hash (List.replicate 135 0) =
    18946676226192871058387231643792898752776336720096239710285640021474136472638 := by
  decide +kernel
example : hash (List.replicate 136 0) =
    26391523361189685675012273911989963211388955111946558690139574972697269708281 := by
  decide +kernel
example : hash (List.replicate 137 0) =
    86349320094517957708823086655008166321692264600573236910167712870522516913448 := by
  decide +kernel

end SolarTV.Keccak
