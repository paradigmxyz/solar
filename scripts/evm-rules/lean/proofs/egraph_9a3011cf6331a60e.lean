-- Rounding x down to a multiple of y never exceeds x, so the product never wraps.
subst h₁
have hround := round_le x y
first | rw [lt_bits] | rw [gt_bits]
simp [Nat.not_lt.mpr hround]
