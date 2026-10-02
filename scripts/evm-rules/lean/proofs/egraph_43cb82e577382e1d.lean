-- x / c > d => x > c * d + (c - 1), when c != 0 and the bound is a word (`lt_div_iff`).
have hfit := mul_fits h₂
have hbound : (Evm.mul c d).toNat + (Evm.sub c 1).toNat < 2 ^ 256 := add_fits h₃
rw [gt_bits, gt_bits, toNat_div, toNat_add_of_fits hbound, toNat_mul_of_fits hfit,
  toNat_sub_one h₁]
exact bits_congr (lt_div_iff (toNat_pos h₁))
