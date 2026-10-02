-- clz(_) / b => 0 for constant b > 256: the count is at most 256, below the divisor. With the
-- quotient split on that comparison, `bv_decide` checks the count's bound without a divider.
simp only [smtUDiv_split]
bv_decide
