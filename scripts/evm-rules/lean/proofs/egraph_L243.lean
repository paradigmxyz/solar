-- a / 2 ** k => a >> k, when 1 <= k <= 255: the divisor is `twoPow k`, which is nonzero
-- below the word width, and `BitVec.udiv_twoPow_eq_of_lt` divides by shifting.
simp
intros
subst_vars
rename_i hlow hhigh
have hk : solar_query_0.toNat < 256 := by simpa [BitVec.ult] using hhigh
have hpow : (1#256) <<< solar_query_0.toNat = BitVec.twoPow 256 solar_query_0.toNat := rfl
have hne : BitVec.twoPow 256 solar_query_0.toNat ≠ 0#256 := by
  intro hz
  have h := congrArg BitVec.toNat hz
  rw [BitVec.toNat_twoPow, Nat.mod_eq_of_lt (Nat.pow_lt_pow_right (by decide) hk)] at h
  exact Nat.pos_iff_ne_zero.mp (Nat.two_pow_pos _) h
simp only [hpow, hne, ite_false, BitVec.smtUDiv_eq, BitVec.udiv_twoPow_eq_of_lt hk]
