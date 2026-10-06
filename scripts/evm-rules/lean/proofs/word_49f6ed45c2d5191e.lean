-- (x << shift) == c => x == (c >> shift).
have hx := shr_shl_of_clear h₃
apply eq_congr_iff
constructor
· intro e
  rw [← e, hx]
· intro e
  rw [e, h₂]
