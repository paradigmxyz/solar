# evm2 on boxctl m4-metal-large (comb4 source, znver4), Solar fe4b93c2 vs solc 0.8.38

Primary run (dani-solar-rerun2):

```
## solar-gas (vs solc-legacy200, solc-ir200, best of both per case)
  interp cyc: solar 16.555M legacy 21.913M (-24.5%) via-IR 21.616M (-23.4%) best 20.815M (-20.5%)
  interp ins: solar 56.761M legacy 75.680M (-25.0%) via-IR 75.428M (-24.7%) best 72.217M (-21.4%)
  JIT cyc: solar 6.246M legacy 7.314M (-14.6%) via-IR 7.865M (-20.6%) best 6.932M (-9.9%)
  JIT ins: solar 17.818M legacy 20.195M (-11.8%) via-IR 20.167M (-11.6%) best 19.489M (-8.6%)
  cases where JIT instructions exceed legacy: ['seeded-words +8.0%', 'solady-encoding +2.8%', 'verified-words +3.7%']
## solar-size (vs solc-legacy1, solc-ir1, best of both per case)
  interp cyc: solar 18.264M legacy 22.162M (-17.6%) via-IR 21.683M (-15.8%) best 20.909M (-12.7%)
  interp ins: solar 62.618M legacy 76.537M (-18.2%) via-IR 75.838M (-17.4%) best 72.639M (-13.8%)
  JIT cyc: solar 6.356M legacy 7.347M (-13.5%) via-IR 7.854M (-19.1%) best 6.953M (-8.6%)
  JIT ins: solar 18.359M legacy 20.223M (-9.2%) via-IR 20.167M (-9.0%) best 19.514M (-5.9%)
  cases where JIT instructions exceed legacy: ['seeded-words +8.0%', 'solady-encoding +6.9%', 'solady-lib-string +0.3%', 'verified-words +5.0%', 'word-recipes +6.1%']
```

Repeat run on a second box of the same plan (dani-solar-rerun4):

```
## solar-gas (vs solc-legacy200, solc-ir200, best of both per case)
  interp cyc: solar 16.516M legacy 21.897M (-24.6%) via-IR 21.602M (-23.5%) best 20.782M (-20.5%)
  interp ins: solar 56.761M legacy 75.680M (-25.0%) via-IR 75.428M (-24.7%) best 72.216M (-21.4%)
  JIT cyc: solar 6.212M legacy 7.317M (-15.1%) via-IR 7.856M (-20.9%) best 6.924M (-10.3%)
  JIT ins: solar 17.818M legacy 20.195M (-11.8%) via-IR 20.167M (-11.6%) best 19.489M (-8.6%)
  cases where JIT instructions exceed legacy: ['seeded-words +8.0%', 'solady-encoding +2.8%', 'verified-words +3.7%']
## solar-size (vs solc-legacy1, solc-ir1, best of both per case)
  interp cyc: solar 18.221M legacy 22.162M (-17.8%) via-IR 21.687M (-16.0%) best 20.916M (-12.9%)
  interp ins: solar 62.618M legacy 76.537M (-18.2%) via-IR 75.838M (-17.4%) best 72.639M (-13.8%)
  JIT cyc: solar 6.335M legacy 7.323M (-13.5%) via-IR 7.865M (-19.5%) best 6.947M (-8.8%)
  JIT ins: solar 18.359M legacy 20.223M (-9.2%) via-IR 20.167M (-9.0%) best 19.514M (-5.9%)
  cases where JIT instructions exceed legacy: ['seeded-words +8.0%', 'solady-encoding +6.9%', 'solady-lib-string +0.3%', 'verified-words +5.0%', 'word-recipes +6.1%']
```

## Per-case table from shapes.py (primary run)
### bundles: kcycles per case (min of 5 rounds), Δ vs solar-gas

| case | solar-gas interp | solar-gas JIT | solc-ir200 interp | solc-ir200 JIT | solc-legacy200 interp | solc-legacy200 JIT | solar-size interp | solar-size JIT | solc-ir1 interp | solc-ir1 JIT | solc-legacy1 interp | solc-legacy1 JIT |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| factorial | 51.6 | 40.8 | +97.6% | +111.3% | +147.8% | +137.0% | +16.5% | +0.3% | +97.7% | +111.8% | +150.1% | +141.4% |
| counter | 15.6 | 16.3 | +5.6% | +1.4% | +9.8% | +2.1% | -0.6% | -0.0% | +5.1% | +1.6% | +10.5% | +3.6% |
| counter-loop | 185.3 | 26.0 | +51.4% | +35.9% | +221.2% | +429.7% | +13.2% | -0.5% | +51.3% | +35.9% | +222.5% | +429.3% |
| minimal-proxy | 24.4 | 24.3 | +15.2% | +1.8% | +32.1% | +2.2% | +2.5% | -1.9% | +17.0% | +0.4% | +31.2% | +2.7% |
| sum-array | 71.5 | 14.7 | +133.0% | +795.1% | +243.5% | +946.1% | +11.6% | +25.8% | +131.6% | +790.2% | +236.6% | +960.0% |
| arithmetic | 61.8 | 30.8 | +106.4% | +215.3% | +289.7% | +357.2% | +3.4% | -4.0% | +106.4% | +211.8% | +290.1% | +349.8% |
| verified-words | 33.9 | 10.4 | +66.4% | +20.1% | +72.0% | +0.6% | +12.0% | +0.1% | +65.7% | +19.4% | +72.4% | +1.5% |
| word-recipes | 69.6 | 25.8 | +50.8% | +5.7% | +101.3% | +5.9% | +25.5% | +8.1% | +50.9% | +4.7% | +101.0% | +6.5% |
| seeded-words | 46.7 | 13.7 | +42.5% | +25.8% | +52.6% | -0.6% | +11.7% | -0.3% | +42.5% | +23.4% | +52.7% | +1.1% |
| division-words | 192.1 | 89.7 | +135.7% | +100.9% | +224.8% | +130.7% | +19.9% | +25.2% | +134.8% | +102.8% | +224.9% | +131.4% |
| calldata-loops | 610.4 | 238.2 | +144.0% | +75.7% | +175.1% | +89.1% | +26.6% | -20.9% | +145.6% | +76.0% | +176.0% | +89.4% |
| compiler-optimizations | 85.5 | 59.2 | +48.9% | +13.0% | +88.0% | +31.9% | +8.7% | +2.8% | +48.2% | +13.7% | +90.2% | +33.3% |
| openzeppelin-erc20-mock | 36.1 | 35.0 | +10.1% | +1.6% | +33.6% | +3.0% | +1.0% | +0.4% | +10.6% | +2.6% | +35.8% | +3.4% |
| openzeppelin-vesting-wallet | 59.8 | 58.9 | +18.4% | +3.3% | +40.2% | +6.3% | +2.2% | +0.6% | +17.9% | +2.1% | +47.5% | +6.6% |
| nitro-one-step-proof | 122.1 | 85.5 | +65.3% | +19.3% | +39.9% | +13.2% | +2.7% | +1.9% | +65.9% | +18.6% | +41.0% | +13.4% |
| aave-l2-encoder | 218.6 | 117.5 | +43.5% | +13.7% | +74.1% | +17.3% | +3.0% | -0.1% | +50.9% | +12.9% | +81.4% | +19.5% |
| lilweb3-ens | 52.4 | 47.4 | +18.6% | +9.0% | +36.4% | +8.5% | +2.0% | +1.7% | +20.7% | +7.5% | +38.6% | +9.1% |
| lilweb3-flashloan | 59.9 | 61.3 | +15.0% | +4.3% | +29.1% | +4.7% | +2.2% | -1.1% | +14.7% | +3.9% | +30.3% | +5.4% |
| lilweb3-fractional | 46.5 | 47.8 | +11.4% | +2.2% | +22.1% | +2.2% | -0.7% | +0.3% | +14.4% | +2.4% | +24.6% | +2.7% |
| maple-erc20 | 55.6 | 54.6 | +16.4% | +1.1% | +26.5% | +2.5% | +4.1% | +0.2% | +17.0% | +0.4% | +30.0% | +2.6% |
| openzeppelin-governor | 242.0 | 156.0 | +43.6% | +9.3% | +40.4% | +9.5% | +16.5% | +0.6% | +43.6% | +9.3% | +50.7% | +9.3% |
| openzeppelin-p256 | 3734.8 | 3041.8 | +26.9% | -0.2% | +13.2% | +3.9% | +1.7% | +0.6% | +27.6% | -0.3% | +13.4% | +3.9% |
| solady-signature-checker | 56.1 | 46.8 | +23.9% | +5.2% | +14.8% | +1.0% | +10.3% | +0.9% | +24.9% | +4.4% | +25.6% | +4.4% |
| solady-lib-string | 1847.7 | 432.9 | +47.2% | +31.0% | +47.6% | +2.9% | +25.2% | +9.6% | +47.8% | +32.1% | +55.2% | +8.0% |
| solady-encoding | 7241.8 | 994.9 | +9.7% | +76.2% | +6.7% | +0.9% | +9.1% | +2.7% | +9.7% | +75.5% | +7.1% | +1.1% |
| solady-algorithms | 1333.1 | 476.1 | +42.7% | +31.2% | +40.5% | +20.9% | +13.9% | +8.4% | +43.1% | +30.7% | +41.1% | +20.8% |

| variant | interp Mcyc | interp Mins | JIT Mcyc | JIT Mins | Δinterp cyc | ΔJIT cyc |
|---|---:|---:|---:|---:|---:|---:|
| solar-gas | 16.555 | 56.761 | 6.246 | 17.818 | +0.00% | +0.00% |
| solc-ir200 | 21.616 | 75.428 | 7.865 | 20.167 | +30.57% | +25.91% |
| solc-legacy200 | 21.913 | 75.680 | 7.314 | 20.195 | +32.36% | +17.09% |
| solar-size | 18.264 | 62.618 | 6.356 | 18.359 | +10.32% | +1.75% |
| solc-ir1 | 21.683 | 75.838 | 7.854 | 20.167 | +30.98% | +25.74% |
| solc-legacy1 | 22.162 | 76.537 | 7.347 | 20.223 | +33.87% | +17.62% |
