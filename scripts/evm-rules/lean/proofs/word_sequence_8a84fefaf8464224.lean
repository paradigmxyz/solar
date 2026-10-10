-- 0 ** n => n == 0.
unfold Evm.exp Evm.eq
by_cases h : n = 0
· subst h
  rfl
· obtain ⟨k, hk⟩ : ∃ k, n.toNat = k + 1 :=
    ⟨n.toNat - 1, by have := fun e => h (BitVec.eq_of_toNat_eq e); simp at this; omega⟩
  rw [hk, BitVec.pow_succ, beq_eq_false_iff_ne.mpr h, Bool.cond_false]
  exact BitVec.mul_zero
