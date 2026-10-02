-- balance(address & mask) => balance(address) when the mask keeps the low 160 bits: both
-- read the balance of the same account.
subst h₁
have key : (Evm.and address mask).setWidth 160 = address.setWidth 160 := by
  simp only [Evm.and, Evm.shl_eq, Evm.not] at h₂ ⊢
  bv_decide
simp only [Evm.balance, key]
