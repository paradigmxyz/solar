-- x == (x * c) / c => x < MAX / c + 1, when c >= 2 (`wrapped_div_eq_iff`).
subst h₁
have h2 : 2 ≤ c.toNat := h₂
have hc : 0 < c.toNat := by omega
have hone : (1 : Word).toNat = 1 := rfl
have hq : (Evm.div MAX c).toNat + (1 : Word).toNat < 2 ^ 256 := by
  have := Nat.div_lt_self (n := 2 ^ 256 - 1) (by decide) (by omega : 1 < c.toNat)
  rw [toNat_div, toNat_max, hone]
  omega
rw [eq_bits, lt_bits, toNat_div, toNat_mul, toNat_add_of_fits hq, toNat_div, toNat_max, hone]
apply bits_congr
rw [eq_comm, wrapped_div_eq_iff hc, lt_max_div_succ_iff hc]
