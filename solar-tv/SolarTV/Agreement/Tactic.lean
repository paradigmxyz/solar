import SolarTV.MIR.Ops
import SolarTV.EVMIR.Ops
import EvmRules.Tactic
import EvmRules.Reference

/-! Tactics for the generated agreement theorems of `Encoder.lean`.

Every theorem is proved by rewriting with the lemmas below and checked by the kernel alone:
no SAT solver, and `decide +kernel` only on closed terms. The EVM opcodes go through the
second transcription of `EvmRules.Reference`, the MIR operations through lemmas about
zero-extended operands, and divisions, remainders, products and the powers of `EXP` through
`BitVec.toNat`, as do the abstract tier's laws. -/

namespace SolarTV.Agreement

open EvmRules

/-- SMT-LIB's unsigned division by zero gives all ones; the guard the encoder puts
around it gives zero, as `DIV` does. -/
theorem guarded_div (a b : Word) :
    (if b = 0#256 then 0#256 else BitVec.smtUDiv a b) = Evm.div a b := by
  by_cases h : b = 0#256 <;> simp [h, BitVec.smtUDiv, Evm.div]

theorem guarded_sdiv (a b : Word) :
    (if b = 0#256 then 0#256 else BitVec.smtSDiv a b) = Evm.sdiv a b := by
  by_cases h : b = 0#256
  · simp [h, Evm.sdiv]
  · have h' : -b ≠ 0#256 := by simpa using h
    simp only [h, ite_false, BitVec.smtSDiv, BitVec.sdiv, BitVec.smtUDiv, Evm.sdiv]
    split <;> simp_all

theorem guarded_mod (a b : Word) :
    (if b = 0#256 then 0#256 else BitVec.umod a b) = Evm.mod a b := by
  by_cases h : b = 0#256 <;> simp [h, Evm.mod, BitVec.umod_eq]

theorem guarded_smod (a b : Word) :
    (if b = 0#256 then 0#256 else BitVec.srem a b) = Evm.smod a b := by
  by_cases h : b = 0#256 <;> simp [h, Evm.smod]

set_option exponentiation.threshold 512 in
theorem guarded_addmod (a b n : Word) :
    (if n = 0#256 then 0#256 else BitVec.extractLsb' 0 256
      (BitVec.umod (BitVec.setWidth 257 a + BitVec.setWidth 257 b) (BitVec.setWidth 257 n))) =
      Evm.addmod a b n := by
  rw [← BitVec.setWidth_eq_extractLsb' (by decide)]
  by_cases h : n = 0#256
  · simp [h, Evm.addmod]
  · have h0 : ¬n = 0 := h
    simp only [h, h0, ite_false, Evm.addmod, beq_iff_eq, Bool.cond_eq_ite, BitVec.umod_eq]
    apply BitVec.eq_of_toNat_eq
    have ha := a.isLt
    have hb := b.isLt
    have hn := n.isLt
    have h1 : (2:Nat) ^ 257 = 2 * 2 ^ 256 := by rw [Nat.pow_succ, Nat.mul_comm]
    have h2 : (2:Nat) ^ 512 = 2 ^ 256 * 2 ^ 256 := by rw [← Nat.pow_add]
    have h3 : 2 ≤ (2:Nat) ^ 256 := Nat.le_self_pow (by decide) 2
    have h4 : 2 * 2 ^ 256 ≤ (2:Nat) ^ 256 * 2 ^ 256 := Nat.mul_le_mul_right _ h3
    simp only [BitVec.toNat_setWidth, BitVec.toNat_umod, BitVec.toNat_add,
      Nat.mod_eq_of_lt (show a.toNat < 2 ^ 512 by omega),
      Nat.mod_eq_of_lt (show b.toNat < 2 ^ 512 by omega),
      Nat.mod_eq_of_lt (show n.toNat < 2 ^ 512 by omega),
      Nat.mod_eq_of_lt (show a.toNat < 2 ^ 257 by omega),
      Nat.mod_eq_of_lt (show b.toNat < 2 ^ 257 by omega),
      Nat.mod_eq_of_lt (show n.toNat < 2 ^ 257 by omega),
      Nat.mod_eq_of_lt (show a.toNat + b.toNat < 2 ^ 512 by omega),
      Nat.mod_eq_of_lt (show a.toNat + b.toNat < 2 ^ 257 by omega)]

theorem guarded_mulmod (a b n : Word) :
    (if n = 0#256 then 0#256 else BitVec.extractLsb' 0 256
      (BitVec.umod (BitVec.setWidth 512 a * BitVec.setWidth 512 b) (BitVec.setWidth 512 n))) =
      Evm.mulmod a b n := by
  rw [← BitVec.setWidth_eq_extractLsb' (by decide)]
  by_cases h : n = 0#256
  · simp [h, Evm.mulmod]
  · simp [h, Evm.mulmod, BitVec.umod_eq]

/-- The value of a power, which `BitVec` defines by repeated multiplication. -/
theorem toNat_pow {w : Nat} (x : BitVec w) (n : Nat) : (x ^ n).toNat = x.toNat ^ n % 2 ^ w := by
  induction n with
  | zero => simp [BitVec.pow_zero, BitVec.toNat_ofNat]
  | succ n ih => rw [BitVec.pow_succ, BitVec.toNat_mul, ih, Nat.pow_succ, Nat.mod_mul_mod]

/-- A base `2^k` raised to `e` is `1 <<< (e k)`, zero once `e k` reaches the width: the
encoder's limit `L` is the least exponent with `L k ≥ 256`, below which `e k` cannot wrap. -/
theorem exp_power_of_two (k L : Nat) (e : Word) (hk : k < 256) (hL : 256 ≤ L * k)
    (hL' : (L - 1) * k < 256) (hL256 : L ≤ 256) :
    (if e ≥ BitVec.ofNat 256 L then 0#256 else 1#256 <<< (e * BitVec.ofNat 256 k)) =
      Evm.exp (BitVec.ofNat 256 (2 ^ k)) e := by
  apply BitVec.eq_of_toNat_eq
  have h256 : 256 < 2 ^ 256 := Nat.lt_two_pow_self
  have hpow : 2 ^ k < 2 ^ 256 := Nat.pow_lt_pow_right (by decide) hk
  have hL2 : L < 2 ^ 256 := by omega
  have hk2 : k < 2 ^ 256 := by omega
  simp only [Evm.exp, toNat_pow, BitVec.toNat_ofNat, Nat.mod_eq_of_lt hpow, ← Nat.pow_mul]
  simp only [GE.ge, BitVec.le_def, BitVec.toNat_ofNat, Nat.mod_eq_of_lt hL2]
  by_cases he : L ≤ e.toNat
  · simp only [he, ↓reduceIte]
    have : L * k ≤ e.toNat * k := Nat.mul_le_mul_right k he
    obtain ⟨d, hd⟩ : ∃ d, k * e.toNat = 256 + d := ⟨k * e.toNat - 256, by
      rw [Nat.mul_comm k]; omega⟩
    rw [hd, Nat.pow_add, Nat.mul_mod_right]
    rfl
  · simp only [he, ↓reduceIte]
    have : e.toNat ≤ L - 1 := by omega
    have hsmall : e.toNat * k < 256 := Nat.lt_of_le_of_lt (Nat.mul_le_mul_right k this) hL'
    rw [BitVec.shiftLeft_eq', BitVec.toNat_shiftLeft, BitVec.toNat_mul, BitVec.toNat_ofNat,
      BitVec.toNat_ofNat, Nat.mod_eq_of_lt hk2, Nat.mod_eq_of_lt (by omega : e.toNat * k < 2 ^ 256),
      Nat.shiftLeft_eq, Nat.mul_comm (e.toNat) k]
    simp

theorem exp_zero_base (e : Word) :
    (if e = 0#256 then 1#256 else 0#256) = Evm.exp 0#256 e := by
  apply BitVec.eq_of_toNat_eq
  by_cases hz : e = 0#256
  · simp [hz, Evm.exp]
  · have : e.toNat ≠ 0 := fun hn => hz (BitVec.eq_of_toNat_eq (by simp [hn]))
    simp [hz, Evm.exp, toNat_pow, Nat.zero_pow (Nat.pos_of_ne_zero this)]

theorem exp_one_base (e : Word) : 1#256 = Evm.exp 1#256 e := by
  apply BitVec.eq_of_toNat_eq
  simp [Evm.exp, toNat_pow]

/-- Low bits as the mask the encoder applies, for the narrow widths. -/
theorem mask_160 (t : Word) :
    t &&& 0x000000000000000000000000ffffffffffffffffffffffffffffffffffffffff#256 =
      (t.setWidth 160).setWidth 256 := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_and, BitVec.toNat_setWidth, BitVec.toNat_setWidth,
    show (0x000000000000000000000000ffffffffffffffffffffffffffffffffffffffff#256).toNat =
      2 ^ 160 - 1 from rfl,
    Nat.and_two_pow_sub_one_eq_mod, Nat.mod_eq_of_lt (a := t.toNat % 2 ^ 160)]
  exact Nat.lt_of_lt_of_le (Nat.mod_lt _ (by decide)) (Nat.pow_le_pow_right (by decide) (by decide))

theorem mask_1 (t : Word) :
    t &&& 0x0000000000000000000000000000000000000000000000000000000000000001#256 =
      (t.setWidth 1).setWidth 256 := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_and, BitVec.toNat_setWidth, BitVec.toNat_setWidth,
    show (0x0000000000000000000000000000000000000000000000000000000000000001#256).toNat =
      2 ^ 1 - 1 from rfl,
    Nat.and_two_pow_sub_one_eq_mod, Nat.mod_eq_of_lt (a := t.toNat % 2 ^ 1)]
  exact Nat.lt_of_lt_of_le (Nat.mod_lt _ (by decide)) (Nat.pow_le_pow_right (by decide) (by decide))

theorem setWidth_div {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    x.setWidth 256 / y.setWidth 256 = (x / y).setWidth 256 := by
  apply BitVec.eq_of_toNat_eq
  have hx := x.isLt
  have hy := y.isLt
  have : 2 ^ w ≤ 2 ^ 256 := Nat.pow_le_pow_right (by omega) h
  have hq : x.toNat / y.toNat < 2 ^ w := Nat.lt_of_le_of_lt (Nat.div_le_self _ _) hx
  simp only [BitVec.toNat_udiv, BitVec.toNat_setWidth,
    Nat.mod_eq_of_lt (show x.toNat < 2 ^ 256 by omega),
    Nat.mod_eq_of_lt (show y.toNat < 2 ^ 256 by omega),
    Nat.mod_eq_of_lt (show x.toNat / y.toNat < 2 ^ 256 by omega)]

theorem setWidth_mod {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    x.setWidth 256 % y.setWidth 256 = (x % y).setWidth 256 := by
  apply BitVec.eq_of_toNat_eq
  have hx := x.isLt
  have hy := y.isLt
  have : 2 ^ w ≤ 2 ^ 256 := Nat.pow_le_pow_right (by omega) h
  have hr : x.toNat % y.toNat < 2 ^ w := Nat.lt_of_le_of_lt (Nat.mod_le _ _) hx
  simp only [BitVec.toNat_umod, BitVec.toNat_setWidth,
    Nat.mod_eq_of_lt (show x.toNat < 2 ^ 256 by omega),
    Nat.mod_eq_of_lt (show y.toNat < 2 ^ 256 by omega),
    Nat.mod_eq_of_lt (show x.toNat % y.toNat < 2 ^ 256 by omega)]

theorem setWidth_mul {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    ((x.setWidth 256 * y.setWidth 256).setWidth w).setWidth 256 = (x * y).setWidth 256 := by
  rw [BitVec.setWidth_mul _ _ h, BitVec.setWidth_setWidth_of_le _ h,
    BitVec.setWidth_setWidth_of_le _ h, BitVec.setWidth_eq, BitVec.setWidth_eq]

/-- `SIGNEXTEND`'s shift is eight times the byte index; as a shift it bit-blasts cheaply. -/
theorem mul_eight (a : Word) :
    0x0000000000000000000000000000000000000000000000000000000000000008#256 * a = a <<< 3 := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_mul, BitVec.toNat_shiftLeft, Nat.shiftLeft_eq]
  simp only [BitVec.toNat_ofNat]
  rw [Nat.mul_comm]

theorem beq_zero {a : Word} (h : ¬a = 0#256) : (a == 0) = false := by
  simpa using h

theorem toNat_ne_zero {a : Word} (h : ¬a = 0#256) : a.toNat ≠ 0 :=
  fun hn => h (BitVec.eq_of_toNat_eq (by simp [hn]))

theorem mul_comm (a b : Word) : Evm.mul a b = Evm.mul b a := BitVec.mul_comm _ _

theorem div_le (a b : Word) : Evm.div a b ≤ a := by
  simp only [Evm.div, BitVec.le_def, BitVec.toNat_udiv]
  exact Nat.div_le_self _ _

theorem mod_lt (a b : Word) : ¬b = 0#256 → Evm.mod a b < b := by
  intro h
  simp only [Evm.mod, beq_zero h, Bool.cond_false, BitVec.lt_def, BitVec.toNat_umod]
  exact Nat.mod_lt _ (Nat.pos_of_ne_zero (toNat_ne_zero h))

theorem mod_le (a b : Word) : Evm.mod a b ≤ a := by
  by_cases h : b = 0#256
  · subst h; simp [Evm.mod, BitVec.le_def]
  · simp only [Evm.mod, beq_zero h, Bool.cond_false, BitVec.le_def, BitVec.toNat_umod]
    exact Nat.mod_le _ _

theorem mod_of_lt (a b : Word) : a < b → Evm.mod a b = a := by
  intro h
  have hb : ¬b = 0#256 := by
    intro hb; subst hb; simp [BitVec.lt_def] at h
  simp only [Evm.mod, beq_zero hb, Bool.cond_false]
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_umod]
  exact Nat.mod_eq_of_lt (BitVec.lt_def.mp h)

set_option exponentiation.threshold 512 in
theorem setWidth_umod_lt (x n : BitVec 512) (h : n.toNat ≠ 0) :
    (BitVec.setWidth 256 (x % n)).toNat < n.toNat := by
  simp only [BitVec.toNat_setWidth, BitVec.toNat_umod]
  exact Nat.lt_of_le_of_lt (Nat.mod_le _ _) (Nat.mod_lt _ (Nat.pos_of_ne_zero h))

set_option exponentiation.threshold 512 in
theorem addmod_lt (a b n : Word) : ¬n = 0#256 → Evm.addmod a b n < n := by
  intro h
  have : (2:Nat) ^ 256 ≤ 2 ^ 512 := Nat.pow_le_pow_right (by decide) (by decide)
  have hn : (n.setWidth 512).toNat = n.toNat := by
    simp only [BitVec.toNat_setWidth]; exact Nat.mod_eq_of_lt (by have := n.isLt; omega)
  simp only [Evm.addmod, beq_zero h, Bool.cond_false, BitVec.lt_def]
  have := setWidth_umod_lt (BitVec.setWidth 512 a + BitVec.setWidth 512 b) (n.setWidth 512)
    (by rw [hn]; exact toNat_ne_zero h)
  rwa [hn] at this

set_option exponentiation.threshold 512 in
theorem mulmod_lt (a b n : Word) : ¬n = 0#256 → Evm.mulmod a b n < n := by
  intro h
  have : (2:Nat) ^ 256 ≤ 2 ^ 512 := Nat.pow_le_pow_right (by decide) (by decide)
  have hn : (n.setWidth 512).toNat = n.toNat := by
    simp only [BitVec.toNat_setWidth]; exact Nat.mod_eq_of_lt (by have := n.isLt; omega)
  simp only [Evm.mulmod, beq_zero h, Bool.cond_false, BitVec.lt_def]
  have := setWidth_umod_lt (BitVec.setWidth 512 a * BitVec.setWidth 512 b) (n.setWidth 512)
    (by rw [hn]; exact toNat_ne_zero h)
  rwa [hn] at this

theorem addmod_zero (a b n : Word) : n = 0#256 → Evm.addmod a b n = 0#256 := by
  intro h; subst h; rfl

theorem mulmod_zero (a b n : Word) : n = 0#256 → Evm.mulmod a b n = 0#256 := by
  intro h; subst h; rfl

/-! Kernel proofs of the remaining terms, without a SAT solver: the EVM opcodes through the
second transcription of `EvmRules.Reference`, and the MIR operations at the operand width
through the zero-extension lemmas below. An `i1` operand takes two values, so those
theorems split on them and the kernel evaluates each closed case. -/

theorem bv1_cases (x : BitVec 1) : x = 0#1 ∨ x = 1#1 := by
  have h := x.isLt
  have : x.toNat = 0 ∨ x.toNat = 1 := by omega
  rcases this with h | h
  · exact Or.inl (BitVec.eq_of_toNat_eq (by simp [h]))
  · exact Or.inr (BitVec.eq_of_toNat_eq (by simp [h]))

theorem enc_lt (a b : Word) :
    (if a < b then 1#256 else 0#256) = Evm.lt a b := by
  rw [lt_agrees]; rfl

theorem enc_gt (a b : Word) :
    (if a > b then 1#256 else 0#256) = Evm.gt a b := by
  rw [gt_agrees]; rfl

theorem enc_slt (a b : Word) :
    (if (BitVec.slt a b = true) then 1#256 else 0#256) = Evm.slt a b := by
  rw [slt_agrees]; rfl

theorem enc_sgt (a b : Word) :
    (if (BitVec.slt b a = true) then 1#256 else 0#256) = Evm.sgt a b := by
  rw [sgt_agrees]; rfl

theorem enc_eq (a b : Word) :
    (if a = b then 1#256 else 0#256) = Evm.eq a b := by
  rw [eq_agrees]; rfl

theorem enc_iszero (a : Word) :
    (if a = 0#256 then 1#256 else 0#256) = Evm.iszero a := by
  rw [iszero_agrees]; rfl

theorem enc_shl (a b : Word) :
    (if a ≥ 256#256 then 0#256 else b <<< a) = Evm.shl a b := by
  rw [shl_agrees]
  unfold Reference.shl
  by_cases h : a ≥ 256#256
  · rw [ite_eq_left h, ite_eq_right (by simpa [BitVec.le_def, BitVec.lt_def] using h)]
  · rw [ite_eq_right h, ite_eq_left (by simpa [BitVec.le_def, BitVec.lt_def] using h), BitVec.shiftLeft_eq']

theorem enc_shr (a b : Word) :
    (if a ≥ 256#256 then 0#256 else b >>> a) = Evm.shr a b := by
  rw [shr_agrees]
  unfold Reference.shr
  by_cases h : a ≥ 256#256
  · rw [ite_eq_left h, ite_eq_right (by simpa [BitVec.le_def, BitVec.lt_def] using h)]
  · rw [ite_eq_right h, ite_eq_left (by simpa [BitVec.le_def, BitVec.lt_def] using h), BitVec.ushiftRight_eq']

/-- Shifting by 255 already copies the sign bit into every bit. -/
theorem sshiftRight_255 (b : Word) : b.sshiftRight 255 = b.sshiftRight 256 := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [BitVec.getLsbD_sshiftRight, BitVec.msb_eq_getLsbD_last]
  by_cases h0 : i = 0
  · subst h0; simp
  · simp [show ¬255 + i < 256 by omega, show ¬256 + i < 256 by omega]

theorem enc_sar (a b : Word) :
    (if a ≥ 256#256 then BitVec.sshiftRight' b 255#256 else BitVec.sshiftRight' b a) =
      Evm.sar a b := by
  rw [sar_agrees]
  unfold Reference.sar
  by_cases h : a ≥ 256#256
  · rw [ite_eq_left h, ite_eq_right (by simpa [BitVec.le_def, BitVec.lt_def] using h),
      BitVec.sshiftRight_eq']
    exact sshiftRight_255 b
  · rw [ite_eq_right h, ite_eq_left (by simpa [BitVec.le_def, BitVec.lt_def] using h),
      BitVec.sshiftRight_eq']

theorem enc_byte (a b : Word) :
    (if a ≥ 32#256 then 0#256 else (b >>> ((31#256 - a) * 8#256)) &&& 255#256) =
      Evm.byte a b := by
  rw [byte_agrees]
  unfold Reference.byte
  by_cases h : a ≥ 32#256
  · rw [ite_eq_left h, ite_eq_right (by simpa [BitVec.le_def, BitVec.lt_def] using h)]
  · have hc : ((31#256 - a) * 8#256).toNat < 256 := by
      simp only [GE.ge, BitVec.le_def, BitVec.toNat_ofNat] at h
      bv_omega
    rw [ite_eq_right h, ite_eq_left (by simpa [BitVec.le_def, BitVec.lt_def] using h), Reference.shr,
      ite_eq_left (by simpa [BitVec.lt_def] using hc), BitVec.ushiftRight_eq']

theorem enc_signextend (a b : Word) :
    (if a ≥ 31#256 then b else
      BitVec.sshiftRight' (b <<< (248#256 - 8#256 * a)) (248#256 - 8#256 * a)) =
      Evm.signextend a b := by
  rw [signextend_agrees]
  unfold Reference.signextend
  by_cases h : a ≥ 31#256
  · rw [ite_eq_left h, ite_eq_right (by simpa [BitVec.le_def, BitVec.lt_def] using h)]
  · have hc : (248#256 - a * 8#256).toNat < 256 := by
      simp only [GE.ge, BitVec.le_def, BitVec.toNat_ofNat] at h
      bv_omega
    rw [ite_eq_right h, ite_eq_left (by simpa [BitVec.le_def, BitVec.lt_def] using h), Reference.sar,
      Reference.shl, ite_eq_left (by simpa [BitVec.lt_def] using hc),
      ite_eq_left (by simpa [BitVec.lt_def] using hc), BitVec.mul_comm 8#256 a,
      BitVec.sshiftRight_eq', BitVec.shiftLeft_eq']

theorem toNat_setWidth_256 {w : Nat} (x : BitVec w) (h : w ≤ 256) :
    (x.setWidth 256).toNat = x.toNat := by
  rw [BitVec.toNat_setWidth,
    Nat.mod_eq_of_lt (Nat.lt_of_lt_of_le x.isLt (Nat.pow_le_pow_right (by decide) h))]

theorem setWidth_256_inj {w : Nat} {x y : BitVec w} (h : w ≤ 256) :
    x.setWidth 256 = y.setWidth 256 ↔ x = y := by
  constructor
  · intro e
    apply BitVec.eq_of_toNat_eq
    have := congrArg BitVec.toNat e
    rwa [toNat_setWidth_256 _ h, toNat_setWidth_256 _ h] at this
  · intro e; rw [e]

theorem setWidth_256_lt {w : Nat} {x y : BitVec w} (h : w ≤ 256) :
    x.setWidth 256 < y.setWidth 256 ↔ x < y := by
  simp only [BitVec.lt_def, toNat_setWidth_256 _ h]

theorem setWidth_256_ge {w : Nat} {x : BitVec w} (h : w ≤ 256) :
    x.setWidth 256 ≥ 256#256 ↔ 256 ≤ x.toNat := by
  simp only [GE.ge, BitVec.le_def, toNat_setWidth_256 _ h, BitVec.toNat_ofNat]

theorem low_ext {w : Nat} (x : BitVec w) (h : w ≤ 256) : (x.setWidth 256).setWidth w = x := by
  rw [BitVec.setWidth_setWidth_of_le _ h, BitVec.setWidth_eq]

theorem low_add {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (x.setWidth 256 + y.setWidth 256).setWidth w = x + y := by
  rw [BitVec.setWidth_add _ _ h, low_ext _ h, low_ext _ h]

theorem low_sub {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (x.setWidth 256 - y.setWidth 256).setWidth w = x - y := by
  rw [BitVec.sub_eq_add_neg, BitVec.setWidth_add _ _ h, BitVec.setWidth_neg_of_le h,
    low_ext _ h, low_ext _ h, ← BitVec.sub_eq_add_neg]

theorem low_and {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (x.setWidth 256 &&& y.setWidth 256).setWidth w = x &&& y := by
  rw [BitVec.setWidth_and, low_ext _ h, low_ext _ h]

theorem low_or {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (x.setWidth 256 ||| y.setWidth 256).setWidth w = x ||| y := by
  rw [BitVec.setWidth_or, low_ext _ h, low_ext _ h]

theorem low_xor {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (x.setWidth 256 ^^^ y.setWidth 256).setWidth w = x ^^^ y := by
  rw [BitVec.setWidth_xor, low_ext _ h, low_ext _ h]

theorem low_not {w : Nat} (x : BitVec w) (h : w ≤ 256) :
    (~~~x.setWidth 256).setWidth w = ~~~x := by
  rw [BitVec.setWidth_not h, low_ext _ h]

theorem low_mod {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (bif y.setWidth 256 == 0 then 0 else (x % y).setWidth 256).setWidth w =
      if y = 0 then 0 else x % y := by
  by_cases hy : y = 0
  · subst hy; simp
  · have : (y.setWidth 256 == 0) = false := by
      rw [beq_eq_false_iff_ne, ne_eq]
      intro e
      exact hy ((setWidth_256_inj h).mp (e.trans (BitVec.setWidth_zero 256 w).symm))
    rw [this, Bool.cond_false, ite_eq_right hy, low_ext _ h]

theorem low_shl {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (if x.setWidth 256 ≥ 256#256 then 0#256 else y.setWidth 256 <<< x.setWidth 256).setWidth w =
      y <<< x.toNat := by
  rw [BitVec.shiftLeft_eq', toNat_setWidth_256 _ h]
  by_cases hx : x.setWidth 256 ≥ 256#256
  · rw [ite_eq_left hx, BitVec.shiftLeft_eq_zero (by have := (setWidth_256_ge h).mp hx; omega)]
    simp
  · rw [ite_eq_right hx, BitVec.setWidth_shiftLeft_of_le h, low_ext _ h]

theorem low_shr {w : Nat} (x y : BitVec w) (h : w ≤ 256) :
    (if x.setWidth 256 ≥ 256#256 then 0#256 else y.setWidth 256 >>> x.setWidth 256).setWidth w =
      y >>> x.toNat := by
  rw [BitVec.ushiftRight_eq', toNat_setWidth_256 _ h]
  by_cases hx : x.setWidth 256 ≥ 256#256
  · rw [ite_eq_left hx, BitVec.ushiftRight_eq_zero (by have := (setWidth_256_ge h).mp hx; omega)]
    simp
  · rw [ite_eq_right hx, ← BitVec.setWidth_ushiftRight h, low_ext _ h]

theorem word_mod_guard (x y : Word) :
    (bif y == 0 then 0 else x % y) = if y = 0 then 0 else x % y := by
  by_cases hy : y = 0 <;> simp [hy]

theorem word_smod_guard (x y : Word) :
    (bif y == 0 then 0 else x.srem y) = if y = 0 then 0 else x.srem y := by
  by_cases hy : y = 0 <;> simp [hy]

theorem word_shl_guard (x y : Word) :
    (if x ≥ 256#256 then 0#256 else y <<< x) = y <<< x.toNat := by
  rw [enc_shl, shl_agrees]; unfold Reference.shl
  by_cases hx : x < 256#256
  · rw [ite_eq_left hx]
  · rw [ite_eq_right hx, BitVec.shiftLeft_eq_zero (by simp [BitVec.lt_def] at hx; omega)]

theorem word_shr_guard (x y : Word) :
    (if x ≥ 256#256 then 0#256 else y >>> x) = y >>> x.toNat := by
  rw [enc_shr, shr_agrees]; unfold Reference.shr
  by_cases hx : x < 256#256
  · rw [ite_eq_left hx]
  · rw [ite_eq_right hx, BitVec.ushiftRight_eq_zero (by simp [BitVec.lt_def] at hx; omega)]

theorem word_sar_guard (x y : Word) :
    (if x ≥ 256#256 then y.sshiftRight' 255#256 else y.sshiftRight' x) =
      y.sshiftRight x.toNat := by
  rw [enc_sar, sar_agrees]; unfold Reference.sar
  by_cases hx : x < 256#256
  · rw [ite_eq_left hx]
  · rw [ite_eq_right hx]
    exact BitVec.sshiftRight_eq_sshiftRight_of_le (by decide)
      (by simp [BitVec.lt_def] at hx; omega)

theorem sext_160_256 (x : BitVec 160) :
    BitVec.signExtend 256 (BitVec.extractLsb' 0 160 (BitVec.setWidth 256 x)) &&&
      115792089237316195423570985008687907853269984665640564039457584007913129639935#256 =
    BitVec.signExtend 256 x := by
  rw [← BitVec.setWidth_eq_extractLsb' (by decide), low_ext _ (by decide),
    show (115792089237316195423570985008687907853269984665640564039457584007913129639935#256) =
      BitVec.allOnes 256 from by decide +kernel, BitVec.and_allOnes]

/-- The abstract tier's laws of the exact operation: the uninterpreted function is the
operation, so the first law holds by definition and each other one by a lemma above. -/
macro "agree_abstract" : tactic =>
  `(tactic| (
    subst_vars
    simp only [*, true_and]
    repeat' apply And.intro
    all_goals first
      | with_reducible exact mul_comm _ _
      | with_reducible exact div_le _ _
      | with_reducible exact mod_le _ _
      | with_reducible exact mod_lt _ _
      | with_reducible exact mod_of_lt _ _
      | with_reducible exact addmod_lt _ _ _
      | with_reducible exact mulmod_lt _ _ _
      | with_reducible exact addmod_zero _ _ _
      | with_reducible exact mulmod_zero _ _ _
      | (rintro (h | h) <;> simp [h, Evm.mul])
      | (intro h; subst h; simp [Evm.mul, Evm.div, Evm.mod, Evm.addmod, Evm.mulmod])))

/-- The term of a pure opcode is its EVM definition: by definition, through the division
guards, or through the opcode's lemma above. -/
macro "agree_word" : tactic =>
  `(tactic| first
    | rfl
    | simp only [guarded_div, guarded_sdiv, guarded_mod, guarded_smod, guarded_addmod,
        guarded_mulmod]; done
    | exact enc_lt _ _
    | exact enc_gt _ _
    | exact enc_slt _ _
    | exact enc_sgt _ _
    | exact enc_eq _ _
    | exact enc_iszero _
    | exact enc_byte _ _
    | exact enc_shl _ _
    | exact enc_shr _ _
    | exact enc_sar _ _
    | exact enc_signextend _ _)

/-- The constant shapes of `EXP`. -/
macro "agree_exp" : tactic =>
  `(tactic| first
    | exact exp_zero_base _
    | exact exp_one_base _
    | (apply exp_power_of_two <;> decide)
    | (simp only [Evm.exp, BitVec.toNat_ofNat, BitVec.pow_succ, BitVec.pow_zero]; done)
    | (simp [Evm.exp, BitVec.pow_succ, BitVec.pow_zero]; done))

set_option hygiene false in
/-- A MIR operation's term: substitute the zero-extended operands, rewrite masks as low
bits and divisions, remainders and products at the operand width, then each operation at the
operand width by its lemma above. Comparisons split on their condition. An `i1` operand
(`x`, `y` or `z`, the generated theorems' names, hence the unhygienic macro) is split into
its two values, and the kernel evaluates what is then closed. -/
macro "agree_mir" : tactic =>
  `(tactic| (
    subst_vars
    try simp only [guarded_div, guarded_sdiv, guarded_mod, guarded_smod, mask_160, mask_1,
      Evm.div, Evm.mod, Evm.sdiv, Evm.smod, MIR.eval, Word.binary, MIR.not, MIR.select,
      MIR.compare,
      MIR.cast, BitVec.setWidth_eq, setWidth_div _ _ (by decide : 160 ≤ 256),
      setWidth_div _ _ (by decide : 1 ≤ 256), setWidth_mod _ _ (by decide : 160 ≤ 256),
      setWidth_mod _ _ (by decide : 1 ≤ 256), setWidth_mul _ _ (by decide : 160 ≤ 256),
      setWidth_mul _ _ (by decide : 1 ≤ 256)]
    all_goals first
      | rfl
      | (simp only [low_add _ _ (by decide : 160 ≤ 256), low_sub _ _ (by decide : 160 ≤ 256),
          low_and _ _ (by decide : 160 ≤ 256), low_or _ _ (by decide : 160 ≤ 256),
          low_xor _ _ (by decide : 160 ≤ 256), low_not _ (by decide : 160 ≤ 256),
          low_ext _ (by decide : 160 ≤ 256), low_mod _ _ (by decide : 160 ≤ 256),
          low_shl _ _ (by decide : 160 ≤ 256), low_shr _ _ (by decide : 160 ≤ 256),
          word_mod_guard, word_smod_guard, word_shl_guard, word_shr_guard, word_sar_guard,
          sext_160_256]; done)
      | rw [low_shl _ _ (by decide : 160 ≤ 256)]
      | rw [low_shr _ _ (by decide : 160 ≤ 256)]
      | (rcases bv1_cases x with rfl | rfl <;>
          (try rcases bv1_cases y with rfl | rfl) <;>
          (try rcases bv1_cases z with rfl | rfl) <;>
          first | decide +kernel | simp)
      | (try simp only [gt_iff_lt, setWidth_256_lt (by decide : 160 ≤ 256),
          setWidth_256_inj (by decide : 160 ≤ 256), BitVec.ult_eq_decide_lt, decide_eq_true_eq,
          beq_iff_eq, bne_iff_ne, ne_eq]
         split <;> (try split) <;> first | decide +kernel | (exfalso; simp_all))))

end SolarTV.Agreement
