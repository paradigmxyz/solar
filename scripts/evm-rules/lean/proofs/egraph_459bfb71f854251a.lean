-- (x / a) / b => x / (a * b), when a * b <= MAX (`Nat.div_div_eq_div_mul`).
apply BitVec.eq_of_toNat_eq
rw [toNat_div, toNat_div, toNat_div, toNat_mul_of_fits (mul_fits h₁), Nat.div_div_eq_div_mul]
