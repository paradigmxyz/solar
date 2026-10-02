-- (signed(value) >> inner) >> outer => signed(value) >> min(inner + outer, 256):
-- `BitVec.sshiftRight_add` composes the shifts, and every count of at least the width fills
-- every bit with the sign (`sshiftRight_saturate`).
simp only [BitVec.sshiftRight']
simp
rw [← BitVec.sshiftRight_add, shift_sum_toNat]
by_cases hs : solar_query_0.toNat + solar_query_1.toNat < 256
· rw [Nat.min_eq_left (by omega)]
· rw [Nat.min_eq_right (by omega), sshiftRight_saturate _ (by omega)]
