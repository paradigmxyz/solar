-- x % c + (x / c) * c => x, when c != 0 (`Nat.mod_add_div'`); neither step wraps.
subst h₂
subst h₃
have hsum : (Evm.mod x c).toNat + (Evm.mul (Evm.div x c) c).toNat < 2 ^ 256 := by
  rw [toNat_round, toNat_mod h₁, Nat.mod_add_div']
  exact x.isLt
apply BitVec.eq_of_toNat_eq
rw [toNat_add_of_fits hsum, toNat_round, toNat_mod h₁, Nat.mod_add_div']
