-- An arithmetic right shift distributes over XOR.
simp only [BitVec.sshiftRight', BitVec.sshiftRight_xor_distrib]
simp
