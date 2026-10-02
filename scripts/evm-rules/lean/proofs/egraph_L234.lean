-- value * 2 ** k => value << k: the constant is the power `1 << k`.
subst h₅
simp only [Evm.mul, Evm.shl_eq, BitVec.shiftLeft_eq']
rw [BitVec.shiftLeft_eq_mul_twoPow value]
rfl
