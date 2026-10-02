-- balance(address & mask) => balance(address), when the mask keeps the low 160 bits:
-- both read the balance at the same 160-bit address.
have key : solar_query_3 = solar_query_1 &&& solar_query_2 →
    (~~~solar_query_2) <<< 96 = 0#256 →
    BitVec.extractLsb' 0 160 solar_query_3 = BitVec.extractLsb' 0 160 solar_query_1 := by
  intro h1 h2
  subst h1
  bv_decide
simp
intro h1 _ _ h2
rw [key h1 h2]
