-- (x << shift) < (y << shift) => x < y.
rw [lt_bits, lt_bits, toNat_shl_of_shr_shl (shr_shl_of_clear h₂),
  toNat_shl_of_shr_shl (shr_shl_of_clear h₃)]
simp only [Nat.mul_lt_mul_right (Nat.two_pow_pos _)]
