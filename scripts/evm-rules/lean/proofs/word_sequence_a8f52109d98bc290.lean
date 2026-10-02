-- An arithmetic right shift distributes over AND.
simp only [Evm.and, Evm.sar_eq, BitVec.sshiftRight', BitVec.sshiftRight_and_distrib]
