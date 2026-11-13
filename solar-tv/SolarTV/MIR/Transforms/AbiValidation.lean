import SolarTV.MIR.State.Abi

/-! ABI obligations can be discharged only after establishing canonicality on
the reached path. These proofs do not assume a marker is true, infer its rule
from an instruction's shape, or synthesize a revert when it is false. Raw ABI
function padding and the normalized function word are separate obligations. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

theorem discharge_abi_obligation (rule : AbiRule) (word : Word) (frame : Frame)
    (canonical : abiCanonical rule word = true) :
    (do abiObligation rule word; pure frame : Except String Frame) = .ok frame := by
  simp [abiObligation, canonical]
  rfl

theorem unproved_abi_obligation (rule : AbiRule) (word : Word)
    (uncanonical : abiCanonical rule word = false) :
    abiObligation rule word = .error "unproved ABI canonicality obligation" := by
  simp [abiObligation, uncanonical]

/-- A wrapping shift tests exactly the padding bits discarded by a left-aligned ABI word. -/
@[simp] theorem shift_zero_iff_padding (word : Word) (bits : Nat) (bound : bits ≤ 256) :
    word <<< bits = 0 ↔ word &&& BitVec.ofNat 256 (2^(256-bits)-1) = 0 := by
  have exponent : 256 = (256-bits)+bits := by omega
  have maskBound : 2^(256-bits)-1 < 2^256 := by
    have h := Nat.pow_le_pow_right (n:=2) (by decide) (show 256-bits ≤ 256 by omega)
    have hp := Nat.two_pow_pos (256-bits)
    omega
  rw [← BitVec.toNat_inj, ← BitVec.toNat_inj]
  simp only [BitVec.toNat_shiftLeft, Nat.shiftLeft_eq,
    BitVec.toNat_and, BitVec.toNat_ofNat, Nat.mod_eq_of_lt maskBound,
    Nat.and_two_pow_sub_one_eq_mod]
  have power : 2^256 = 2^(256-bits)*2^bits := by rw [← Nat.pow_add, ← exponent]
  rw [power, Nat.mul_mod_mul_right]
  simp [Nat.mul_eq_zero]

/-- A clean normalized value does not prove the discarded raw padding was canonical. -/
theorem function_normalization_is_not_padding_evidence :
    abiObligation (.uint 192) ((0x110000000000000001#256) >>> 64) = .ok () ∧
    abiObligation (.left 192) (0x110000000000000001#256) =
      .error "unproved ABI canonicality obligation" := by
  have normalized : abiCanonical (.uint 192) ((0x110000000000000001#256) >>> 64) = true := by
    decide +kernel
  have padding : abiCanonical (.left 192) (0x110000000000000001#256) = false := by
    decide +kernel
  constructor <;> unfold abiObligation
  · rw [normalized]
    rfl
  · rw [padding]
    rfl

end SolarTV.MIR.State
