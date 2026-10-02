-- x / c < d => x < c * d, when c != 0 and c * d <= MAX (`Nat.div_lt_iff_lt_mul`).
rw [lt_bits, lt_bits, toNat_div, toNat_mul_of_fits (mul_fits h₂)]
apply bits_congr
rw [Nat.div_lt_iff_lt_mul (toNat_pos h₁), Nat.mul_comm d.toNat]
