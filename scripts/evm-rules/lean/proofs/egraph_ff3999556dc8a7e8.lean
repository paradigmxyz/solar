-- Rounding x down to a multiple of c never exceeds x, so the product never wraps.
subst h₁
subst h₂
have hround := round_le x c
first | rw [lt_bits] | rw [gt_bits]
simp [Nat.not_lt.mpr hround]
