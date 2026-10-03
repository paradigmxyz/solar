-- a / 2 ** k => a >> k for 1 <= k <= 255: dividing by `twoPow k` below the word width shifts
-- (`BitVec.udiv_twoPow_eq_of_lt`).
subst h₃
have hk : «@fresh:1».toNat < 256 := by simpa [BitVec.lt_def] using h₂
simp only [Evm.div, Evm.shl_eq, Evm.shr_eq, BitVec.shiftLeft_eq', BitVec.ushiftRight_eq']
exact BitVec.udiv_twoPow_eq_of_lt hk
