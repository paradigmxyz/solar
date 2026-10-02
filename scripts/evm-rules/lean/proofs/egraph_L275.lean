-- (value >> inner) >> outer => value >> min(inner + outer, 256): `BitVec.shiftRight_add`
-- composes the shifts, and every count of at least the width yields zero.
simp only [Evm.shr_eq, BitVec.ushiftRight_eq', shift_sum_toNat, ← BitVec.shiftRight_add]
by_cases hs : outer.toNat + inner.toNat < 256
· rw [Nat.min_eq_left (by omega), Nat.add_comm]
· rw [Nat.min_eq_right (by omega), BitVec.ushiftRight_eq_zero (by omega),
    BitVec.ushiftRight_eq_zero (by omega)]
