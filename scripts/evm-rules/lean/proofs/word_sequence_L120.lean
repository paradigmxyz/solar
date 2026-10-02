-- An arithmetic right shift distributes over OR.
simp only [Evm.or, Evm.sar_eq, BitVec.sshiftRight', BitVec.sshiftRight_or_distrib]
