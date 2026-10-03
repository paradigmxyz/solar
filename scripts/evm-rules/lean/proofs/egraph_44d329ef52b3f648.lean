-- x * (1 << y) => x << y: a left shift multiplies by a power of two.
simp only [Evm.mul, Evm.shl_eq, BitVec.shiftLeft_eq']
rw [BitVec.shiftLeft_eq_mul_twoPow x]
rfl
