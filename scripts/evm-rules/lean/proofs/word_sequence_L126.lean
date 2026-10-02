-- A left shift distributes over wrapping addition.
simp only [Evm.add, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.shiftLeft_add_distrib]
