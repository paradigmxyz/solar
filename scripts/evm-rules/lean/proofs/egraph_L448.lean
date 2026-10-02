-- 0 % _ => 0: the dividend is zero, and EVM's zero divisor yields zero too.
by_cases h : (0#256) = solar_query_1
· subst h
  simp [BitVec.zero_umod]
· simp [h]
