-- 2 ** exponent => 1 << exponent: both are `twoPow exponent`, zero from 256 on.
subst h₁
simp only [Evm.exp, Evm.shl_eq, BitVec.shiftLeft_eq', two_pow]
rfl
