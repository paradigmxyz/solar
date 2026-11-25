import SolarTV.MIR.State.Messages

/-! Raw message operands are compared at the boundary where they take effect.
Target addresses use their low 160 bits and an empty byte range has no address.
The supplied sample and the child execution use the ordinary message model. -/

set_option autoImplicit false

namespace SolarTV.MIR.CallOperands
open EvmRules SolarTV.Execution MIR.State

private theorem expand_empty (memory : List Byte) (offset : Nat) :
    expand memory offset 0 = expand memory 0 0 := by
  simp only [expand, requiredWords, ite_true]

private theorem write_empty (memory : List Byte) (offset : Nat) :
    writeBytes memory offset [] = memory := by
  simp [writeBytes, List.take_append_drop]

theorem target_width (cfg : Config) (frame : Frame) (fuel opcode : Nat)
    (address : Word) (args : List Word) (gas : Option Word)
    (supported : opcode = 0xf1 ∨ opcode = 0xf2 ∨ opcode = 0xf4 ∨ opcode = 0xfa) :
    external fuel cfg frame opcode (address :: args) gas =
      external fuel cfg frame opcode ((address.setWidth 160).setWidth 256 :: args) gas := by
  have same : ((address.setWidth 160).setWidth 256).setWidth 160 = address.setWidth 160 := by
    rw [BitVec.setWidth_setWidth_of_le _ (by decide : 160 ≤ 256), BitVec.setWidth_eq]
  rcases supported with rfl | rfl | rfl | rfl <;> simp [external, same]

def effectiveOffset (offset size : Word) : Word :=
  if size = 0 then 0 else offset

theorem call_ranges_with_value (cfg : Config) (frame : Frame) (fuel opcode : Nat)
    (address amount inputOffset inputSize outputOffset outputSize : Word) (gas : Option Word)
    (supported : opcode = 0xf1 ∨ opcode = 0xf2) :
    external fuel cfg frame opcode [address,amount,inputOffset,inputSize,outputOffset,outputSize] gas =
      external fuel cfg frame opcode [address,amount,effectiveOffset inputOffset inputSize,inputSize,
        effectiveOffset outputOffset outputSize,outputSize] gas := by
  rcases supported with rfl | rfl <;>
    by_cases hi : inputSize = 0#256 <;> by_cases ho : outputSize = 0#256 <;>
    simp [external, effectiveOffset, hi, ho, expand_empty, write_empty]

theorem call_ranges_without_value (cfg : Config) (frame : Frame) (fuel opcode : Nat)
    (address inputOffset inputSize outputOffset outputSize : Word) (gas : Option Word)
    (supported : opcode = 0xf4 ∨ opcode = 0xfa) :
    external fuel cfg frame opcode [address,inputOffset,inputSize,outputOffset,outputSize] gas =
      external fuel cfg frame opcode [address,effectiveOffset inputOffset inputSize,inputSize,
        effectiveOffset outputOffset outputSize,outputSize] gas := by
  rcases supported with rfl | rfl <;>
    by_cases hi : inputSize = 0#256 <;> by_cases ho : outputSize = 0#256 <;>
    simp [external, effectiveOffset, hi, ho, expand_empty, write_empty]

theorem code_size (cfg : Config) (frame : Frame) (bytes : List Byte)
    (image : cfg.layout.code = some bytes) :
    operation cfg frame (.context (.environment 0x38)) [] =
      value frame (BitVec.ofNat 256 bytes.length) := by
  change (required cfg.layout.code "missing explicit MIR code image" >>= fun code =>
    value frame (BitVec.ofNat 256 code.length)) = _
  rw [image]
  rfl

end SolarTV.MIR.CallOperands
