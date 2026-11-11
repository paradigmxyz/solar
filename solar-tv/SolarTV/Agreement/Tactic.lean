import SolarTV.MIR.Ops
import SolarTV.EVMIR.Ops
import EvmRules.Tactic

/-! Tactics for the generated agreement theorems of `Encoder.lean`.

Most theorems are decided by `bv_decide` after unfolding the EVM and MIR definitions.
Division and remainder would make it bit-blast 256-bit dividers, and products 256-bit
multipliers, so these go through rewriting and `BitVec.toNat` instead, as do the
powers of `EXP` and the abstract tier's laws. -/

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
  bv_decide

theorem mask_1 (t : Word) :
    t &&& 0x0000000000000000000000000000000000000000000000000000000000000001#256 =
      (t.setWidth 1).setWidth 256 := by
  bv_decide

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
  bv_decide

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
guards, or by bit-blasting what is left. -/
macro "agree_word" : tactic =>
  `(tactic| first
    | rfl
    | simp only [guarded_div, guarded_sdiv, guarded_mod, guarded_smod, guarded_addmod,
        guarded_mulmod]; done
    | (try rw [mul_eight]
       evm_unfold
       bv_decide (config := { timeout := 300 })))

/-- The constant shapes of `EXP`. -/
macro "agree_exp" : tactic =>
  `(tactic| first
    | exact exp_zero_base _
    | exact exp_one_base _
    | (apply exp_power_of_two <;> decide)
    | (simp only [Evm.exp, BitVec.toNat_ofNat, BitVec.pow_succ, BitVec.pow_zero]; done)
    | (simp [Evm.exp, BitVec.pow_succ, BitVec.pow_zero]; done))

/-- A MIR operation's term: substitute the zero-extended operands, rewrite masks as low
bits and divisions, remainders and products at the operand width, then bit-blast. -/
macro "agree_mir" : tactic =>
  `(tactic| (
    subst_vars
    try simp only [guarded_div, guarded_sdiv, guarded_mod, guarded_smod, mask_160, mask_1,
      Evm.div, Evm.mod, Evm.sdiv, Evm.smod, MIR.eval, MIR.not, MIR.select, MIR.compare,
      MIR.cast, BitVec.setWidth_eq, setWidth_div _ _ (by decide : 160 ≤ 256),
      setWidth_div _ _ (by decide : 1 ≤ 256), setWidth_mod _ _ (by decide : 160 ≤ 256),
      setWidth_mod _ _ (by decide : 1 ≤ 256), setWidth_mul _ _ (by decide : 160 ≤ 256),
      setWidth_mul _ _ (by decide : 1 ≤ 256)]
    all_goals first
      | rfl
      | (try generalize _ / _ = q at *
         try generalize _ % _ = r at *
         bv_decide)))

end SolarTV.Agreement
