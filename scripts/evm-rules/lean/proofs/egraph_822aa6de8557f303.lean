-- x / c > d => 0, when c != 0 and c * d + (c - 1) > MAX: x / c <= d.
subst h₄
have hover := add_overflows h₃
rw [toNat_mul_of_fits (mul_fits h₂), toNat_sub_one h₁] at hover
have hx := «@fresh:1».isLt
have hge : ¬d.toNat < «@fresh:1».toNat / c.toNat := by
  rw [lt_div_iff (toNat_pos h₁)]
  omega
rw [gt_bits, toNat_div]
simp [hge]
