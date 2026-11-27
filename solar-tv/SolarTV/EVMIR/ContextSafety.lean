import SolarTV.EVMIR.Memory
import SolarTV.EVMIR.Gas

/-! Read-only context correspondence for universal stack proofs.
The same byte reader and 160-bit address encoding are used by direct frame
execution. Universal proofs quantify even inconsistent size/content pairs;
every actual frame embeds through `Context.ofEnvironment`. No incoming caller
or calldata byte is fixed to zero. Reads leave projected memory unchanged. -/

namespace SolarTV.EVM.ContextSafety
open EvmRules

theorem caller_frame (environment : World.Environment) (returnData : List World.Byte)
    (stack : Stack) :
    step (.ofEnvironment environment returnData) .caller stack =
      some (environment.caller.setWidth 256 :: stack) := rfl

theorem callvalue_frame (environment : World.Environment) (returnData : List World.Byte)
    (stack : Stack) :
    step (.ofEnvironment environment returnData) .callvalue stack =
      some (environment.value :: stack) := rfl

theorem calldata_frame (environment : World.Environment) (returnData : List World.Byte)
    (offset : Word) (stack : Stack) :
    step (.ofEnvironment environment returnData) .calldataload (offset :: stack) =
      some (Execution.readWord environment.input offset.toNat :: stack) := rfl

theorem calldata_memory (ctx : Context) (offset : Word) (stack : Stack)
    (memory : SolarTV.Memory.State) (fits : (offset :: stack).length ≤ 1024) :
    Memory.step ctx (.stack .calldataload) (offset :: stack) memory =
      .ok (Execution.readWord ctx.calldata offset.toNat :: stack, memory) := by
  have bound : ¬1024 < stack.length + 1 := by simpa using Nat.not_lt.mpr fits
  simp [Memory.step, stepChecked, step, bound, Except.map]

example : step ⟨0,0,0,[1,2], 0⟩ .calldataload [1] =
    some [BitVec.ofNat 256 (2^249)] := by decide +kernel

example : step ⟨0,0,0,[1,2], 0⟩ .calldataload [BitVec.ofNat 256 (2^256-1)] =
    some [0] := by decide +kernel

example : stepChecked ⟨0,0,0,[], 0⟩ .calldataload [] = .error .underflow := rfl

example : Gas.pureCost .caller = 2 ∧ Gas.pureCost .calldataload = 3 := by decide

example : Gas.pureCost .callvalue = 2 := by decide

example : step ⟨0, 0, 0, [], 7⟩ .callvalue [] ≠ step ⟨0, 0, 0, [], 7⟩ (.push 0) [] := by
  decide +kernel

end SolarTV.EVM.ContextSafety
