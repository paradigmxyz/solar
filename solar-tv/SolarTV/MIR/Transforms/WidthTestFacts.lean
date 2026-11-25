import SolarTV.MIR.Ops

/-! Truth conditions for typed MIR width guards and their use in branches. -/

namespace SolarTV.MIR.WidthTestFacts

theorem zero_shift {width : Nat} (word : BitVec width) (amount : Nat) :
    word >>> amount = 0#width ↔ word.toNat < 2^amount := by
  rw [← BitVec.toNat_inj, BitVec.toNat_ushiftRight]
  change word.toNat >>> amount = 0 ↔ word.toNat < 2^amount
  rw [Nat.shiftRight_eq_div_pow]
  exact Nat.div_eq_zero_iff_lt (Nat.two_pow_pos _)

theorem widen_zero (x : BitVec 1) : x.setWidth 256 = 0#256 ↔ x = 0#1 := by
  rcases BitVec.eq_zero_or_eq_one x with h | h <;> rw [h] <;> decide

theorem flag_zero (x : BitVec 1) : x = 0#1 ↔ x ≠ 1#1 := by
  rcases BitVec.eq_zero_or_eq_one x with h | h <;> rw [h] <;> decide

theorem flag_or (x y : BitVec 1) : (x ||| y) = 1#1 ↔ x = 1#1 ∨ y = 1#1 := by
  rcases BitVec.eq_zero_or_eq_one x with h | h <;> rw [h] <;>
    rcases BitVec.eq_zero_or_eq_one y with h | h <;> rw [h] <;> decide

theorem flag_and (x y : BitVec 1) : (x &&& y) = 1#1 ↔ x = 1#1 ∧ y = 1#1 := by
  rcases BitVec.eq_zero_or_eq_one x with h | h <;> rw [h] <;>
    rcases BitVec.eq_zero_or_eq_one y with h | h <;> rw [h] <;> decide

theorem cmp_eq {width : Nat} (a b : BitVec width) :
    MIR.compare .eq a b = 1#1 ↔ a = b := by
  by_cases h : a = b <;> simp [MIR.compare, h]

theorem cmp_ne {width : Nat} (a b : BitVec width) :
    MIR.compare .ne a b = 1#1 ↔ a ≠ b := by
  by_cases h : a = b <;> simp [MIR.compare, h]

theorem cmp_lt {width : Nat} (a b : BitVec width) :
    MIR.compare .lt a b = 1#1 ↔ a.toNat < b.toNat := by
  by_cases h : a.toNat < b.toNat <;> simp [MIR.compare, BitVec.ult, h]

theorem cmp_gt {width : Nat} (a b : BitVec width) :
    MIR.compare .gt a b = 1#1 ↔ b.toNat < a.toNat := cmp_lt b a

theorem same_branch (left right : BitVec 1) {Outcome : Type} (yes no : Outcome)
    (equal : left = 1#1 ↔ right = 1#1) :
    MIR.branch left yes no = MIR.branch right yes no := by
  change (if left = 1#1 then yes else no) = (if right = 1#1 then yes else no)
  by_cases h : right = 1#1
  · simp [h, equal.mpr h]
  · have hl : left ≠ 1#1 := fun hl => h (equal.mp hl)
    simp [h, hl]

theorem flip_branch (left right : BitVec 1) {Outcome : Type} (yes no : Outcome)
    (equal : left = 1#1 ↔ ¬ right = 1#1) :
    MIR.branch left yes no = MIR.branch right no yes := by
  change (if left = 1#1 then yes else no) = (if right = 1#1 then no else yes)
  by_cases h : right = 1#1
  · have hl : left ≠ 1#1 := fun hl => equal.mp hl h
    simp [h, hl]
  · simp [h, equal.mpr h]

end SolarTV.MIR.WidthTestFacts
