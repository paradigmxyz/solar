-- x / c > d => 0, when c != 0 and c * d > MAX: x / c <= MAX / c <= d.
subst h₃
have hover := mul_overflows h₂
have hx := «@fresh:1».isLt
have hge : ¬d.toNat < «@fresh:1».toNat / c.toNat := by
  rw [lt_div_iff (toNat_pos h₁)]
  omega
rw [gt_bits, toNat_div]
simp [hge]
