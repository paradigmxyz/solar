# evm2 comb4 (dev box, Zen 5), Solar fe4b93c2 vs solc 0.8.38

```
## solar-gas (vs solc-legacy200, solc-ir200, best of both per case)
  interp cyc: solar 12.530M legacy 16.677M (-24.9%) via-IR 16.580M (-24.4%) best 15.907M (-21.2%)
  interp ins: solar 61.786M legacy 83.099M (-25.6%) via-IR 82.642M (-25.2%) best 79.156M (-21.9%)
  JIT cyc: solar 5.204M legacy 6.134M (-15.2%) via-IR 6.497M (-19.9%) best 5.822M (-10.6%)
  JIT ins: solar 18.396M legacy 20.848M (-11.8%) via-IR 20.741M (-11.3%) best 20.128M (-8.6%)
  cases where JIT instructions exceed legacy: ['seeded-words +8.0%', 'solady-encoding +2.6%', 'verified-words +4.4%']
## solar-size (vs solc-legacy1, solc-ir1, best of both per case)
  interp cyc: solar 13.741M legacy 17.011M (-19.2%) via-IR 16.663M (-17.5%) best 16.008M (-14.2%)
  interp ins: solar 68.405M legacy 84.059M (-18.6%) via-IR 83.092M (-17.7%) best 79.593M (-14.1%)
  JIT cyc: solar 5.278M legacy 6.144M (-14.1%) via-IR 6.503M (-18.8%) best 5.835M (-9.5%)
  JIT ins: solar 18.940M legacy 20.878M (-9.3%) via-IR 20.741M (-8.7%) best 20.154M (-6.0%)
  cases where JIT instructions exceed legacy: ['seeded-words +8.9%', 'solady-encoding +6.0%', 'solady-lib-string +4.0%', 'verified-words +6.7%', 'word-recipes +6.3%']
```

## solar-gas vs best of solc-ir200/solc-legacy200 per case, instructions (kins)

| case | best solc interp / JIT kins | solar-gas gas / interp / JIT |
|---|---:|---:|
| solady-encoding | 32612.1 / 3476.4 | -5.9% / -9.4% / +2.6% |
| openzeppelin-p256 | 14190.3 / 9963.5 | -16.4% / -11.5% / -1.0% |
| solady-lib-string | 10213.2 / 1174.3 | -15.9% / -33.9% / -7.6% |
| solady-algorithms | 6982.9 / 1668.8 | -4.1% / -31.0% / -27.8% |
| calldata-loops | 5744.8 / 642.8 | -48.5% / -58.6% / -45.9% |
| openzeppelin-governor | 1213.8 / 463.2 | -2.0% / -31.0% / -7.0% |
| division-words | 1679.1 / 322.3 | -27.0% / -58.0% / -49.3% |
| aave-l2-encoder | 1084.3 / 314.2 | -5.4% / -37.7% / -22.5% |
| counter-loop | 1024.9 / 96.3 | -13.6% / -34.4% / -30.0% |
| nitro-one-step-proof | 611.3 / 266.6 | -4.4% / -31.0% / -13.2% |
| sum-array | 554.8 / 284.3 | -34.8% / -54.6% / -88.3% |
| word-recipes | 374.2 / 65.1 | -6.9% / -35.9% / -7.5% |
| compiler-optimizations | 395.8 / 137.7 | -3.1% / -39.7% / -12.7% |
| arithmetic | 435.4 / 192.2 | -27.1% / -47.3% / -69.7% |
| factorial | 331.9 / 184.7 | -17.0% / -49.5% / -61.4% |
| seeded-words | 228.8 / 30.6 | -7.4% / -32.7% / +8.0% ⚠️ |
| solady-signature-checker | 187.3 / 105.7 | -0.6% / -18.1% / -3.3% |
| openzeppelin-vesting-wallet | 191.6 / 116.6 | -1.2% / -23.2% / -3.6% |
| maple-erc20 | 180.0 / 112.3 | -0.8% / -18.5% / -0.9% |
| lilweb3-flashloan | 179.7 / 127.1 | -0.9% / -18.7% / -4.7% |
| lilweb3-ens | 174.1 / 105.1 | -0.9% / -20.0% / -8.9% |
| lilweb3-fractional | 143.9 / 98.1 | -0.3% / -15.8% / -2.3% |
| verified-words | 201.2 / 23.2 | -8.8% / -44.6% / +4.4% |
| openzeppelin-erc20-mock | 106.3 / 72.7 | -0.4% / -13.4% / -1.3% |
| minimal-proxy | 77.1 / 51.6 | -0.6% / -19.4% / -5.5% |
| counter | 37.3 / 33.0 | -0.2% / -9.3% / -2.3% |
| **total** | 79156 / 20128 | / -21.9% / -8.6% |

## solar-gas vs best of solc-ir200/solc-legacy200 per case, cycles (kcyc)

| case | best solc interp / JIT kcyc | solar-gas gas / interp / JIT |
|---|---:|---:|
| solady-encoding | 5737.8 / 771.9 | -5.9% / -9.6% / -1.8% |
| openzeppelin-p256 | 3527.8 / 2696.5 | -16.4% / -11.3% / -0.2% |
| solady-lib-string | 2006.6 / 358.0 | -15.9% / -31.2% / -7.3% |
| solady-algorithms | 1401.5 / 472.7 | -4.1% / -28.8% / -21.5% |
| calldata-loops | 1084.1 / 353.1 | -48.5% / -58.6% / -42.8% |
| openzeppelin-governor | 277.7 / 145.9 | -2.0% / -26.8% / -8.1% |
| aave-l2-encoder | 237.6 / 111.9 | -5.4% / -31.6% / -15.9% |
| division-words | 333.4 / 152.9 | -27.0% / -56.6% / -51.1% |
| counter-loop | 199.4 / 25.9 | -13.6% / -32.0% / -16.9% |
| nitro-one-step-proof | 140.6 / 80.4 | -4.4% / -29.3% / -11.3% |
| compiler-optimizations | 92.9 / 50.2 | -3.1% / -31.2% / -9.1% |
| sum-array | 128.5 / 101.7 | -34.8% / -59.1% / -89.1% |
| word-recipes | 76.9 / 21.1 | -6.9% / -32.5% / -5.0% |
| arithmetic | 109.0 / 78.6 | -27.1% / -55.5% / -68.2% |
| openzeppelin-vesting-wallet | 55.7 / 46.1 | -1.2% / -15.2% / -3.1% |
| lilweb3-flashloan | 54.6 / 49.1 | -0.9% / -14.2% / -4.7% |
| solady-signature-checker | 51.1 / 37.2 | -0.6% / -12.6% / -2.9% |
| maple-erc20 | 51.6 / 43.2 | -0.8% / -14.2% / -1.8% |
| lilweb3-ens | 49.0 / 37.1 | -0.9% / -14.8% / -6.3% |
| factorial | 92.9 / 72.9 | -17.0% / -56.0% / -54.6% |
| lilweb3-fractional | 40.3 / 37.1 | -0.3% / -11.4% / -1.8% |
| seeded-words | 47.8 / 10.7 | -7.4% / -29.0% / +2.8% |
| openzeppelin-erc20-mock | 31.9 / 27.3 | -0.4% / -9.3% / -0.9% |
| verified-words | 43.1 / 8.1 | -8.8% / -42.9% / +2.0% |
| minimal-proxy | 22.1 / 20.0 | -0.6% / -13.8% / -2.2% |
| counter | 12.6 / 12.3 | -0.2% / -4.9% / -1.0% |
| **total** | 15907 / 5822 | / -21.2% / -10.6% |

## solar-size vs best of solc-ir1/solc-legacy1 per case, instructions (kins)

| case | best solc interp / JIT kins | solar-size gas / interp / JIT |
|---|---:|---:|
| solady-encoding | 32704.4 / 3476.4 | -1.8% / -2.4% / +6.0% ⚠️ |
| openzeppelin-p256 | 14196.9 / 9963.3 | -4.1% / -7.1% / -0.4% |
| solady-lib-string | 10400.6 / 1193.3 | -8.3% / -19.6% / +4.0% |
| solady-algorithms | 7023.9 / 1668.8 | -2.5% / -23.4% / -23.8% |
| calldata-loops | 5747.2 / 642.8 | -40.9% / -46.0% / -37.9% |
| openzeppelin-governor | 1243.3 / 466.4 | -2.1% / -21.1% / -6.4% |
| division-words | 1679.1 / 322.3 | -23.2% / -48.2% / -32.7% |
| counter-loop | 1024.9 / 96.3 | -9.2% / -25.4% / -31.1% |
| aave-l2-encoder | 1118.5 / 314.0 | -5.2% / -36.4% / -20.9% |
| nitro-one-step-proof | 619.1 / 268.2 | -4.0% / -28.8% / -12.9% |
| word-recipes | 374.2 / 65.1 | -3.0% / -17.1% / +6.3% ⚠️ |
| sum-array | 554.8 / 284.3 | -33.7% / -47.2% / -83.9% |
| compiler-optimizations | 396.7 / 137.8 | -2.6% / -33.3% / -9.2% |
| arithmetic | 435.4 / 192.2 | -27.1% / -45.7% / -70.5% |
| factorial | 331.9 / 184.7 | -16.1% / -44.1% / -59.4% |
| solady-signature-checker | 207.3 / 106.5 | -0.5% / -13.9% / -3.1% |
| seeded-words | 228.8 / 30.6 | -4.7% / -22.9% / +8.9% ⚠️ |
| maple-erc20 | 180.0 / 112.1 | -0.6% / -14.6% / -0.2% |
| openzeppelin-vesting-wallet | 189.7 / 116.6 | -1.0% / -20.8% / -3.1% |
| lilweb3-flashloan | 179.7 / 127.1 | -0.9% / -18.3% / -4.5% |
| lilweb3-ens | 181.3 / 105.1 | -0.9% / -20.4% / -8.6% |
| verified-words | 201.2 / 23.2 | -6.1% / -36.1% / +6.7% ⚠️ |
| lilweb3-fractional | 147.1 / 98.1 | -0.3% / -16.1% / -2.2% |
| openzeppelin-erc20-mock | 109.3 / 74.1 | -0.3% / -12.2% / -3.0% |
| minimal-proxy | 80.5 / 51.6 | -0.6% / -19.8% / -5.2% |
| counter | 37.3 / 33.0 | -0.2% / -9.3% / -2.3% |
| **total** | 79593 / 20154 | / -14.1% / -6.0% |

## solar-size vs best of solc-ir1/solc-legacy1 per case, cycles (kcyc)

| case | best solc interp / JIT kcyc | solar-size gas / interp / JIT |
|---|---:|---:|
| solady-encoding | 5796.8 / 771.7 | -1.8% / -4.2% / -0.4% |
| openzeppelin-p256 | 3527.5 / 2701.7 | -4.1% / -8.7% / -0.1% |
| solady-lib-string | 2026.9 / 365.7 | -8.3% / -15.6% / +9.5% ⚠️ |
| solady-algorithms | 1408.1 / 473.6 | -2.5% / -19.3% / -13.1% |
| calldata-loops | 1083.8 / 354.4 | -40.9% / -44.6% / -66.1% |
| openzeppelin-governor | 280.8 / 146.4 | -2.1% / -16.4% / -7.0% |
| division-words | 333.0 / 152.7 | -23.2% / -46.2% / -38.8% |
| aave-l2-encoder | 241.6 / 111.3 | -5.2% / -30.4% / -14.1% |
| counter-loop | 199.3 / 25.9 | -9.2% / -24.3% / -20.7% |
| nitro-one-step-proof | 141.8 / 80.2 | -4.0% / -27.5% / -9.5% |
| compiler-optimizations | 93.8 / 50.2 | -2.6% / -25.9% / -6.0% |
| word-recipes | 76.3 / 21.2 | -3.0% / -14.7% / +2.8% |
| sum-array | 128.0 / 101.2 | -33.7% / -53.6% / -86.1% |
| arithmetic | 108.4 / 78.6 | -27.1% / -53.6% / -68.7% |
| solady-signature-checker | 57.0 / 37.2 | -0.5% / -12.0% / -1.4% |
| openzeppelin-vesting-wallet | 55.6 / 45.7 | -1.0% / -14.0% / -1.5% |
| lilweb3-flashloan | 54.6 / 48.5 | -0.9% / -13.2% / -3.0% |
| maple-erc20 | 51.8 / 43.5 | -0.6% / -12.0% / -1.6% |
| factorial | 93.1 / 73.2 | -16.1% / -52.3% / -54.2% |
| lilweb3-ens | 49.6 / 37.1 | -0.9% / -14.2% / -6.3% |
| seeded-words | 47.5 / 10.6 | -4.7% / -21.1% / +2.7% |
| lilweb3-fractional | 41.1 / 36.5 | -0.3% / -12.6% / -0.9% |
| openzeppelin-erc20-mock | 32.7 / 27.7 | -0.3% / -9.0% / -1.6% |
| verified-words | 43.0 / 8.2 | -6.1% / -36.2% / +0.4% |
| minimal-proxy | 22.8 / 19.8 | -0.6% / -14.4% / -1.8% |
| counter | 12.8 / 12.3 | -0.2% / -6.7% / -1.1% |
| **total** | 16008 / 5835 | / -14.2% / -9.5% |
