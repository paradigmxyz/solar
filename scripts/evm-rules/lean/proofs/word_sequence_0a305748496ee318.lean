-- smod(x, 2^k) == 0 => (x & (2^k - 1)) == 0.
have hk : («@fresh:1»).toNat < 255 := by
  have : ¬255 ≤ («@fresh:1»).toNat := h₄
  omega
exact eq_congr_iff (smod_pow_eq_zero_iff x m «@fresh:1» h₂ h₃ hk)
