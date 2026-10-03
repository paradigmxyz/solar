-- (signed(value) >> inner) >> outer => signed(value) >> min(inner + outer, 256):
-- `BitVec.sshiftRight_add` composes the shifts, and every count of at least the width fills
-- every bit with the sign (`sshiftRight_saturate`).
simp only [Evm.sar_eq, BitVec.sshiftRight', shift_sum_toNat, ← BitVec.sshiftRight_add]
by_cases hs : outer.toNat + inner.toNat < 256
· rw [Nat.min_eq_left (by omega), Nat.add_comm]
· rw [Nat.min_eq_right (by omega), sshiftRight_saturate _ (by omega),
    sshiftRight_saturate _ (by omega)]
