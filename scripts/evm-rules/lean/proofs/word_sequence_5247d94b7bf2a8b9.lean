-- x / x => x != 0.
unfold Evm.div Evm.ne
by_cases h : x = 0
· subst h
  rfl
· rw [beq_eq_false_iff_ne.mpr h, Bool.cond_false]
  apply BitVec.eq_of_toNat_eq
  have hx : x.toNat ≠ 0 := fun e => h (BitVec.eq_of_toNat_eq e)
  rw [BitVec.toNat_udiv, Nat.div_self (Nat.pos_of_ne_zero hx)]
  rfl
