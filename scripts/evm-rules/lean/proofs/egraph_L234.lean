-- value * 2 ** k => value << k: the guard makes the constant `1 << k`.
have key : solar_query_4 * ((1#256) <<< solar_query_0.toNat) =
    solar_query_4 <<< solar_query_0.toNat := by
  rw [BitVec.shiftLeft_eq_mul_twoPow solar_query_4]
  rfl
simp
intros
subst_vars
exact key
