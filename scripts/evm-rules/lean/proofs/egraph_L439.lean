-- sdiv(0, _) => 0: a zero dividend makes every signed case an unsigned quotient of zero.
by_cases h : (0#256) = solar_query_1
· subst h
  by_cases hq : solar_query_0 = 0#256
  · simp [hq]
  · have hn : -solar_query_0 ≠ 0#256 := by
      intro hc
      exact hq (by simpa using congrArg (fun x => -x) hc)
    simp [hq, hn, BitVec.smtSDiv, BitVec.smtUDiv_eq, BitVec.zero_udiv]
    split <;> simp_all [BitVec.smtUDiv_eq, BitVec.zero_udiv]
· simp [h]
