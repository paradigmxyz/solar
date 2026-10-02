-- smod(0, _) => 0: a zero dividend has a clear sign bit, so `srem` is an unsigned
-- remainder of zero.
by_cases h : (0#256) = solar_query_1
· subst h
  cases hmsb : solar_query_0.msb <;> simp [BitVec.srem, BitVec.zero_umod, hmsb]
· simp [h]
