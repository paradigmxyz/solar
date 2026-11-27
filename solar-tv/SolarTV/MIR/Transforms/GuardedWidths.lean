import SolarTV.MIR.Effects

/-! Width facts from the reached arm of unsigned comparisons. An edge cut
checks paths from entry and from the operand's own definition, so revisiting
a definition in a loop cannot silently reuse a stale check. -/

namespace SolarTV.MIR.GuardedWidths
open EvmRules

theorem upper_of_lt (word limit : Word) (accepted : MIR.compare .lt word limit = 1) :
    word.toNat < limit.toNat := by
  simpa [MIR.compare, BitVec.ult_iff_toNat_lt] using accepted

theorem upper_of_not_lt (word limit : Word) (accepted : MIR.compare .lt limit word = 0) :
    word.toNat ≤ limit.toNat := by
  simpa [MIR.compare, BitVec.ult_iff_toNat_lt] using accepted

theorem masked_upper (word mask : Word) :
    (MIR.eval .and word mask).toNat ≤ mask.toNat := by
  change (word &&& mask).toNat ≤ mask.toNat
  rw [BitVec.toNat_and]
  exact Nat.and_le_right

theorem sub_upper (left right : Word) (accepted : MIR.compare .lt left right = 0) :
    (MIR.eval .sub left right).toNat ≤ left.toNat := by
  have order := upper_of_not_lt right left accepted
  change (left - right).toNat ≤ left.toNat
  rw [BitVec.toNat_sub_of_le order]
  exact Nat.sub_le ..

theorem mask (word : Word) (bits : Nat) (fits : bits ≤ 256)
    (small : word.toNat < 2^bits) :
    MIR.eval .and word (BitVec.ofNat 256 (2^bits - 1)) = word :=
  MIR.Effects.mask_of_bound 256 bits word fits small

theorem widen_bool (word : Word) (small : word.toNat < 2) :
    (MIR.compare .ne word 0).setWidth 256 = word := by
  have widened : (MIR.compare .ne word 0).setWidth 256 =
      (if word = 0 then 0 else 1 : Word) := by
    by_cases zero : word = 0#256 <;> simp [MIR.compare, zero]
  rw [widened]
  exact MIR.Effects.bool_of_bound word (by omega)

theorem compare_bool (word : Word) (flag : BitVec 1) (small : word.toNat < 2) :
    MIR.compare .ne word (flag.setWidth 256) = MIR.compare .ne flag (MIR.compare .ne word 0) ∧
    MIR.compare .ne (flag.setWidth 256) word = MIR.compare .ne flag (MIR.compare .ne word 0) := by
  let bit := MIR.compare .ne word 0
  have widen : bit.setWidth 256 = word := widen_bool word small
  have narrow (a b : BitVec 1) :
      MIR.compare .ne (a.setWidth 256) (b.setWidth 256) = MIR.compare .ne b a ∧
      MIR.compare .ne (b.setWidth 256) (a.setWidth 256) = MIR.compare .ne b a := by
    rcases BitVec.eq_zero_or_eq_one a with rfl | rfl <;>
      rcases BitVec.eq_zero_or_eq_one b with rfl | rfl <;> decide
  change MIR.compare .ne word (flag.setWidth 256) = MIR.compare .ne flag bit ∧
    MIR.compare .ne (flag.setWidth 256) word = MIR.compare .ne flag bit
  simpa only [widen] using narrow bit flag

theorem shl_mask (word amount : Word) (bits : Nat) (fits : bits ≤ 256)
    (discarded : 256 ≤ bits + amount.toNat) :
    MIR.shift .shl (MIR.eval .and word (BitVec.ofNat 256 (2^bits-1))) amount =
      MIR.shift .shl word amount := by
  unfold MIR.shift Word.shift
  split
  · rfl
  · apply BitVec.eq_of_getLsbD_eq
    intro index inside
    have hi : index - amount.toNat < bits ∨ index < amount.toNat := by omega
    have validMask : 2^bits-1 < 2^256 :=
      Nat.sub_one_lt_of_le (Nat.two_pow_pos _) (Nat.pow_le_pow_right (by decide) fits)
    change ((word &&& BitVec.ofNat 256 (2^bits-1)) <<< amount.toNat).getLsbD index =
      (word <<< amount.toNat).getLsbD index
    have bit : (BitVec.ofNat 256 (2^bits-1)).getLsbD (index-amount.toNat) =
        decide (index-amount.toNat < bits) := by
      rw [← BitVec.testBit_toNat, BitVec.toNat_ofNat, Nat.mod_eq_of_lt validMask,
        Nat.testBit_two_pow_sub_one]
    rcases hi with h | h <;>
      simp [BitVec.getLsbD_shiftLeft, BitVec.getLsbD_and, bit, h]

abbrev Edge := Nat × Nat

structure Site where
  definition : Nat
  useBlock : Nat
  admitted : Edge
  outside : List Nat

def check (edges : List Edge) (site : Site) : Bool :=
  site.outside.contains 0 && site.outside.contains site.definition &&
    !site.outside.contains site.useBlock &&
    edges.all (fun edge =>
      edge == site.admitted || !site.outside.contains edge.1 || site.outside.contains edge.2)

inductive Bypass (edges : List Edge) (site : Site) : Nat → Prop where
  | entry : Bypass edges site 0
  | reset : Bypass edges site site.definition
  | step {a b : Nat} : Bypass edges site a → (a,b) ∈ edges →
      (a,b) ≠ site.admitted → Bypass edges site b

theorem sound (edges : List Edge) (site : Site) (valid : check edges site = true) :
    ¬ Bypass edges site site.useBlock := by
  simp only [check, Bool.and_eq_true] at valid
  obtain ⟨⟨⟨entry, reset⟩, absent⟩, closed⟩ := valid
  have inside {i : Nat} (path : Bypass edges site i) : site.outside.contains i = true := by
    induction path with
    | entry => exact entry
    | reset => exact reset
    | @step a b _ edge other ih =>
      have member := List.all_eq_true.mp closed _ edge
      change ((a,b) == site.admitted || !site.outside.contains a || site.outside.contains b) = true at member
      rw [beq_eq_false_iff_ne.mpr other, ih] at member
      exact member
  intro reached
  rw [inside reached] at absent
  contradiction

theorem sites_sound (edges : List Edge) (sites : List Site)
    (valid : sites.all (check edges) = true) (site : Site) (member : site ∈ sites) :
    ¬ Bypass edges site site.useBlock :=
  sound edges site (List.all_eq_true.mp valid _ member)

end SolarTV.MIR.GuardedWidths
