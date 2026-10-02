-- (value << inner) << outer => value << min(inner + outer, 256): `BitVec.shiftLeft_add`
-- composes the shifts, and every count of at least the width yields zero.
simp only [Evm.shl_eq, BitVec.shiftLeft_eq', shift_sum_toNat, ← BitVec.shiftLeft_add]
by_cases hs : outer.toNat + inner.toNat < 256
· rw [Nat.min_eq_left (by omega), Nat.add_comm]
· rw [Nat.min_eq_right (by omega), BitVec.shiftLeft_eq_zero (by omega),
    BitVec.shiftLeft_eq_zero (by omega)]
