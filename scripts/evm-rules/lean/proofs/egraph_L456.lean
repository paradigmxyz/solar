-- clz(x) % b => clz(x) for constant b > 256: the count is at most 256, below the divisor. The
-- bound needs only the count and a comparison, so it bit-blasts without a divider.
subst h₁
have hlt : Evm.clz «@fresh:1» < b := by
  simp only [Evm.clz]
  bv_decide
have hb : (b == 0) = false := by
  rw [beq_eq_false_iff_ne]
  rintro rfl
  simp [BitVec.lt_def] at hlt
simp only [Evm.mod, hb, Bool.cond_false]
exact BitVec.umod_eq_of_lt hlt
