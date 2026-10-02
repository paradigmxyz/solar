import Std.Tactic.BVDecide

namespace Evm

abbrev Word := BitVec 256

def shl (count value : Word) : Word :=
  if count < 256#256 then value <<< count.toNat else 0#256

def shr (count value : Word) : Word :=
  if count < 256#256 then value >>> count.toNat else 0#256

def sar (count value : Word) : Word :=
  if count < 256#256 then value.sshiftRight count.toNat else value.sshiftRight 256

def div (a b : Word) : Word := if b = 0#256 then 0#256 else a / b

def mod (a b : Word) : Word := if b = 0#256 then 0#256 else a % b

def sdiv (a b : Word) : Word := if b = 0#256 then 0#256 else a.sdiv b

def smod (a b : Word) : Word := if b = 0#256 then 0#256 else a.srem b

def addmod (a b n : Word) : Word :=
  if n = 0#256 then 0#256 else ((a.setWidth 512 + b.setWidth 512) % n.setWidth 512).setWidth 256

def mulmod (a b n : Word) : Word :=
  if n = 0#256 then 0#256 else ((a.setWidth 512 * b.setWidth 512) % n.setWidth 512).setWidth 256

def exp (a b : Word) : Word := a ^ b.toNat

def byte (index value : Word) : Word :=
  if index < 32#256 then shr ((31#256 - index) * 8#256) value &&& 255#256 else 0#256

def signextend (index value : Word) : Word :=
  if index < 31#256 then sar (248#256 - index * 8#256) (shl (248#256 - index * 8#256) value) else value

def clz (value : Word) : Word := value.clz

theorem shl_eq (count value : Word) : shl count value = value <<< count.toNat := by
  unfold shl
  split
  · rfl
  · symm
    apply BitVec.shiftLeft_eq_zero
    simp only [BitVec.lt_def, BitVec.toNat_ofNat] at *
    omega

theorem shr_eq (count value : Word) : shr count value = value >>> count.toNat := by
  unfold shr
  split
  · rfl
  · symm
    apply BitVec.ushiftRight_eq_zero
    simp only [BitVec.lt_def, BitVec.toNat_ofNat] at *
    omega

@[simp] theorem mul_shl_one (x count : Word) : x * shl count 1#256 = shl count x := by
  simp only [shl_eq, ← BitVec.twoPow_eq, BitVec.mul_twoPow_eq_shiftLeft]

@[simp] theorem shl_one_mul (x count : Word) : shl count 1#256 * x = shl count x := by
  rw [BitVec.mul_comm, mul_shl_one]

@[simp] theorem div_eq (x y : Word) : div x y = x / y := by
  by_cases h : y = 0#256 <;> simp [div, h]

@[simp] theorem div_lt (x y : Word) (h : x < y) : x / y = 0#256 := by
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_udiv, BitVec.toNat_zero]
  exact Nat.div_eq_of_lt h

@[simp] theorem mod_lt (x y : Word) (h : x < y) : mod x y = x := by
  have hn : y ≠ 0#256 := by bv_omega
  simp [mod, hn, BitVec.umod_eq_of_lt h]

@[simp] theorem div_shl_one (x count : Word) (h : count < 256#256) :
    x / shl count 1#256 = shr count x := by
  rw [shl_eq, ← BitVec.twoPow_eq, shr_eq]
  apply BitVec.udiv_twoPow_eq_of_lt
  exact h

@[simp] theorem mod_shl_one (x count : Word) (h : count < 256#256) :
    mod x (shl count 1#256) = x &&& (shl count 1#256 - 1#256) := by
  have hc : count.toNat < 256 := h
  have hp : 2 ^ count.toNat < 2 ^ 256 := Nat.pow_lt_pow_of_lt (by decide) hc
  have hn : (shl count 1#256).toNat = 2 ^ count.toNat := by
    rw [shl_eq, ← BitVec.twoPow_eq, BitVec.toNat_twoPow, Nat.mod_eq_of_lt hp]
  have hz : shl count 1#256 ≠ 0#256 := by
    intro hz
    have := Nat.two_pow_pos count.toNat
    rw [hz, BitVec.toNat_zero] at hn
    omega
  simp only [mod, hz, ↓reduceIte]
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_umod, BitVec.toNat_and, BitVec.toNat_sub_of_le]
  · simp [hn]
  · change 1 ≤ (shl count 1#256).toNat
    rw [hn]
    exact Nat.two_pow_pos count.toNat

theorem two_pow (n : Nat) : 2#256 ^ n = BitVec.twoPow 256 n := by
  induction n with
  | zero => simp [BitVec.twoPow_zero]
  | succ n ih =>
    rw [BitVec.pow_succ, ih]
    change BitVec.twoPow 256 n * BitVec.twoPow 256 1 = _
    exact BitVec.twoPow_mul_twoPow_eq n 1

@[simp] theorem exp_two (count : Word) : exp 2#256 count = shl count 1#256 := by
  simp only [exp, two_pow, BitVec.twoPow_eq, shl_eq]

@[simp] theorem exp_square (x : Word) : exp x 2#256 = x * x := by
  simp [exp, BitVec.pow_succ]

@[simp] theorem exp_zero (x : Word) : exp x 0#256 = 1#256 := by simp [exp]

@[simp] theorem exp_one (x : Word) : exp x 1#256 = x := by simp [exp]

@[simp] theorem one_exp (x : Word) : exp 1#256 x = 1#256 := by
  unfold exp
  induction x.toNat with
  | zero => rfl
  | succ n ih => simp [BitVec.pow_succ, ih]

def shiftSum (a b : Word) : Word := BitVec.ofNat 256 (min 256 (a.toNat + b.toNat))

@[simp] theorem shiftSum_toNat (a b : Word) :
    (shiftSum a b).toNat = min 256 (a.toNat + b.toNat) := by
  simp only [shiftSum, BitVec.toNat_ofNat]
  apply Nat.mod_eq_of_lt
  have := Nat.min_le_left 256 (a.toNat + b.toNat)
  omega

@[simp] theorem shl_shl (a b x : Word) : shl a (shl b x) = shl (shiftSum a b) x := by
  simp only [shl_eq, shiftSum_toNat, ← BitVec.shiftLeft_add]
  by_cases h : a.toNat + b.toNat < 256
  · rw [Nat.min_eq_right (by omega), Nat.add_comm]
  · rw [Nat.min_eq_left (by omega)]
    rw [BitVec.shiftLeft_eq_zero (by omega), BitVec.shiftLeft_eq_zero (by omega)]

@[simp] theorem shr_shr (a b x : Word) : shr a (shr b x) = shr (shiftSum a b) x := by
  simp only [shr_eq, shiftSum_toNat, ← BitVec.shiftRight_add]
  by_cases h : a.toNat + b.toNat < 256
  · rw [Nat.min_eq_right (by omega), Nat.add_comm]
  · rw [Nat.min_eq_left (by omega)]
    rw [BitVec.ushiftRight_eq_zero (by omega), BitVec.ushiftRight_eq_zero (by omega)]

theorem sar_eq (count value : Word) : sar count value = value.sshiftRight count.toNat := by
  unfold sar
  split
  · rfl
  · apply BitVec.sshiftRight_eq_sshiftRight_of_le (by omega)
    simp only [BitVec.lt_def, BitVec.toNat_ofNat] at *
    omega

@[simp] theorem sar_sar (a b x : Word) : sar a (sar b x) = sar (shiftSum a b) x := by
  simp only [sar_eq, shiftSum_toNat, ← BitVec.sshiftRight_add]
  by_cases h : a.toNat + b.toNat < 256
  · rw [Nat.min_eq_right (by omega), Nat.add_comm]
  · rw [Nat.min_eq_left (by omega)]
    exact BitVec.sshiftRight_eq_sshiftRight_of_le (by omega) (by omega)

@[simp] theorem div_clz (x b : Word) (h : 256#256 < b) : div (clz x) b = 0#256 := by
  rw [div_eq]
  apply div_lt
  have hc := BitVec.clz_le (x := x)
  change x.clz < b
  exact Nat.lt_of_le_of_lt hc h

@[simp] theorem mod_clz (x b : Word) (h : 256#256 < b) : mod (clz x) b = clz x := by
  apply mod_lt
  have hc := BitVec.clz_le (x := x)
  change x.clz < b
  exact Nat.lt_of_le_of_lt hc h

@[simp] theorem mod_self (x : Word) : mod x x = 0#256 := by
  simp [mod]

@[simp] theorem smod_self (x : Word) : smod x x = 0#256 := by
  simp [smod]

theorem sign_bits (n : Nat) (x : Word) (hn : n < 256) (i : Nat) (hi : i < 256) :
    ((x <<< n).sshiftRight n).getLsbD i = x.getLsbD (min i (255 - n)) := by
  simp only [BitVec.getLsbD_sshiftRight, BitVec.getLsbD_shiftLeft, BitVec.msb_eq_getLsbD_last]
  by_cases h : n + i < 256
  · simp [h, show ¬ 256 ≤ i by omega, Nat.min_eq_left (show i ≤ 255 - n by omega), show ¬ n + i < n by omega]
  · simp [h, show ¬ 256 ≤ i by omega, Nat.min_eq_right (show 255 - n ≤ i by omega), show ¬ 255 < n by omega]

theorem signextend_bits (n x : Word) (i : Nat) (hi : i < 256) :
    (signextend n x).getLsbD i = x.getLsbD (min i (n.toNat * 8 + 7)) := by
  unfold signextend
  split
  · rename_i hn
    have hn' : n.toNat < 31 := hn
    have hs : (248#256 - n * 8#256).toNat = 248 - n.toNat * 8 := by
      bv_omega
    simp only [sar_eq, shl_eq]
    rw [sign_bits _ _ (by omega) i hi, hs]
    congr 1
    omega
  · rename_i hn
    have hn' : 31 ≤ n.toNat := by
      simp only [BitVec.lt_def, BitVec.toNat_ofNat] at hn
      omega
    rw [Nat.min_eq_left (by omega)]

@[simp] theorem signextend_signextend (a b x : Word) :
    signextend a (signextend b x) = signextend (if a < b then a else b) x := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  rw [signextend_bits a _ i hi, signextend_bits b _ _ (by omega), signextend_bits _ _ i hi]
  by_cases h : a < b
  · have h' : a.toNat < b.toNat := h
    simp only [ite_eq_left h]
    rw [Nat.min_assoc, Nat.min_eq_left (show a.toNat * 8 + 7 ≤ b.toNat * 8 + 7 by omega)]
  · have h' : b.toNat ≤ a.toNat := by
      simp only [BitVec.lt_def] at h
      omega
    simp only [ite_eq_right h]
    rw [Nat.min_assoc, Nat.min_eq_right (show b.toNat * 8 + 7 ≤ a.toNat * 8 + 7 by omega)]


def signedMask (n m x : Word) :=
  sar (256#256 - (if n < 256#256 then n else 256#256))
      (shl (256#256 - (if n < 256#256 then n else 256#256)) x) &&& (shl m 1#256 - 1#256)

theorem mask_bits (n : Word) (i : Nat) (hi : i < 256) :
    (shl n 1#256 - 1#256).getLsbD i = decide (i < n.toNat) := by
  rw [shl_eq, ← BitVec.not_neg, ← BitVec.shiftLeft_neg]
  change (~~~(BitVec.allOnes 256 <<< n.toNat)).getLsbD i = _
  simp only [BitVec.getLsbD_not, BitVec.getLsbD_shiftLeft, BitVec.getLsbD_allOnes]
  simp [hi, show i - n.toNat < 256 by omega]

theorem signedMask_recover (n m x : Word) (hn : 1#256 ≤ n) (hn' : n ≤ 256#256)
    (hm : n < m) (hx : x &&& (shl n 1#256 - 1#256) = x) :
    signedMask n m x &&& (shl n 1#256 - 1#256) = x := by
  have hn0 : 1 ≤ n.toNat := hn
  have hn256 : n.toNat ≤ 256 := hn'
  have hm' : n.toNat < m.toNat := hm
  have hs : (256#256 - (if n < 256#256 then n else 256#256)).toNat = 256 - n.toNat := by
    split <;> bv_omega
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have hx' := congrArg (fun v : Word => v.getLsbD i) hx
  rw [BitVec.getLsbD_and, mask_bits n i hi] at hx'
  unfold signedMask
  rw [BitVec.getLsbD_and, BitVec.getLsbD_and, mask_bits n i hi, mask_bits m i hi]
  rw [sar_eq, shl_eq, sign_bits _ _ (by omega) i hi, hs]
  by_cases h : i < n.toNat
  · simp only [Nat.min_eq_left (show i ≤ 255 - (256 - n.toNat) by omega)]
    simp [h, show i < m.toNat by omega]
  · simp only [h, decide_false, Bool.and_false] at hx' ⊢
    exact hx'

@[simp] theorem signed_mask_eq (n m x y : Word) (hn : 1#256 ≤ n) (hn' : n ≤ 256#256)
    (hm : n < m) (hx : x &&& (shl n 1#256 - 1#256) = x)
    (hy : y &&& (shl n 1#256 - 1#256) = y) :
    ((sar (256#256 - (if n < 256#256 then n else 256#256)) (shl (256#256 - (if n < 256#256 then n else 256#256)) x) &&& (shl m 1#256 - 1#256)) = (sar (256#256 - (if n < 256#256 then n else 256#256)) (shl (256#256 - (if n < 256#256 then n else 256#256)) y) &&& (shl m 1#256 - 1#256))) ↔ x = y := by
  constructor
  · intro h
    have h' := congrArg (fun v => v &&& (shl n 1#256 - 1#256)) h
    change signedMask n m x &&& _ = signedMask n m y &&& _ at h'
    simpa only [signedMask_recover n m x hn hn' hm hx,
      signedMask_recover n m y hn hn' hm hy] using h'
  · intro h
    subst y
    rfl

@[simp] theorem sar_and (n x y : Word) :
    sar n x &&& sar n y = sar n (x &&& y) := by
  simp only [sar_eq, BitVec.sshiftRight_and_distrib]

@[simp] theorem sar_or (n x y : Word) :
    sar n x ||| sar n y = sar n (x ||| y) := by
  simp only [sar_eq, BitVec.sshiftRight_or_distrib]

@[simp] theorem sar_xor (n x y : Word) :
    sar n x ^^^ sar n y = sar n (x ^^^ y) := by
  simp only [sar_eq, BitVec.sshiftRight_xor_distrib]

@[simp] theorem shl_and (n x y : Word) :
    shl n x &&& shl n y = shl n (x &&& y) := by
  simp only [shl_eq, BitVec.shiftLeft_and_distrib]

@[simp] theorem shl_or (n x y : Word) :
    shl n x ||| shl n y = shl n (x ||| y) := by
  simp only [shl_eq, BitVec.shiftLeft_or_distrib]

@[simp] theorem shl_xor (n x y : Word) :
    shl n x ^^^ shl n y = shl n (x ^^^ y) := by
  simp only [shl_eq, BitVec.shiftLeft_xor_distrib]

@[simp] theorem shl_add (n x y : Word) :
    shl n x + shl n y = shl n (x + y) := by
  simp only [shl_eq, BitVec.shiftLeft_add_distrib]

@[simp] theorem shr_and (n x y : Word) :
    shr n x &&& shr n y = shr n (x &&& y) := by
  simp only [shr_eq, BitVec.ushiftRight_and_distrib]

@[simp] theorem shr_or (n x y : Word) :
    shr n x ||| shr n y = shr n (x ||| y) := by
  simp only [shr_eq, BitVec.ushiftRight_or_distrib]

@[simp] theorem shr_xor (n x y : Word) :
    shr n x ^^^ shr n y = shr n (x ^^^ y) := by
  simp only [shr_eq, BitVec.ushiftRight_xor_distrib]

end Evm
