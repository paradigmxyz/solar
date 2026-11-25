import SolarTV.MIR.State.Ops
import SolarTV.MIR.Transforms.PhaseShaping

/-! Copying calldata starting at its exact end writes zero bytes. Both
operations ask for the same memory expansion before the write. -/
namespace SolarTV.MIR.LowerMemoryZero
open EvmRules SolarTV.Execution

theorem read_padding (input : List Byte) (source count : Nat)
    (outside : input.length ≤ source) :
    readBytes input source count = List.replicate count 0 := by
  apply List.ext_getElem
  · simp [readBytes]
  · intro i hi hj
    have missing : input.length ≤ source + i := by omega
    simp [readBytes, List.getElem?_eq_none missing]

theorem padding_copy (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (destination count : Word) (fits : cfg.environment.input.length < 2^256) :
    MIR.State.memoryOperation cfg frame .zero [destination, count] =
      (do
        match ← MIR.State.memoryOperation cfg frame .calldataSize [] with
        | .returned (some size, next) =>
          MIR.State.memoryOperation cfg next .calldataCopy [destination, size, count]
        | .returned (none, _) => .error "calldata size result missing"
        | .reverted data => return .reverted data
        | .invalid => return .invalid) := by
  rw [MIR.PhaseShaping.calldata_size_copy]
  change _ = MIR.State.memoryOperation cfg frame .calldataCopy
    [destination, BitVec.ofNat 256 cfg.environment.input.length, count]
  have length : (BitVec.ofNat 256 cfg.environment.input.length).toNat = cfg.environment.input.length := by
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt fits]
  simp only [MIR.State.memoryOperation]
  rw [length, read_padding _ _ _ (Nat.le_refl _)]
  rfl

theorem zero_word_store (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (destination : Word) :
    MIR.State.memoryOperation cfg frame .zero [destination, 32] =
      MIR.State.memoryOperation cfg frame .store [destination, 0] := by
  have zero : wordBytes (0 : Word) = List.replicate 32 0 := by decide
  simp only [MIR.State.memoryOperation, zero]
  rfl

end SolarTV.MIR.LowerMemoryZero
