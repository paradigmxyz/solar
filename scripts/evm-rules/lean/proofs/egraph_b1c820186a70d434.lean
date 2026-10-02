-- x / c != 0 => x > c - 1, when c != 0: a quotient is nonzero exactly from its divisor on.
have hc := toNat_pos h₁
have hzero : (0 : Word).toNat = 0 := rfl
rw [ne_bits, gt_bits, toNat_div, toNat_sub_one h₁, hzero]
apply bits_congr
rw [Ne, Nat.div_eq_zero_iff_lt hc]
omega
