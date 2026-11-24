import SolarTV.MIR.Effects

namespace SolarTV.MIR.LowerIntegers
open EvmRules

theorem widenLow (word : Word) (width : Nat) (fits : width ≤ 256) :
    word &&& BitVec.ofNat 256 (2^width - 1) = (word.setWidth width).setWidth 256 := by
  have power := Nat.pow_le_pow_right (by decide : 0 < 2) fits
  have positive := Nat.two_pow_pos width
  have bound : 2^width - 1 < 2^256 := by omega
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_setWidth, BitVec.toNat_and, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt bound, Nat.and_two_pow_sub_one_eq_mod]
  rw [Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le (Nat.mod_lt _ positive) power)]

theorem eqWiden {width : Nat} (left right : BitVec width) (fits : width ≤ 256) :
    left.setWidth 256 = right.setWidth 256 ↔ left = right := by
  constructor
  · intro equal
    simpa [BitVec.setWidth_setWidth_of_le _ fits] using
      congrArg (fun (value : Word) => value.setWidth width) equal
  · rintro rfl
    rfl

theorem ultWiden {width : Nat} (left right : BitVec width) (fits : width ≤ 256) :
    (left.setWidth 256).ult (right.setWidth 256) = left.ult right := by
  simp [BitVec.ult, BitVec.toNat_setWidth_of_le fits]

theorem eqWidenNat {width : Nat} (word : BitVec width) (number : Nat)
    (fits : width ≤ 256) (clean : number < 2^width) :
    word.setWidth 256 = BitVec.ofNat 256 number ↔ word = BitVec.ofNat width number := by
  rw [← BitVec.setWidth_ofNat_of_le_of_lt fits clean]
  exact eqWiden _ _ fits

theorem oneBit (value : BitVec 1) : (if value = 0#1 then 0#1 else 1#1) = value := by
  rcases BitVec.eq_zero_or_eq_one value with rfl | rfl <;> decide

end SolarTV.MIR.LowerIntegers
