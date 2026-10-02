-- x * (1 << y) => x << y: `1 << n` is `twoPow`, and `x << n = x * twoPow n` for every n,
-- saturating counts included.
have key : solar_query_1 * ((1#256) <<< solar_query_2.toNat) =
    solar_query_1 <<< solar_query_2.toNat := by
  rw [BitVec.shiftLeft_eq_mul_twoPow solar_query_1]
  rfl
simp [key]
