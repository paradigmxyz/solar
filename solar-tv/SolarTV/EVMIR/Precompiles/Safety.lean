import SolarTV.EVMIR.Precompiles.Observe

/-! Kernel-checked digest vectors and failure boundaries. Differential tests
cover every active precompile against the native engine; these checks additionally
pin the pure hashing definitions and distinctions between malformed input,
out-of-gas, and successful empty signature output without foreign computation. -/

namespace SolarTV.Precompiles
open EVM.State
set_option maxRecDepth 16384

example : SHA256.hash [] = be 32 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 := by
  decide +kernel
example : RIPEMD160.hash [97,98,99] = be 20 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc := by
  decide +kernel
example : execute .cancun 1 [] 3000 = .ok (.success,[],0) := by rfl
example : execute .osaka 256 [] 6899 = .ok (.outOfGas,[],0) := by rfl
example : execute .prague 12 [] 0 = .ok (.exception,[],0) := by rfl
example : execute .osaka 5 (be 32 1025 ++ be 32 0 ++ be 32 0) 499 =
    .ok (.outOfGas,[],0) := by rfl

end SolarTV.Precompiles
