-- sdiv(x, 2^k) => x >> k, for a nonnegative x.
subst h₃
have hc := toNat_shl_one h₂
have hk : («@fresh:1»).toNat < 255 := h₄
have hx : x.toNat < 2 ^ 255 := by
  have : x.toNat < (Evm.shl 255 1).toNat := h₅
  rwa [toNat_shl_one (by decide)] at this
have hxm : x.msb = false := BitVec.msb_eq_false_iff_two_mul_lt.mpr (by omega)
have hcm : (Evm.shl «@fresh:1» 1).msb = false := by
  rw [BitVec.msb_eq_false_iff_two_mul_lt, hc]
  have := Nat.pow_lt_pow_right (a := 2) (by decide) hk
  omega
rw [Evm.sdiv, BitVec.sdiv_eq, hxm, hcm]
apply BitVec.eq_of_toNat_eq
rw [BitVec.udiv_eq, BitVec.toNat_udiv, hc, toNat_shr]
