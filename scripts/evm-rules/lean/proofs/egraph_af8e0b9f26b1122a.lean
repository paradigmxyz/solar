-- (x / a) / b => 0, when a * b > MAX: the dividend is below the product of the divisors.
have hover := mul_overflows h₁
have hx := «@fresh:1».isLt
have hzero : (0 : Word).toNat = 0 := rfl
apply BitVec.eq_of_toNat_eq
rw [toNat_div, toNat_div, Nat.div_div_eq_div_mul, hzero, Nat.div_eq_of_lt (by omega)]
