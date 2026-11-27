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
  simp [BitVec.toNat_setWidth, Nat.mod_eq_of_lt fits]

private theorem bounded_mask {mask wide : Nat} (word : BitVec wide)
    (bound : word.toNat < 2^mask) (maskFits : mask ≤ wide) :
    word &&& BitVec.ofNat wide (2^mask - 1) = word := by
  have max : 2^mask ≤ 2^wide := Nat.pow_le_pow_right (by decide) maskFits
  have pos := Nat.two_pow_pos mask
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_and, BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega),
    Nat.and_two_pow_sub_one_eq_mod,
    Nat.mod_eq_of_lt bound]

theorem clean_mask {narrow mask wide : Nat} (word : BitVec narrow)
    (narrowFits : narrow ≤ mask) (maskFits : mask ≤ wide) :
    word.setWidth wide &&& BitVec.ofNat wide (2^mask - 1) = word.setWidth wide := by
  apply bounded_mask _ _ maskFits
  rw [BitVec.toNat_setWidth_of_le (Nat.le_trans narrowFits maskFits)]
  exact Nat.lt_of_lt_of_le word.isLt (Nat.pow_le_pow_right (by decide) narrowFits)

theorem packed_low {wide mask : Nat} (high low : BitVec wide)
    (bounded : low.toNat < 2^mask) (fits : mask ≤ wide) :
    ((high &&& BitVec.ofNat wide (2^wide - 2^mask)) ||| low) &&&
      BitVec.ofNat wide (2^mask - 1) = low := by
  let lowMask := BitVec.ofNat wide (2^mask - 1)
  have order : 2^mask ≤ 2^wide := Nat.pow_le_pow_right (by decide) fits
  have positive := Nat.two_pow_pos mask
  have masks : BitVec.ofNat wide (2^wide - 2^mask) = ~~~lowMask := by
    apply BitVec.eq_of_toNat_eq
    simp only [lowMask, BitVec.toNat_ofNat, BitVec.toNat_not,
      Nat.mod_eq_of_lt (show 2^wide - 2^mask < 2^wide by omega),
      Nat.mod_eq_of_lt (show 2^mask - 1 < 2^wide by omega)]
    omega
  change ((high &&& BitVec.ofNat wide (2^wide - 2^mask)) ||| low) &&& lowMask = low
  have lowPart : low &&& lowMask = low := bounded_mask low bounded fits
  rw [masks, BitVec.and_or_distrib_right, BitVec.and_assoc, BitVec.not_and_self, lowPart]
  simp

theorem shift_mask {width mask : Nat} (word amount : BitVec width)
    (bound : width - amount.toNat ≤ mask) (fits : mask ≤ width) :
    MIR.shift .shr word amount &&& BitVec.ofNat width (2^mask - 1) =
      MIR.shift .shr word amount := by
  apply bounded_mask _ _ fits
  unfold MIR.shift Word.shift
  split
  · simp [Nat.two_pow_pos]
  · rw [BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
    apply (Nat.div_lt_iff_lt_mul (Nat.two_pow_pos _)).mpr
    rw [← Nat.pow_add]
    exact Nat.lt_of_lt_of_le word.isLt (Nat.pow_le_pow_right (by decide) (by omega))

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

theorem offset_difference {width : Nat} (base offset : BitVec width) :
    MIR.eval .sub (MIR.eval .add base offset) base = offset := by
  change base + offset - base = offset
  rw [BitVec.add_comm base offset, BitVec.add_sub_cancel]

theorem add_literals (width a b : Nat) :
    MIR.eval .add (BitVec.ofNat width a) (BitVec.ofNat width b) =
      BitVec.ofNat width ((a+b) % 2^width) := by
  change BitVec.ofNat width a + BitVec.ofNat width b = _
  rw [← BitVec.ofNat_add]
  apply BitVec.eq_of_toNat_eq
  simp

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

theorem below_one {width : Nat} (word : BitVec width) (fits : 0 < width) :
    MIR.compare .lt word 1 = MIR.compare .eq word 0 := by
  have bound : 1 < 2^width := Nat.one_lt_pow (by omega) (by decide)
  have zero : word.toNat < 1 ↔ word.toNat = 0 := by omega
  simp [MIR.compare, BitVec.ult, BitVec.toNat_eq, Nat.mod_eq_of_lt bound, zero]

theorem nonmax_tests {width : Nat} (word : BitVec width) :
    MIR.compare .lt word (BitVec.allOnes width) = MIR.compare .ne word (BitVec.allOnes width) ∧
      MIR.compare .ne (MIR.unary .not word) 0 = MIR.compare .ne word (BitVec.allOnes width) := by
  have below : word.ult (BitVec.allOnes width) = (word != BitVec.allOnes width) := by
    rw [Bool.eq_iff_iff, BitVec.ult_iff_lt, bne_iff_ne, BitVec.lt_allOnes_iff]
  have inverse : (~~~word = 0#width) ↔ word = BitVec.allOnes width := by
    calc
      (~~~word = 0#width) ↔ (~~~word = ~~~(BitVec.allOnes width)) := by rw [BitVec.not_allOnes]
      _ ↔ _ := BitVec.not_inj
  simp [MIR.compare, Word.unary, below, inverse]

theorem widened_zero_test {width : Nat} (flag : BitVec 1) (fits : 1 ≤ width) :
    MIR.compare .ne (flag.setWidth width) 0 = flag ∧
      MIR.compare .eq (flag.setWidth width) 0 = MIR.compare .eq flag 0 := by
  have zero : flag.setWidth width = 0#width ↔ flag = 0#1 := by
    rw [← BitVec.toNat_inj, BitVec.toNat_setWidth_of_le fits]
    simp [← BitVec.toNat_inj]
  change (if flag.setWidth width != 0#width then 1 else 0) = flag ∧
    (if flag.setWidth width == 0#width then 1 else 0) = (if flag == 0#1 then 1 else 0)
  simp only [bne_iff_ne, beq_iff_eq, not_congr zero, zero]
  rcases BitVec.eq_zero_or_eq_one flag with rfl | rfl <;> decide

theorem negate_equality {width : Nat} (a b : BitVec width) :
    MIR.compare .eq (MIR.compare .ne a b) 0 = MIR.compare .eq a b ∧
      MIR.compare .eq (MIR.compare .eq a b) 0 = MIR.compare .ne a b := by
  by_cases same : a = b <;> simp [MIR.compare, same]

theorem select_mul (flag : BitVec 1) (word : Word) :
    MIR.eval .mul word ((MIR.compare .eq flag 0).setWidth 256) =
      (if flag = 1 then 0 else word) ∧
    MIR.eval .mul ((MIR.compare .eq flag 0).setWidth 256) word =
      (if flag = 1 then 0 else word) := by
  rcases BitVec.eq_zero_or_eq_one flag with rfl | rfl <;> simp [MIR.eval, MIR.compare]

theorem negate_cut {width : Nat} (word lower : BitVec width)
    (fits : lower.toNat + 1 < 2^width) :
    MIR.compare .eq (MIR.compare .lt lower word) 0 =
      MIR.compare .lt word (BitVec.ofNat width (lower.toNat+1)) := by
  by_cases above : lower.toNat < word.toNat
  · have opposite : ¬ word.toNat < lower.toNat+1 := by omega
    simp [MIR.compare, BitVec.ult, Nat.mod_eq_of_lt fits, above, opposite]
  · have opposite : word.toNat < lower.toNat+1 := by omega
    simp [MIR.compare, BitVec.ult, Nat.mod_eq_of_lt fits, above, opposite]

theorem widened_sub_xor {width : Nat} (a b : BitVec 1) (fits : 1 ≤ width) :
    MIR.compare .ne (MIR.eval .sub (a.setWidth width) (b.setWidth width)) 0 =
      MIR.eval .xor a b := by
  have zero : (a.setWidth width - b.setWidth width = 0#width) ↔ a = b := by
    constructor
    · intro same
      have widened : a.setWidth width = b.setWidth width := by
        calc
          _ = a.setWidth width - b.setWidth width + b.setWidth width :=
            (BitVec.sub_add_cancel _ _).symm
          _ = _ := by simp [same]
      exact (round_trip a fits).symm.trans ((congrArg (BitVec.setWidth 1) widened).trans
        (round_trip b fits))
    · intro same
      simp [same]
  have condition : (a.setWidth width - b.setWidth width != 0#width) = (a != b) := by
    rw [Bool.eq_iff_iff, bne_iff_ne, bne_iff_ne]
    exact not_congr zero
  change (if a.setWidth width - b.setWidth width != 0#width then 1 else 0) = a ^^^ b
  rw [condition]
  rcases BitVec.eq_zero_or_eq_one a with rfl | rfl <;>
    rcases BitVec.eq_zero_or_eq_one b with rfl | rfl <;> decide

theorem widened_xor {width : Nat} (a b : BitVec 1) (fits : 1 ≤ width) :
    MIR.compare .ne (MIR.eval .xor (a.setWidth width) (b.setWidth width)) 0 =
      MIR.eval .xor a b := by
  change MIR.compare .ne (a.setWidth width ^^^ b.setWidth width) 0 = a ^^^ b
  rw [← BitVec.setWidth_xor]
  exact (widened_zero_test (a ^^^ b) fits).1

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

theorem timestamp_value (cfg : Config) (frame : Frame) :
    operation cfg frame (.context (.environment 0x42)) [] =
      value frame cfg.context.timestamp := rfl

theorem returndata_guard (cfg : Config) (frame : Frame)
    (bound : frame.returnData.length < 2^255) :
    let length := BitVec.ofNat 256 frame.returnData.length
    memoryOperation cfg frame .returnDataSize [] = value frame length ∧
      MIR.compare .lt (MIR.LowerReturndata.size length) length = 0 := by
  constructor
  · rfl
  · apply MIR.RoundedExtent.overflow_clear
    rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega)]
    exact bound

/-- The incoming operand is read before any of the destination phis are written. -/
theorem phi_read (frame : Frame) (predecessor result : Nat) (operand : Operand)
    (same : operand.ty.same operand.ty = true) :
    enter (some predecessor) [⟨result,operand.ty,[(predecessor,operand)]⟩] frame =
      (do
        let word ← readValue frame operand
        return {frame with locals := put frame.locals result word}) := by
  simp [enter, same]

/-- A self edge copies the typed value already held by the phi. The
caller proves that a seed edge ran before the first self edge. -/
theorem phi_carry (frame : Frame) (predecessor result : Nat) (ty : ValueType) (seed : Value)
    (same : ty.same ty = true) (fits : seed.fits 34 ty = true)
    (initialized : readValue frame ⟨.local result, ty⟩ = .ok seed) :
    let after := {frame with locals := put frame.locals result seed}
    enter (some predecessor) [⟨result, ty, [(predecessor, ⟨.local result, ty⟩)]⟩] frame = .ok after ∧
      readValue after ⟨.local result, ty⟩ = .ok seed := by
  constructor
  · rw [phi_read frame predecessor result ⟨.local result, ty⟩ same, initialized]
    rfl
  · simp [readValue, put, required, fits, Bind.bind, Except.bind, Pure.pure, Except.pure]

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
