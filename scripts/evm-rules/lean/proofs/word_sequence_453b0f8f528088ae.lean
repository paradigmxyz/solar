-- MAX ** n => 1 - ((n & 1) + (n & 1)): -1 to an even power is one, to an odd power -1.
unfold Evm.exp Evm.sub Evm.add Evm.and
rw [max_pow]
have hb : n &&& 1 = if n.toNat % 2 = 0 then 0 else 1 := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_and, show (1 : Word).toNat = 1 from rfl, Nat.and_one_is_mod]
  split <;> simp_all <;> omega
rw [hb]
split <;> decide
