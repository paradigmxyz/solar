import SolarTV.MIR.Transforms.CfgFlow

/-! Equal scalar values may share one definition in the unpriced MIR projection.
The edge certificate checks that a reused producer is on every entry path.
Width and predicate identities preserve the original typed operation. -/

set_option autoImplicit false

namespace SolarTV.MIR.ScalarReuse
open EvmRules MIR.State

theorem unsigned_nonzero {width : Nat} (word : BitVec width) :
    MIR.compare .gt word 0 = MIR.compare .ne word 0 := by
  have bit : (0 : BitVec width).ult word = (word != 0) := by
    rw [Bool.eq_iff_iff, BitVec.ult_iff_toNat_lt, bne_iff_ne]
    simp [BitVec.toNat_eq]
    omega
  change (if (0 : BitVec width).ult word then 1 else 0) =
    (if word != 0 then 1 else 0)
  rw [bit]

theorem unsigned_flip {width : Nat} (left right : BitVec width) :
    MIR.compare .gt left right = MIR.compare .lt right left := rfl

theorem same_width {width : Nat} (word : BitVec width) :
    word.setWidth width = word := BitVec.setWidth_eq word

theorem round_trip {narrow wide : Nat} (word : BitVec narrow) (fits : narrow ≤ wide) :
    (word.setWidth wide).setWidth narrow = word := by
  rw [BitVec.setWidth_setWidth_of_le _ fits, BitVec.setWidth_eq]

theorem literal_cast (source target number : Nat) (fits : number < 2^source) :
    (BitVec.ofNat source number).setWidth target =
      BitVec.ofNat target (number % 2^target) := by
  apply BitVec.eq_of_toNat_eq
  simp [BitVec.toNat_setWidth, Nat.mod_eq_of_lt fits, Nat.mod_mod]

theorem clean_mask {narrow wide : Nat} (word : BitVec narrow) (fits : narrow ≤ wide) :
    word.setWidth wide &&& BitVec.ofNat wide (2^narrow - 1) = word.setWidth wide := by
  have max : 2^narrow ≤ 2^wide := Nat.pow_le_pow_right (by decide) fits
  have pos := Nat.two_pow_pos narrow
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_and, BitVec.toNat_setWidth_of_le fits, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt (by omega), Nat.and_two_pow_sub_one_eq_mod, Nat.mod_eq_of_lt word.isLt]

theorem clean_shift {narrow wide : Nat} (word : BitVec narrow) (amount : BitVec wide)
    (fits : narrow + amount.toNat ≤ wide) (legal : amount.toNat < wide) :
    MIR.shift .shr (MIR.shift .shl (word.setWidth wide) amount) amount =
      word.setWidth wide := by
  have narrowFits : narrow ≤ wide := by omega
  have extent : word.toNat * 2^amount.toNat < 2^wide := by
    have mult := Nat.mul_lt_mul_of_pos_right word.isLt (Nat.two_pow_pos amount.toNat)
    rw [← Nat.pow_add] at mult
    exact Nat.lt_of_lt_of_le mult (Nat.pow_le_pow_right (by decide) fits)
  simp only [MIR.shift, Word.shift, Nat.not_le.mpr legal, ite_false]
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_ushiftRight, BitVec.toNat_shiftLeft, BitVec.toNat_setWidth_of_le narrowFits,
    Nat.shiftRight_eq_div_pow, Nat.shiftLeft_eq, Nat.mod_eq_of_lt extent,
    Nat.mul_div_cancel _ (Nat.two_pow_pos _)]

theorem zero_identities {width : Nat} (word : BitVec width) :
    MIR.eval .add word 0 = word ∧ MIR.eval .add 0 word = word ∧
      MIR.eval .or word 0 = word ∧ MIR.eval .or 0 word = word ∧
      MIR.eval .xor word 0 = word ∧ MIR.eval .xor 0 word = word ∧
      MIR.eval .sub word 0 = word ∧ MIR.eval .sub word word = 0 := by
  simp [MIR.eval]

theorem closed_comparison {width left right : Nat}
    (lbound : left < 2^width) (rbound : right < 2^width) :
    MIR.compare .eq (BitVec.ofNat width left) (BitVec.ofNat width right) =
        (if left = right then 1 else 0) ∧
      MIR.compare .ne (BitVec.ofNat width left) (BitVec.ofNat width right) =
        (if left = right then 0 else 1) := by
  simp [MIR.compare, BitVec.toNat_eq, Nat.mod_eq_of_lt lbound, Nat.mod_eq_of_lt rbound]

theorem bit_test (flag : BitVec 1) :
    MIR.compare .ne flag 0 = flag ∧ MIR.compare .ne 0 flag = flag ∧
      MIR.compare .eq flag 1 = flag ∧ MIR.compare .eq 1 flag = flag := by
  rcases BitVec.eq_zero_or_eq_one flag with rfl | rfl <;> decide

theorem flip_branch {width : Nat} (flag : BitVec 1) (fits : 1 ≤ width)
    {α : Type} (yes no : α) :
    MIR.branch (MIR.compare .eq (flag.setWidth width) 0) yes no =
      MIR.branch flag no yes := by
  have zero : flag.setWidth width = 0 ↔ flag = 0 := by
    rw [← BitVec.toNat_inj, BitVec.toNat_setWidth_of_le fits]
    simp [← BitVec.toNat_inj]
  have comparison : MIR.compare .eq (flag.setWidth width) 0 =
      (if flag = 0 then 1 else 0) := by
    change (if flag.setWidth width == 0 then 1 else 0) = _
    simp only [beq_iff_eq, zero]
  rw [comparison]
  rcases BitVec.eq_zero_or_eq_one flag with bit | bit <;> simp [MIR.branch, bit]

theorem complement_equality {width : Nat} (a b : BitVec width) {α : Type} (yes no : α) :
    MIR.branch (MIR.compare .ne a b) yes no =
      MIR.branch (MIR.compare .eq a b) no yes := by
  by_cases same : a = b <;> simp [MIR.branch, MIR.compare, same]

theorem complement_cut {width : Nat} (word lower : BitVec width)
    (fits : lower.toNat + 1 < 2^width) {α : Type} (yes no : α) :
    MIR.branch (MIR.compare .lt lower word) yes no =
      MIR.branch (MIR.compare .lt word (BitVec.ofNat width (lower.toNat+1))) no yes := by
  by_cases above : lower.toNat < word.toNat
  · have opposite : ¬ word.toNat < lower.toNat+1 := by omega
    simp [MIR.branch, MIR.compare, BitVec.ult, Nat.mod_eq_of_lt fits, above, opposite]
  · have opposite : word.toNat < lower.toNat+1 := by omega
    simp [MIR.branch, MIR.compare, BitVec.ult, Nat.mod_eq_of_lt fits, above, opposite]

theorem caller_value (cfg : Config) (frame : Frame) :
    operation cfg frame (.context (.environment 0x33)) [] =
      value frame (cfg.environment.caller.setWidth 256) := rfl

/-- The incoming operand is read before any of the destination phis are written. -/
theorem phi_read (frame : Frame) (predecessor result : Nat) (operand : Operand)
    (same : operand.ty.same operand.ty = true) :
    enter (some predecessor) [⟨result,operand.ty,[(predecessor,operand)]⟩] frame =
      (do
        let word ← readValue frame operand
        return {frame with locals := put frame.locals result word}) := by
  simp [enter, same]

abbrev Edges := List (Nat × Nat)

inductive Avoids (edges : Edges) (producer : Nat) : Nat → Prop where
  | entry : 0 ≠ producer → Avoids edges producer 0
  | step {before after : Nat} : Avoids edges producer before →
      (before,after) ∈ edges → after ≠ producer → Avoids edges producer after

structure ReuseSite where
  producer : Nat
  consumer : Nat
  outside : List Nat

/-- `outside` contains the entry and is closed under reachable edges after
removing the producer. It excludes the consumer. -/
def check (edges : Edges) (site : ReuseSite) : Bool :=
  site.producer == 0 ||
    (site.outside.contains 0 && !site.outside.contains site.consumer &&
     edges.all (fun (before,after) =>
       !site.outside.contains before || after == site.producer || site.outside.contains after))

theorem dominance (edges : Edges) (site : ReuseSite) (verified : check edges site = true) :
    ¬ Avoids edges site.producer site.consumer := by
  obtain ⟨producer, consumer, outside⟩ := site
  intro path
  simp only [check, Bool.or_eq_true, beq_iff_eq] at verified
  simp only at path
  rcases verified with root | checked
  · induction path with
    | entry distinct => exact distinct root.symm
    | step _ _ _ ih => exact ih
  · simp only [Bool.and_eq_true] at checked
    obtain ⟨⟨entry, absent⟩, closed⟩ := checked
    have contained : outside.contains consumer = true := by
      clear absent
      induction path with
      | entry _ => exact entry
      | @step before after _ edge distinct ih =>
        have move := List.all_eq_true.mp closed _ edge
        change (!outside.contains before || after == producer || outside.contains after) = true at move
        rw [ih, beq_eq_false_iff_ne.mpr distinct] at move
        exact move
    rw [contained] at absent
    contradiction

theorem sites_sound (edges : Edges) (sites : List ReuseSite)
    (verified : sites.all (check edges) = true) (site : ReuseSite) (member : site ∈ sites) :
    ¬ Avoids edges site.producer site.consumer :=
  dominance edges site (List.all_eq_true.mp verified _ member)

end SolarTV.MIR.ScalarReuse
