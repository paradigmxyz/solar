-- a / d => 0, when a < d: the quotient's value is `a.toNat / d.toNat = 0`.
cases solar_query_1
· simp
· by_cases hlt : BitVec.ult solar_query_0 solar_query_2 = true
  · have hlt' : solar_query_0.toNat < solar_query_2.toNat := by
      simpa [BitVec.ult] using hlt
    have hm : solar_query_2 ≠ 0#256 := by
      intro hz
      simp [hz] at hlt'
    have hq : solar_query_0 / solar_query_2 = 0#256 := by
      apply BitVec.eq_of_toNat_eq
      simp [BitVec.toNat_udiv, Nat.div_eq_of_lt hlt']
    simp [hm, BitVec.smtUDiv_eq, hq]
  · simp [hlt]
