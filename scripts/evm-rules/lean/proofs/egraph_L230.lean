-- (1 << y) * x => x << y: as `x * (1 << y)`, after commuting the product.
have key : ((1#256) <<< solar_query_2.toNat) * solar_query_1 =
    solar_query_1 <<< solar_query_2.toNat := by
  rw [BitVec.mul_comm, BitVec.shiftLeft_eq_mul_twoPow solar_query_1]
  rfl
simp [key]
