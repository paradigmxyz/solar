import SolarTV.EVMIR.State.Observe

/-! Kernel checks for the entry evaluator's proof boundary and creation rules.
Unsupported execution cannot establish equivalence, and reverted frames restore
their original journal. Address derivation is pinned to EIP-1014's published
vector, independently of the native replay engine. -/

namespace SolarTV.EVM.State

theorem unsupported_not_equivalent (a b : Config) (policy : Policy)
    (reason : String) (other : Run) :
    ¬ equivalent a b policy (.error reason) other := by
  cases other <;> simp [equivalent]

theorem reverted_entry_restores_world (fuel : Nat) (cfg : Config)
    (world changed : Journal) (frame : Frame) (data : List Byte) (gas : Nat)
    (work : Nat)
    (execution : run fuel cfg world frame = .ok ⟨.reverted, data, changed, gas, work⟩) :
    execute fuel cfg world frame = .ok ⟨.reverted, data, world, gas, work⟩ := by
  simp only [execute, execution]
  rfl

theorem exception_cannot_refine_to_success (cfg : Config) (world : Journal) :
    ¬ equivalent cfg cfg .refine (.ok ⟨.exception, [], world, 0, 0⟩)
      (.ok ⟨.success, [], world, 0, 0⟩) := by
  simp [equivalent, observe]

-- EIP-1014, example 0: https://eips.ethereum.org/EIPS/eip-1014#examples
example : create2Address 0 0 [0] = 0x4d1a2e2bb4f88f0250f26ffff098b0b30b26bf38 := by
  decide +kernel

example : validDeployedCode .berlin [0xef] = true := by decide
example : validDeployedCode .london [0xef] = false := by decide
example (code : List Byte) (size : code.length > 24576) :
    validDeployedCode .cancun code = false := by
  simp [validDeployedCode, Fork.atLeast, Fork.ordinal, show ¬ code.length ≤ 24576 by omega]

example : validJump [0x60, 0x5b, 0x00] 1 = false := by decide
example : validJump [0x60, 0x01, 0x5b] 2 = true := by decide

private def destructionFixture (created : Bool) : Journal :=
  {accounts := [{address := 1, nonce := 1, balance := 100, code := [0], storage := [(0,7)], created}]
   original := []
   warmAddresses := [1]
   warmSlots := []}

example : (finalize (destroy .shanghai (destructionFixture false) 1 1)).accounts = [] := by decide +kernel
example : (account (destroy .cancun (destructionFixture false) 1 1) 1).balance = 100 := by decide +kernel
example : (finalize (destroy .cancun (destructionFixture true) 1 1)).accounts = [] := by decide +kernel
example : (account (destroy .shanghai (destructionFixture false) 1 1) 1).code = [0] := by decide +kernel
example : (destroy .berlin (destroy .berlin (destructionFixture false) 1 1) 1 1).refund = 24000 := by decide +kernel

example : blockHash {number := 300, hashes := [(43,1),(44,2),(299,3),(300,4)]} 43 = 0 := by decide +kernel
example : blockHash {number := 300, hashes := [(44,2)]} 44 = 2 := by decide +kernel
example : blockHash {number := 300, hashes := [(300,4)]} 300 = 0 := by decide +kernel
example : delegationTarget .cancun ([0xef,0x01,0x00] ++ List.replicate 20 0) = none := by decide +kernel
example : delegationTarget .prague ([0xef,0x01,0x00] ++ List.replicate 19 0 ++ [42]) = some 42 := by decide +kernel
example : delegationTarget .prague ([0xef,0x01,0x00] ++ List.replicate 21 0) = none := by decide +kernel

end SolarTV.EVM.State
