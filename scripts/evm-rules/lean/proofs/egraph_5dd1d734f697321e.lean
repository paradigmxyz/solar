-- The rounded product never wraps, so dividing it by a nonzero y gives back x / y
-- (`Nat.mul_div_cancel`); a zero y makes both sides zero.
subst h₁
subst h₂
rw [eq_bits, toNat_div (Evm.mul _ _), toNat_round, toNat_div]
by_cases hy : y.toNat = 0
· simp [hy]
· rw [Nat.mul_div_cancel _ (by omega)]
  simp
