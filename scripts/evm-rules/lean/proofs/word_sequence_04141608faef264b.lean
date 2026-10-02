-- An arithmetic right shift distributes over XOR.
simp only [Evm.xor, Evm.sar_eq, BitVec.sshiftRight', BitVec.sshiftRight_xor_distrib]
