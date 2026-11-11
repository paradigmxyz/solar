import SolarTV.Memory

/-! Executable checks of byte order, overlapping writes and exact expansion.
Gas is deliberately not priced yet. These kernel proofs pin the memory rules
used by both IR models, including zero-sized accesses at maximal offsets. -/

namespace SolarTV.Memory

set_option maxRecDepth 4096

example : (expand (initial [] 0) (2^256 - 1) 0).words = 0 := by decide
example : msize (expand (initial [] 0) 1 32) = 64 := by decide
example : load (initial [0xffff] 0) 0 = 0 := by decide
example : load (initial [0x1234] 1) 0 = 0x1234 := by decide
example : load (store (initial [] 0) 1 0x1234) 1 = 0x1234 := by decide
example : load (storeByte (store (initial [] 0) 0 0x1234) 31 0x1ff) 0 = 0x12ff := by decide
example : load (storeByte (initial [] 0) 0 0xff) 0 = (0xff : EvmRules.Word) <<< 248 := by decide

end SolarTV.Memory
