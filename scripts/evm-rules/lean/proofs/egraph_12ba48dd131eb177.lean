-- (1 << y) * x => x << y.
simp only [Evm.mul, Evm.shl_eq, BitVec.shiftLeft_eq']
rw [BitVec.mul_comm, BitVec.shiftLeft_eq_mul_twoPow x]
rfl
