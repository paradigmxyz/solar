-- a % 2 ** k => a & (2 ** k - 1) for 1 <= k <= 255: on values, `a % 2 ^ k` is the mask
-- `a &&& (2 ^ k - 1)` (`Nat.and_two_pow_sub_one_eq_mod`).
subst h₃
have hne : (Evm.shl «@fresh:1» 1 == 0) = false := by
  rw [beq_eq_false_iff_ne]
  exact shl_one_ne_zero _ h₂
have hk : «@fresh:1».toNat < 256 := by simpa [BitVec.lt_def] using h₂
have hlt : 2 ^ «@fresh:1».toNat < 2 ^ 256 := Nat.pow_lt_pow_right (by decide) hk
simp only [Evm.mod, hne, Bool.cond_false]
simp only [Evm.and, Evm.sub, Evm.shl_eq, BitVec.shiftLeft_eq']
change _ % BitVec.twoPow 256 _ = _ &&& (BitVec.twoPow 256 _ - 1)
apply BitVec.eq_of_toNat_eq
rw [BitVec.toNat_umod, BitVec.toNat_and, BitVec.toNat_sub, BitVec.toNat_twoPow,
  Nat.mod_eq_of_lt hlt]
have hmask : (2 ^ 256 - (1 : Word).toNat + 2 ^ «@fresh:1».toNat) % 2 ^ 256 =
    2 ^ «@fresh:1».toNat - 1 := by
  have hpos := Nat.two_pow_pos «@fresh:1».toNat
  have hone : (1 : Word).toNat = 1 := rfl
  rw [hone]
  generalize 2 ^ «@fresh:1».toNat = p at *
  omega
rw [hmask, Nat.and_two_pow_sub_one_eq_mod]
