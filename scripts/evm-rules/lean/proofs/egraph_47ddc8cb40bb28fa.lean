-- x == 0 | (x * c) / x == c => x < MAX / c + 1, when c >= 2: zero never wraps, and a
-- nonzero x divides the wrapped product back to c exactly when it did not wrap.
subst h₁
have h2 : 2 ≤ c.toNat := h₂
have hc : 0 < c.toNat := by omega
have hone : (1 : Word).toNat = 1 := rfl
have hzero : (0 : Word).toNat = 0 := rfl
have hq : (Evm.div MAX c).toNat + (1 : Word).toNat < 2 ^ 256 := by
  have := Nat.div_lt_self (n := 2 ^ 256 - 1) (by decide) (by omega : 1 < c.toNat)
  rw [toNat_div, toNat_max, hone]
  omega
rw [eq_bits, eq_bits, or_bits, lt_bits, toNat_add_of_fits hq, toNat_div, toNat_div, toNat_mul,
  toNat_max, hone, hzero, ← Bool.decide_or]
apply bits_congr
rw [lt_max_div_succ_iff hc]
by_cases hx : x.toNat = 0
· simp [hx]
· rw [Nat.mul_comm x.toNat c.toNat, wrapped_div_eq_iff (by omega)]
  simp [hx]
