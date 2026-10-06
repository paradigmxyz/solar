-- sdiv(x, MIN) => x == MIN.
subst h₁
rw [shl_255_one, Evm.sdiv, BitVec.sdiv_intMin, Evm.eq]
by_cases h : x = BitVec.intMin 256
· subst h
  rfl
· rw [ite_eq_right h, beq_eq_false_iff_ne.mpr h]
  rfl
