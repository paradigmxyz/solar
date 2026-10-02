-- (value >> inner) >> outer => value >> min(inner + outer, 256): `BitVec.shiftRight_add`
-- composes the shifts, and every count of at least the width shifts out every bit.
simp
rw [← BitVec.shiftRight_add, shift_sum_toNat]
by_cases hs : solar_query_0.toNat + solar_query_1.toNat < 256
· rw [Nat.min_eq_left (by omega)]
· rw [Nat.min_eq_right (by omega), BitVec.ushiftRight_eq_zero (by omega),
    BitVec.ushiftRight_eq_zero (by omega)]
