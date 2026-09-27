# Compiler pass parallelism

MIR function passes and EVM block cleanup use three scheduling constants to
choose between serial and parallel execution. MIR counts allocated instruction IDs across runnable functions,
or across the module when building call summaries. This is a cheap size estimate
that includes removed instructions. EVM counts live instructions across all
blocks. EVM workers process a fixed number of blocks and reuse their peephole scratch buffer within each batch.

## Calibration

The September 2026 calibration uses the corpus and `wall` command in
[`scripts/compile_perf.py`](../scripts/compile_perf.py). The machine is an AMD
EPYC 4585PX with 16 physical cores. The binary uses the profiling profile, the
`asm` feature, one codegen unit, thin LTO, and `nightly-2026-09-20`. Threads use
fixed physical-core affinity. Builds and timing runs do not overlap.

The first sweep covers all 45 successful corpus inputs from 14 archived projects:
gas, size, and unoptimized builds, plus four single-contract selections. Four
inputs with existing compile errors are excluded from timings. Five samples and
one warmup per point produce 4,050 timed compilations across these settings:

| Setting | Values tested |
| --- | --- |
| MIR instruction cutoff | 0, 256, 1,024, 4,096, 16,384, disabled |
| EVM instruction cutoff | 0, 1,024, 4,096, 16,384, 65,536, disabled |
| Blocks per task | 8, 16, 32, 64, 128, 256 |

Each sweep changes one coordinate around `(1024, 4096, 64)`. It also includes
both local scheduling paths disabled and a second copy of the baseline. This
makes 18 configurations, including the duplicate. Input and configuration order
are shuffled with seed `20260927`.

All configurations use one tuning binary with temporary environment overrides,
so recompilation and code layout do not confound the comparison. Production
code uses ordinary constants. The benchmark script retains every Hyperfine
sample under `target/compile-perf/wall/`.

Comparisons use each workload's median divided by the geometric mean of its two
baseline medians. The duplicate baselines' median absolute difference is 1.7%,
with a worst case of 9.5%. Small differences are not evidence of a better cutoff.
Eight inputs take less than 10 ms and are especially sensitive to launch overhead;
the analysis also checks the other 37 inputs separately.

### Eight-thread sweep

Positive changes are slower. The geometric mean gives each workload equal
weight; summed medians give longer compilations more weight. Comparing both
helps distinguish broad gains from results dominated by one long workload.

| Changed setting | Geometric mean | Summed medians |
| --- | ---: | ---: |
| mir-0 | +1.77% | +0.57% |
| mir-256 | +0.58% | +0.19% |
| mir-4096 | +1.01% | -0.36% |
| mir-16384 | +1.78% | +0.39% |
| mir-off | +2.26% | +1.87% |
| evm-0 | +0.72% | +0.20% |
| evm-1024 | +0.76% | -0.25% |
| evm-16384 | +0.61% | -0.93% |
| evm-65536 | +0.18% | +0.25% |
| evm-off | +1.03% | +1.10% |
| chunk-8 | +0.34% | +0.05% |
| chunk-16 | +0.24% | -1.14% |
| chunk-32 | +0.66% | +0.64% |
| chunk-128 | +1.12% | +0.57% |
| chunk-256 | +0.62% | +0.85% |
| serial-local | +2.85% | +3.75% |

The smallest geometric mean among the alternatives is still within control
noise. Disabling both local scheduling paths is materially worse on the large
single-contract cases: FuzzEngineTest takes 1,950 ms against the 1,621 ms control
reference, and PoolManagerTest takes 237 ms against 190 ms. Whole-project builds
have other parallel work and show much less dependence on local scheduling.

### Thread counts and combined settings

Eight representative workloads repeat eleven configurations at two and sixteen
threads, nine samples per point: another 1,584 timed compilations. The workloads
are Nitro gas, Solmate gas, Morpho gas, Solady size, Seaport, PoolManager,
FuzzEngineTest, and PoolManagerTest. Seeds are `20260928` and `20260929`.

The combined settings are `(256, 1024, 32)` for smaller tasks and
`(4096, 16384, 128)` for larger tasks. The two-thread Seaport controls differed by
8.3%, so that case was repeated with 25 samples per configuration and a fresh
order, adding 275 samples. The repeated controls differed by 0.26%; this repeat
replaces that case in the two-thread summary below. Both runs remain in the raw
data. Percentages are geometric means of median ratios.

| Changed setting | Two threads | Sixteen threads |
| --- | ---: | ---: |
| mir-256 | -0.24% | +0.83% |
| mir-4096 | +2.31% | +3.23% |
| evm-1024 | +0.06% | -0.70% |
| evm-16384 | +0.48% | +1.10% |
| evm-65536 | +1.19% | -0.05% |
| chunk-32 | +1.40% | -0.39% |
| chunk-128 | +0.57% | -0.18% |
| small-tasks | -0.07% | -1.16% |
| large-tasks | +2.57% | +2.95% |

A fresh eight-thread run tests both combined settings and both baseline copies
with 17 samples on each of the eight workloads, adding 544 samples. Smaller tasks
change the geometric mean by -0.26% and summed medians by +0.30%; larger tasks
change them by +2.97% and +0.28%. The two baseline geometric means differ by 1.1%.
The full calibration contains **6,453 timed compilations**, excluding warmups.

## Selected constants

Retain **1,024 MIR instruction IDs**, **4,096 EVM instructions**, and **64 blocks
per task**. The measurements support a useful range, without establishing a
unique optimum integer or a value that is best on every machine.

The MIR cutoff keeps parallelism available to medium-sized modules. Raising it
to 4,096 slows PoolManager by 6.2% at two threads; lowering it to 256 has mixed
results across the full corpus and thread counts. The EVM cutoff avoids scheduling
small cleanups, while higher cutoffs lose useful parallel work on some inputs.
Batches of 64 share task costs and scratch storage while providing work for idle
workers. Neither smaller nor larger batches show a consistent improvement across
the corpus. The combined-setting checks likewise give no reason to move all three
constants together.

These guards also respect queued compiler work: a worker with pending tasks uses
the serial local path. Debug printing, pass diffs, and pass timing use that path
too. The defaults should be revisited when the workload mix, pass costs, or
scheduler changes.

## Repeating the comparison

Use `compile_perf.py build NAME --features asm` for each proposed setting, then
compare the named binaries with `compile_perf.py wall BASE CAND --jobs N -r 17`,
adding `-i INPUT` for the corpus selections. Keep CPU affinity and build flags
fixed, include duplicate controls, and retain the exported samples. Use
`compile_perf.py identity BASE CAND` before accepting a scheduling change.

The calibration artifacts are retained locally under `target/compile-perf/`:
`parallel-calibration-results.json` contains every raw sample and configuration;
`parallel-tuning-source.patch` records the tuning build; `parallel_sweep.py` and
`confirm_parallel.sh` drive the repository's benchmark script. The normal build
has no benchmark environment overrides.
