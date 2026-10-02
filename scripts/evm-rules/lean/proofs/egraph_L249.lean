-- a % 2 ** k => a & (2 ** k - 1), when 1 <= k <= 255: on values, `a % 2 ^ k` is the
-- mask `a &&& (2 ^ k - 1)` (`Nat.and_two_pow_sub_one_eq_mod`).
simp
intros
subst_vars
rename_i hlow hhigh
have hk : solar_query_0.toNat < 256 := by simpa [BitVec.ult] using hhigh
have hlt : 2 ^ solar_query_0.toNat < 2 ^ 256 := Nat.pow_lt_pow_right (by decide) hk
have hpos := Nat.two_pow_pos solar_query_0.toNat
have hpow : (1#256) <<< solar_query_0.toNat = BitVec.twoPow 256 solar_query_0.toNat := rfl
have hne : BitVec.twoPow 256 solar_query_0.toNat ≠ 0#256 := by
  intro hz
  have h := congrArg BitVec.toNat hz
  rw [BitVec.toNat_twoPow, Nat.mod_eq_of_lt hlt] at h
  exact Nat.pos_iff_ne_zero.mp hpos h
simp only [hpow, hne, ite_false]
apply BitVec.eq_of_toNat_eq
rw [BitVec.toNat_umod, BitVec.toNat_and, BitVec.toNat_sub, BitVec.toNat_twoPow,
  Nat.mod_eq_of_lt hlt]
have hmask : (2 ^ 256 - (1#256).toNat + 2 ^ solar_query_0.toNat) % 2 ^ 256 =
    2 ^ solar_query_0.toNat - 1 := by
  simp only [BitVec.toNat_ofNat]
  generalize 2 ^ solar_query_0.toNat = p at *
  omega
rw [hmask, Nat.and_two_pow_sub_one_eq_mod]
