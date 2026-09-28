# Queue-guard comparison

Measured on 2026-09-28 using `scripts/compile_perf.py wall`, with profiling
builds, `asm`, one codegen unit, thin LTO, and `nightly-2026-09-20`.
The host is an AMD EPYC 4585PX (16 physical cores). Each compiler uses eight
threads pinned to physical cores 6–13. Builds, identity checks, and timing
runs do not overlap.

The three binaries are main (`b0afb3f30`), the standalone parallelism PR
(`5778fa12b`), and that PR with its three `current_thread_has_pending_tasks`
checks removed. The instruction cutoffs and block chunk size stay unchanged.
`queue-guard-off.patch` and `queue-guard-binary-hashes.txt` identify the variant
and executables. The temporary source edits were restored after building.

The corpus has 41 successful whole-project configurations from 14 projects,
covering gas, size, and no optimization, plus four single-contract selections.
Solady without optimization and all three Uniswap V2 configurations already
fail on main and are excluded from timing. They remain in the 49-input
output comparison. All 45 successful outputs match with the guard disabled;
the failing Solady input differs only in which code-size warnings precede
the same compilation error.

Two rounds use 11 samples per binary per input, with one warmup per command.
Workload order and each workload's initial binary order are shuffled using
seed 20260928. The second round reverses each workload's binary order and
reshuffles workloads. This gives 2,970 timed compilations, excluding warmups.
Time changes use the geometric mean of the two within-round median ratios.
This preserves the control comparison when host speed shifts between rounds;
ratios of medians pooled across rounds can even reverse the within-round
direction. Both per-round ratios and pooled absolute medians are reported.

Positive unguarded-versus-guarded percentages mean removing the guard is
slower. Geometric means weight inputs equally; tiny inputs are reported
separately from inputs taking at least 10 ms. This experiment tests the
queue heuristic at eight threads; it does not recalibrate the other cutoffs.

Raw samples and their binary order are embedded in
`target/compile-perf/queue-guard-sweep/manifest.json` and retained in the wall
command's exported JSON files. `queue_guard_sweep.py` drives the repository
script; `analyze_queue_guard.py` computes the medians and ratios.

## Full-corpus result

The guard has no clear aggregate benefit in this run: disabling it changes the
whole-project geometric mean by +0.18%, and the mean across all 45 inputs by
+0.12%. Among the 37 inputs taking at least 10 ms, the change is +0.15%.
Only 15 of 45 inputs keep the same direction of guard effect in both rounds.
These results do not reproduce the earlier claim that the guard removes a
roughly 5% Morpho regression.

The PR versus main changes the whole-project geometric mean by -0.27%,
which is effectively flat. Gains concentrate in selected single contracts
(-15.5% to -19.9%), Nitro gas/size (-13.6%/-9.1%), and Solmate gas (-9.5%).
Regressions in this sweep include PRB Math size (+8.1%), v4-core unoptimized (+5.2%),
and Morpho size (+4.4%). The following tables retain every project.

### PR versus main

Positive values mean the PR is slower.

| Project | Gas | Size | Unoptimized |
| --- | ---: | ---: | ---: |
| aave-l2-encoder | -2.1% | +0.8% | +1.4% |
| forge-std-1.16.1 | +2.0% | -2.4% | -0.2% |
| lilweb3-ens | +2.1% | +3.4% | +2.4% |
| lilweb3-runtime | +1.8% | -0.7% | +0.9% |
| maple-erc20 | -0.6% | -0.5% | -2.9% |
| morpho-blue-1.0.0 | +3.3% | +4.4% | -4.1% |
| nitro-one-step-proof | -13.6% | -9.1% | -0.1% |
| openzeppelin-5.6.1 | +2.1% | +1.3% | +1.3% |
| prb-math-4.1.1 | +3.3% | +8.1% | -3.4% |
| seaport-1.6 | -0.5% | -0.9% | -0.5% |
| solady-0.1.26 | +1.9% | +1.4% | Existing error |
| solarray-a547630 | -2.3% | +0.0% | +2.7% |
| solmate-6 | -9.5% | -4.9% | -0.6% |
| v4-core-4.0.0 | +1.1% | -0.1% | +5.2% |

### Removing the guard

Positive values mean the unguarded variant is slower than the PR.

| Project | Gas | Size | Unoptimized |
| --- | ---: | ---: | ---: |
| aave-l2-encoder | +2.7% | -1.2% | +0.2% |
| forge-std-1.16.1 | +0.1% | +4.9% | +2.0% |
| lilweb3-ens | +0.9% | -0.5% | -3.4% |
| lilweb3-runtime | +1.2% | -0.4% | +0.3% |
| maple-erc20 | +0.4% | +0.1% | +1.8% |
| morpho-blue-1.0.0 | -0.7% | +0.7% | +1.6% |
| nitro-one-step-proof | -0.2% | -0.3% | -0.2% |
| openzeppelin-5.6.1 | -0.4% | -1.3% | -0.4% |
| prb-math-4.1.1 | +0.2% | -3.4% | -1.2% |
| seaport-1.6 | -1.0% | +1.3% | -0.5% |
| solady-0.1.26 | +0.1% | -0.5% | Existing error |
| solarray-a547630 | +0.3% | +0.9% | -0.8% |
| solmate-6 | +0.3% | +0.6% | -0.6% |
| v4-core-4.0.0 | +3.1% | +0.9% | -0.2% |

## Confirmation with duplicate controls

Four inputs were selected after the broad sweep: Morpho gas and size check
the original motivation, v4-core gas checks a guard benefit seen in both
initial rounds, and PRB Math size checks a loss seen in both initial rounds.
Each runs 25 samples of main, the unguarded binary, and two copies of the
same guarded binary. Guarded copies
bracket the other two, whose order is shuffled with seed 20260929.
Ratios use the geometric mean of the two guarded medians as their control.
These 400 additional timings bring the total to **3,370**. They are reported
separately and do not replace points in the full-corpus tables. The control
gap shows drift between identical binaries; small effects need that context.

| Input | Guarded control gap | Unguarded vs guarded | Guarded vs main |
| --- | ---: | ---: | ---: |
| morpho-blue-1.0.0-gas | +2.9% | -3.2% | +8.5% |
| morpho-blue-1.0.0-size | -1.3% | -1.6% | +4.5% |
| prb-math-4.1.1-size | +0.3% | +2.3% | +1.0% |
| v4-core-4.0.0-gas | +2.8% | -2.0% | -2.0% |

The confirmation reverses the apparent guard benefit on v4-core gas and
the apparent guard loss on PRB Math size. Morpho gas still does not show
the historical benefit. Control drift reaches 2.9%, so these small
differences do not support a strong general claim for the queue heuristic.
The PRB Math size regression versus main also shrinks from +8.1% in the
broad sweep to +1.0% in confirmation.

## Absolute timings and round effects

Absolute times are pooled medians in milliseconds. Percentage changes combine
within-round ratios as described above, so they need not equal the ratio of
the displayed pooled medians. The last column shows both rounds of the guard
comparison; direction reversals should not be presented as reliable gains.

| Input | Main ms | PR ms | Unguarded ms | PR vs main | Unguarded vs PR | Guard effect by round |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| aave-l2-encoder-gas | 12.82 | 12.58 | 12.82 | -2.1% | +2.7% | +1.8% / +3.6% |
| aave-l2-encoder-none | 8.58 | 8.75 | 8.69 | +1.4% | +0.2% | +1.5% / -1.2% |
| aave-l2-encoder-size | 12.66 | 12.80 | 12.61 | +0.8% | -1.2% | +0.4% / -2.8% |
| forge-std-1.16.1-gas | 219.06 | 223.21 | 223.25 | +2.0% | +0.1% | +2.8% / -2.6% |
| forge-std-1.16.1-none | 116.24 | 113.99 | 116.91 | -0.2% | +2.0% | +5.4% / -1.2% |
| forge-std-1.16.1-size | 206.56 | 203.37 | 203.90 | -2.4% | +4.9% | -0.3% / +10.4% |
| lilweb3-ens-gas | 3.82 | 3.92 | 3.97 | +2.1% | +0.9% | -1.3% / +3.2% |
| lilweb3-ens-none | 2.90 | 2.90 | 2.82 | +2.4% | -3.4% | -1.7% / -5.0% |
| lilweb3-ens-size | 3.77 | 3.85 | 3.86 | +3.4% | -0.5% | +1.7% / -2.6% |
| lilweb3-runtime-gas | 13.66 | 13.89 | 14.14 | +1.8% | +1.2% | +0.6% / +1.7% |
| lilweb3-runtime-none | 7.62 | 7.70 | 7.66 | +0.9% | +0.3% | -0.3% / +0.9% |
| lilweb3-runtime-size | 14.60 | 14.50 | 14.48 | -0.7% | -0.4% | -0.6% / -0.1% |
| maple-erc20-gas | 9.10 | 9.21 | 9.11 | -0.6% | +0.4% | +2.2% / -1.3% |
| maple-erc20-none | 5.46 | 5.45 | 5.53 | -2.9% | +1.8% | +2.2% / +1.5% |
| maple-erc20-size | 9.21 | 9.18 | 9.18 | -0.5% | +0.1% | +1.4% / -1.2% |
| morpho-blue-1.0.0-gas | 283.84 | 291.99 | 284.98 | +3.3% | -0.7% | +1.2% / -2.6% |
| morpho-blue-1.0.0-none | 145.40 | 143.76 | 145.77 | -4.1% | +1.6% | +0.3% / +3.0% |
| morpho-blue-1.0.0-size | 238.52 | 251.43 | 249.51 | +4.4% | +0.7% | +4.3% / -2.9% |
| nitro-one-step-proof-gas | 38.25 | 33.12 | 32.98 | -13.6% | -0.2% | -2.2% / +1.9% |
| nitro-one-step-proof-none | 19.09 | 19.17 | 19.15 | -0.1% | -0.2% | +0.2% / -0.6% |
| nitro-one-step-proof-size | 41.10 | 37.35 | 37.54 | -9.1% | -0.3% | -1.5% / +0.9% |
| openzeppelin-5.6.1-gas | 326.38 | 333.52 | 332.10 | +2.1% | -0.4% | -0.8% / +0.0% |
| openzeppelin-5.6.1-none | 212.36 | 215.82 | 212.79 | +1.3% | -0.4% | -2.9% / +2.1% |
| openzeppelin-5.6.1-size | 341.86 | 342.67 | 340.76 | +1.3% | -1.3% | +0.5% / -2.9% |
| prb-math-4.1.1-gas | 141.59 | 147.09 | 146.59 | +3.3% | +0.2% | -2.1% / +2.7% |
| prb-math-4.1.1-none | 80.96 | 78.10 | 77.70 | -3.4% | -1.2% | -2.8% / +0.4% |
| prb-math-4.1.1-size | 127.59 | 136.15 | 132.24 | +8.1% | -3.4% | -5.4% / -1.4% |
| seaport-1.6-gas | 2795.21 | 2788.81 | 2758.70 | -0.5% | -1.0% | -0.5% / -1.4% |
| seaport-1.6-gas-fuzz-engine | 2041.11 | 1712.10 | 1716.71 | -15.8% | -0.3% | -0.9% / +0.4% |
| seaport-1.6-gas-single | 134.33 | 113.16 | 112.82 | -15.6% | -0.2% | -0.5% / +0.1% |
| seaport-1.6-none | 1270.78 | 1263.87 | 1264.43 | -0.5% | -0.5% | -0.5% / -0.4% |
| seaport-1.6-size | 2384.71 | 2365.41 | 2395.07 | -0.9% | +1.3% | +2.1% / +0.5% |
| solady-0.1.26-gas | 604.87 | 617.84 | 615.71 | +1.9% | +0.1% | +1.4% / -1.2% |
| solady-0.1.26-size | 514.92 | 523.20 | 521.77 | +1.4% | -0.5% | +0.0% / -1.0% |
| solarray-a547630-gas | 18.99 | 18.66 | 18.65 | -2.3% | +0.3% | +2.0% / -1.3% |
| solarray-a547630-none | 14.30 | 14.59 | 14.45 | +2.7% | -0.8% | -2.1% / +0.5% |
| solarray-a547630-size | 19.61 | 19.76 | 19.75 | +0.0% | +0.9% | +1.6% / +0.3% |
| solmate-6-gas | 266.29 | 239.64 | 244.11 | -9.5% | +0.3% | -0.3% / +0.9% |
| solmate-6-none | 145.67 | 145.61 | 144.90 | -0.6% | -0.6% | +0.1% / -1.2% |
| solmate-6-size | 137.42 | 131.64 | 130.66 | -4.9% | +0.6% | +0.6% / +0.5% |
| v4-core-4.0.0-gas | 298.24 | 301.13 | 308.59 | +1.1% | +3.1% | +2.5% / +3.8% |
| v4-core-4.0.0-gas-pool-test | 238.30 | 190.67 | 192.38 | -19.9% | +0.1% | +1.0% / -0.8% |
| v4-core-4.0.0-gas-single | 80.60 | 67.85 | 67.19 | -15.5% | -1.5% | -4.6% / +1.7% |
| v4-core-4.0.0-none | 174.52 | 185.52 | 184.87 | +5.2% | -0.2% | -0.4% / -0.1% |
| v4-core-4.0.0-size | 312.54 | 310.24 | 312.38 | -0.1% | +0.9% | +0.5% / +1.3% |

The confirmation samples and order are retained in
`target/compile-perf/queue-guard-confirm/manifest.json`.
No production source changes were made for this measurement.
