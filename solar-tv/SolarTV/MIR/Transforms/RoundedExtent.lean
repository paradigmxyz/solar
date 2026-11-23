import SolarTV.MIR.Transforms.LowerReturndata
import SolarTV.MIR.Transforms.WordRewrite

/-! Preserve the live results of rounding a byte length for an allocation.
The overflow branch can be removed when the source length fits in 255 bits.
The checked allocation itself remains an observable action. -/

namespace SolarTV.MIR.RoundedExtent
open EvmRules

theorem shifted_length (word count : Word) (positive : 0 < count.toNat) :
    (MIR.shift .shr word count).toNat < 2^255 := by
  unfold MIR.shift Word.shift
  split
  · change 0 < 2^255
    decide
  · rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
    apply (Nat.div_lt_iff_lt_mul (Nat.two_pow_pos _)).mpr
    have power : 2 ≤ 2 ^ count.toNat := Nat.le_pow (by omega)
    have bound := word.isLt
    have scale := Nat.mul_le_mul_left (2^255) power
    omega

theorem masked_length (length mask : Word) (fits : length.toNat < 2^255) :
    (length &&& mask).toNat < 2^255 := by
  rw [BitVec.toNat_and]
  exact Nat.lt_of_le_of_lt Nat.and_le_left fits

theorem overflow_clear (length : Word) (fits : length.toNat < 2^255) :
    MIR.compare .lt (LowerReturndata.size length) length = 0 := by
  have bound : length.toNat + 63 < 2^256 := by omega
  have rounded := LowerReturndata.roundedSize length.toNat bound
  have same : BitVec.ofNat 256 length.toNat = length := BitVec.eq_of_toNat_eq (by
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt length.isLt])
  rw [same] at rounded
  have ordered : ¬ (LowerReturndata.size length).ult length := by
    rw [BitVec.ult_iff_toNat_lt, rounded, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt (by omega)]
    omega
  simp [MIR.compare, Bool.eq_false_iff.mpr ordered]

def result (division checked : Bool) (length : Word) : Word × Word × Word × Word × BitVec 1 :=
  let sum := MIR.eval .add length 31
  let words := if division then MIR.eval .div sum 32 else MIR.shift .shr sum 5
  let aligned := MIR.eval .and sum (~~~(31 : Word))
  let extent := MIR.eval .add aligned 32
  (sum, words, aligned, extent, if checked then MIR.compare .lt extent length else 0)

theorem normalized (division checked : Bool) (length : Word) (fits : length.toNat < 2^255) :
    result division checked length = result false false length := by
  have quotient : MIR.shift .shr (length + 31) 5 = MIR.eval .div (length + 31) 32 :=
    WordRewrite.normalized (.scale .div 5) (length + 31)
  have guard := overflow_clear length fits
  change MIR.compare .lt
    (MIR.eval .add (MIR.eval .and (MIR.eval .add length 31) (~~~(31 : Word))) 32)
    length = 0 at guard
  cases division <;> cases checked <;>
    simp only [result, Bool.false_eq_true, ite_false, ite_true, quotient, guard]

end SolarTV.MIR.RoundedExtent
