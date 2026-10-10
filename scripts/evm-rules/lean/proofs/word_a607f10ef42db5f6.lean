-- (x << shift) > c => x > (c >> shift).
have hc : Evm.shr shift (Evm.shl shift (Evm.shr shift c)) = Evm.shr shift c := by rw [h₂]
have hct := toNat_shl_of_shr_shl hc
rw [h₂] at hct
rw [gt_bits, gt_bits, toNat_shl_of_shr_shl (shr_shl_of_clear h₃), hct]
simp only [Nat.mul_lt_mul_right (Nat.two_pow_pos _)]
