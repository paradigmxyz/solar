-- x / c == 0 => x < c, when c != 0: a quotient is zero exactly below its divisor.
have hzero : (0 : Word).toNat = 0 := rfl
rw [eq_bits, lt_bits, toNat_div, hzero]
exact bits_congr (Nat.div_eq_zero_iff_lt (toNat_pos h₁))
