-- a % m => a, when a < m: `BitVec.umod_eq_of_lt`; then m is nonzero.
cases solar_query_1
· simp
· by_cases hlt : BitVec.ult solar_query_0 solar_query_2 = true
  · have hlt' : solar_query_0 < solar_query_2 := by simpa [BitVec.ult, BitVec.lt_def] using hlt
    have hm : solar_query_2 ≠ 0#256 := by
      intro hz
      simp [hz, BitVec.lt_def] at hlt'
    simp [hm, BitVec.umod_eq_of_lt hlt']
  · simp [hlt]
