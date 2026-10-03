-- smod(a, a) => 0: a signed remainder by itself is zero, as is any remainder by zero.
simp [Evm.smod, BitVec.srem_self]
