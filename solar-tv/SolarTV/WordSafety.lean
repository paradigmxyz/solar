import SolarTV.Word

/-! Correspondence of logarithmic exponentiation with the ordinary bit-vector
power. This establishes all exponents, including the full 256-bit range, without
unfolding an exponent-sized number of multiplication steps in the checker. -/

namespace SolarTV.Word

private theorem square_power (base : BitVec width) (n : Nat) :
    (base * base) ^ n = base ^ (n + n) := by
  induction n with
  | zero => simp
  | succ n ih =>
    rw [BitVec.pow_succ, ih]
    have h : n + 1 + (n + 1) = (n + n + 1) + 1 := by omega
    rw [h, BitVec.pow_succ, BitVec.pow_succ]
    simp [BitVec.mul_assoc]

theorem power_eq_pow (base : BitVec width) (n : Nat) : power base n = base ^ n := by
  induction n using Nat.strongRecOn generalizing base with
  | ind n ih =>
    rw [power]
    split
    · rename_i h
      simp [h]
    · rename_i h
      rw [ih (n / 2) (Nat.div_lt_self (Nat.pos_of_ne_zero h) (by decide)), square_power]
      split
      · rename_i even
        change base ^ (n / 2 + n / 2) = base ^ n
        exact congrArg (fun exponent => base ^ exponent) (by omega)
      · rename_i odd
        change base * base ^ (n / 2 + n / 2) = base ^ n
        rw [BitVec.mul_comm, ← BitVec.pow_succ]
        exact congrArg (fun exponent => base ^ exponent) (by omega)

end SolarTV.Word
