-- (x / c) * c + x % c => x, when c != 0 (`Nat.div_add_mod'`); neither step wraps.
subst h₂
subst h₃
have hsum : (Evm.mul (Evm.div x c) c).toNat + (Evm.mod x c).toNat < 2 ^ 256 := by
  rw [toNat_round, toNat_mod h₁, Nat.div_add_mod']
  exact x.isLt
apply BitVec.eq_of_toNat_eq
rw [toNat_add_of_fits hsum, toNat_round, toNat_mod h₁, Nat.div_add_mod']
