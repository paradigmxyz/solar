-- 0 / _ => 0: `BitVec.zero_udiv` for a nonzero divisor.
by_cases h : (0#256) = solar_query_1
· subst h
  by_cases hq : solar_query_0 = 0#256 <;> simp [hq, BitVec.smtUDiv_eq, BitVec.zero_udiv]
· simp [h]
