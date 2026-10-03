-- clz(_) / b => 0 for constant b > 256: the count is at most 256, below the divisor. The
-- bound needs only the count and a comparison, so it bit-blasts without a divider.
have hlt : Evm.clz «@fresh:1» < b := by
  simp only [Evm.clz]
  bv_decide
simp [Evm.div, BitVec.udiv_eq_zero_iff_eq_zero_or_lt, hlt]
