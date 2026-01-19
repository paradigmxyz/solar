# Solar vs solc Gas Comparison

Generated: 2026-01-15 18:05:05 UTC

| Suite | solc Pass | Solar Pass | solc Gas (avg) | Solar Gas (avg) | Δ% |
|-------|-----------|------------|----------------|-----------------|-----|
| arithmetic | 38 | 38 | 16661 | 13477 | -19.1% |
| calls | 14 | 11 | 9538 | 6460 | -32.2% |
| constructor-args | 4 | 0 | 7847 | 0 | N/A% |
| control-flow | 43 | 26 | 12040 | 6438 | -46.5% |
| events | 2 | 2 | 7742 | 4961 | -35.9% |
| inheritance | 5 | 0 | 38468 | 0 | N/A% |
| interfaces | 4 | 0 | 34099 | 0 | N/A% |
| libraries | 9 | 0 | 7044 | 0 | N/A% |
| multi-return | 12 | 12 | 5182 | 3768 | -27.2% |
| stack-deep | 0 | 4 | 0 | 2563 | N/A% |
| storage | 37 | 37 | 43089 | 40122 | -6.8% |

## Summary

- **Total solc tests passed:** 168
- **Total Solar tests passed:** 130
- **Average gas (passing suites):** solc=15708, Solar=12537 (Δ=-20.1%)

**Legend:** Δ% = (Solar - solc) / solc × 100. Negative = Solar uses less gas.
