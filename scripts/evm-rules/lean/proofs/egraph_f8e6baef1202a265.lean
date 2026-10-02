-- a % m => a when a < m, which also rules out a zero modulus.
have hm : (m == 0) = false := by
  rw [beq_eq_false_iff_ne]
  rintro rfl
  simp [BitVec.lt_def] at h₁
simp only [Evm.mod, hm, Bool.cond_false]
exact BitVec.umod_eq_of_lt h₁
