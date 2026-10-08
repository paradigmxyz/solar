-- 1 % x => x > 1.
rw [gt_bits]
apply BitVec.eq_of_toNat_eq
rw [toNat_mod_ite, show (1 : Word).toNat = 1 from rfl]
by_cases h : 1 < x.toNat
· simp [h, Nat.mod_eq_of_lt h]
  omega
· simp [h]
  omega
