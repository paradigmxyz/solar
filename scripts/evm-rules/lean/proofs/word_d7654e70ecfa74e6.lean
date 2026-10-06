-- sdiv(x, 2^k) => sar(k, x), for a multiple x of 2^k: the quotient is exact.
subst h₄
have hc := toNat_shl_one h₃
have hk : («@fresh:2»).toNat < 255 := h₅
have hks : («@fresh:2»).toNat ≤ s.toNat := h₆
have hp : 2 ^ («@fresh:2»).toNat < 2 ^ 255 := Nat.pow_lt_pow_right (by decide) hk
have hcm : (Evm.shl «@fresh:2» 1).msb = false := by
  rw [BitVec.msb_eq_false_iff_two_mul_lt, hc]
  omega
have hci : (Evm.shl «@fresh:2» 1).toInt = 2 ^ («@fresh:2»).toNat := by
  rw [BitVec.toInt_eq_toNat_of_msb hcm, hc]
  simp
have hn : 2 ^ («@fresh:2»).toNat ∣ x.toNat := by
  subst h₁
  rw [toNat_shl, Nat.dvd_mod_iff (Nat.pow_dvd_pow 2 (by omega))]
  exact Nat.dvd_mul_left_of_dvd (Nat.pow_dvd_pow 2 hks) _
have hz : (2 : Int) ^ («@fresh:2»).toNat ∣ x.toInt := by
  have := (pow_dvd_toInt_iff x (k := («@fresh:2»).toNat) (by omega)).mpr
    (Nat.mod_eq_zero_of_dvd hn)
  simpa using this
rw [← BitVec.toInt_inj, Evm.sdiv, Evm.sar_eq, BitVec.toInt_sdiv, BitVec.toInt_sshiftRight', hci,
  Int.tdiv_eq_ediv_of_dvd hz, Int.shiftRight_eq_div_pow]
have hl := BitVec.le_toInt (x.sshiftRight' «@fresh:2»)
have hu := BitVec.toInt_lt (x := x.sshiftRight' «@fresh:2»)
rw [BitVec.toInt_sshiftRight', Int.shiftRight_eq_div_pow] at hl hu
push_cast at hl hu ⊢
apply Int.bmod_eq_of_le <;> omega
