-- x / c < d => 1, when c != 0 and c * d > MAX: x / c <= MAX / c < d.
subst h₃
have hover := mul_overflows h₂
have hx := «@fresh:1».isLt
have hlt : «@fresh:1».toNat / c.toNat < d.toNat := by
  rw [Nat.div_lt_iff_lt_mul (toNat_pos h₁), Nat.mul_comm]
  omega
rw [lt_bits, toNat_div]
simp [hlt]
