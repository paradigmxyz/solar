import SolarTV.MIR.State.Values

/-! Kernel-proved scalar normal forms for symbolic typed SSA execution.
Keep boolean OR and unsigned width checks as word predicates instead of
splitting every intermediate comparison. These laws quantify all widths and
values; they are independent of pass names, source functions and call effects. -/

namespace SolarTV.MIR.State
open EvmRules

@[simp] theorem bit_or_one (a b : BitVec 1) :
    a ||| b = 1 ↔ a = 1 ∨ b = 1 := by
  rcases BitVec.eq_zero_or_eq_one a with rfl | rfl <;>
    rcases BitVec.eq_zero_or_eq_one b with rfl | rfl <;> decide

@[simp↓] theorem nat_or_bits (a b : BitVec width) :
    BitVec.ofNat width (a.toNat ||| b.toNat) = a ||| b := by
  rw [← BitVec.toNat_or]
  simp

@[simp↓] theorem ofNat_toNat_shift_right (a : BitVec width) (shift : Nat) :
    BitVec.ofNat width (a.toNat >>> shift) = a >>> shift := by
  change BitVec.ofNat width ((a >>> shift).toNat) = a >>> shift
  rw [BitVec.ofNat_toNat, BitVec.setWidth_eq]

@[simp] theorem shift_zero (a : BitVec width) (shift : Nat) :
    a >>> shift = BitVec.ofNat width 0 ↔ a.toNat ≤ 2 ^ shift - 1 := by
  rw [← BitVec.toNat_inj, BitVec.toNat_ushiftRight]
  simp only [Nat.shiftRight_eq_div_pow]
  change a.toNat / 2 ^ shift = 0 ↔ _
  rw [Nat.div_eq_zero_iff_lt (Nat.two_pow_pos _)]
  have := Nat.two_pow_pos shift
  omega

end SolarTV.MIR.State
