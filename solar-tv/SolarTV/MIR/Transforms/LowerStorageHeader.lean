import SolarTV.MIR.State.StorageBytes

/-! The word operations used by lower-builtins decode both forms of a storage
byte-string header. Prove the decoded length and malformed-header panic for
every 256-bit header, including dirty bits in a short header. -/

namespace SolarTV.MIR.LowerStorageHeader
open MIR.State EvmRules

def lowered (header : Word) : Except RevertData (Bool × Nat) :=
  let tag := header &&& 1
  let long := tag == 1
  let length := ((header >>> 1 : Word) &&& ((0-tag) ||| 127)).toNat
  if long == (length < 32) then .error (.panic 0x22) else .ok (long,length)

theorem decode (header : Word) : lowered header = bytesHeader header := by
  have tag : header &&& 1 = BitVec.ofNat 256 (header.toNat % 2) := by
    apply BitVec.eq_of_toNat_eq
    have bounded : header.toNat % 2 < 2^256 := by omega
    simp [BitVec.toNat_and,Nat.and_one_is_mod,Nat.mod_eq_of_lt bounded]
  rcases (show header.toNat % 2 = 0 ∨ header.toNat % 2 = 1 by omega) with short | long
  · have mask : ((0 : Word)-0) ||| 127 = 127 := by decide
    have low : (127 : Word).toNat = 2^7-1 := by decide
    have length : ((header >>> 1 : Word) &&& (127 : Word)).toNat =
        header.toNat / 2 % 128 := by
      rw [BitVec.toNat_and, low, Nat.and_two_pow_sub_one_eq_mod]
      simp [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
    unfold lowered bytesHeader
    rw [tag, short]
    change (if ((0 : Word) == 1) == ((((header >>> 1 : Word) &&& ((0:Word)-0 ||| 127)).toNat) < 32)
      then Except.error (RevertData.panic 0x22)
      else Except.ok ((0 : Word) == 1,((header >>> 1 : Word) &&& ((0:Word)-0 ||| 127)).toNat)) = _
    rw [mask, length]
    rfl
  · have mask : ((0 : Word)-1) ||| 127 = BitVec.allOnes 256 := by decide
    have length : (header >>> 1 : Word).toNat = header.toNat / 2 := by
      simp [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
    unfold lowered bytesHeader
    rw [tag, long]
    change (if ((1 : Word) == 1) == ((((header >>> 1 : Word) &&& ((0:Word)-1 ||| 127)).toNat) < 32)
      then Except.error (RevertData.panic 0x22)
      else Except.ok ((1 : Word) == 1,((header >>> 1 : Word) &&& ((0:Word)-1 ||| 127)).toNat)) = _
    rw [mask, BitVec.and_allOnes, length]
    rfl

end SolarTV.MIR.LowerStorageHeader
