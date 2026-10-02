-- clz(x) % b => clz(x) for constant b > 256: the count is at most 256, below the divisor. With
-- the remainder split on that comparison, `bv_decide` checks the bound without a divider.
simp only [umod_split]
bv_decide
