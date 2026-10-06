-- mulmod(x, y, 2^k) => (x * y) & (2^k - 1): the exact product and its wrapped value agree in the
-- low k bits.
have hmask := toNat_mask_of_pow h₂ h₃
have hk : («@fresh:1»).toNat < 256 := h₂
subst h₃
apply BitVec.eq_of_toNat_eq
rw [toNat_mulmod, toNat_shl_one h₂, ite_eq_right (Nat.pos_iff_ne_zero.mp (Nat.two_pow_pos _)),
  Evm.and, BitVec.toNat_and, hmask, Nat.and_two_pow_sub_one_eq_mod, toNat_mul,
  Nat.mod_mod_of_dvd _ (Nat.pow_dvd_pow 2 (by omega))]
