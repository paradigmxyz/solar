# Solar blog: numbers and sources

Every number in the draft comes from one of the runs below. Gas and size are deterministic and match between CI and local runs; compile times come from the local machine unless noted.

## Runs

| Run | What | Where |
|---|---|---|
| CI | `codegen runtime benchmark`, main `37c8e3fc`, solc 0.8.37, solx 0.1.8, oksolc 0.8.36+0.1.5 | [run 39567201442](https://github.com/paradigmxyz/solar/actions/runs/39567201442) |
| L1 | Same harness and flags as CI, plus `--repeat-long-compiles` (5 samples for every compile, including slow solc ones). Solar main `2287a3a8` (bench profile), solc 0.8.37, solx 0.1.8, oksolc 0.2.2 (built locally with Zig 0.17.0, ReleaseFast, native CPU) | `solar-blog-evidence/local1-results.json` |
| L2 | Solar vs solx 0.2.0 Slang preview (`slang-linux-amd64-gnu-2026-10-09`, release `62d94366`), 3 samples | `solar-blog-evidence/local2-results.json` |
| L3 | Solar with `-j1` (one thread), whole projects, 5 samples | `solar-blog-evidence/local3-results.json` |
| L4/L5 | Whole projects with `--evm-version cancun` so solx can parse them (solx rejects pre-Cancun targets); solc + solx 0.1.8 + Solar, then Solar + Slang preview | `solar-blog-evidence/local4-results.json`, `local5-results.json` |
| Safe Solady | `upstream_tests.py --fair` on `djolertrk/solady@9dd51e8` (`feat/safe-solady-natspec`), Solar from PR #1999 head `483b9b24`, solc 0.8.37, forge 1.8.6-nightly, 32 CPUs | `solar-blog-evidence/safe-solady-*` |

Machine: AMD EPYC 4585PX, 16 cores / 32 threads, Linux, shared dev box but idle during timing runs (load < 2). Command for L1:

```sh
uv run benches/runtime/benchmark.py --solar target/release/solar --solc solc-0.8.37 \
  --solx solx-0.1.8 --oksolc oksolc-0.2.2 --mode runtime compile-time --suite all \
  --compile-repeats 5 --repeat-long-compiles --gas --gas-profile hot --start-anvil \
  --allow-failures --output local1/results.json
```

Aggregates are equal-weight geometric means of per-workload ratios, using the harness's `comparable_gas` (excludes the gas-dependent LibString brutalizer calls). "X% less" = 100 × (1 − Solar/reference). Speedup = reference time / Solar time, medians.

## Whole-project compile time (L1, median of 5)

Each project's archived Standard JSON input with its own settings. Solar runs with its default 8 threads; solc is single-threaded.

| Project | Settings | solc 0.8.37 | solx 0.1.8 | oksolc 0.2.2 | Solar (8 threads) | Solar (1 thread, L3) | Speedup (8t / 1t) |
|---|---|---:|---:|---:|---:|---:|---:|
| Seaport 1.6 | legacy, london | 49.76 s | input rejected | legacy unsupported | 2.497 s | 14.82 s | 19.9× / 3.4× |
| Uniswap v4 core | via-IR, cancun | stack too deep | 6.214 s | 5.526 s | 0.281 s | 1.742 s | — |
| Morpho Blue | via-IR, paris | 23.43 s | input rejected | 3.341 s | 0.171 s | 1.039 s | 137× / 22.5× |
| OpenZeppelin 5.6.1 | legacy, osaka | 8.638 s | 5.255 s | legacy unsupported | 0.308 s | 1.684 s | 28.0× / 5.1× |
| Solady 0.1.26 | legacy, paris | 12.20 s | input rejected | legacy unsupported | 0.535 s | 3.435 s | 22.8× / 3.6× |
| forge-std 1.16.1 | legacy, prague | 3.704 s | 3.230 s | legacy unsupported | 0.162 s | 0.778 s | 22.9× / 4.8× |
| PRBMath 4.1.1 | legacy, shanghai | 2.869 s | input rejected | legacy unsupported | 0.131 s | 0.803 s | 21.9× / 3.6× |
| Solmate 6 | legacy, london | 1.571 s | input rejected | legacy unsupported | 0.181 s | 0.503 s | 8.7× / 3.1× |
| Solarray | legacy, cancun | 0.446 s | 0.530 s | legacy unsupported | 0.017 s | 0.022 s | 26.2× / 20.0× |
| Tempo Zones (incl. tests) | via-IR | 29.18 s | pragma too new | 7.509 s | 0.476 s | 1.355 s | 61.3× / 21.5× |

Geomean over the 9 projects solc builds: **28.5×** (8 threads), **6.8×** (1 thread). CI's own numbers: 32.0× (CI caps slow compiles at one sample). Peak RSS, Seaport: solc 2,657 MiB, Solar 618 MiB. Morpho: solc 890 MiB, oksolc 2,837 MiB, Solar 218 MiB.

Pairwise whole-project speedups: vs solx 0.1.8 22.1× on the 4 projects it accepts as archived (L1), 19.5× on 8 projects with a Cancun target (L4); vs oksolc 18.2× (3 projects, L1); vs Slang preview 11.6× (4 projects, L2) and 13.1× on 7 projects with a Cancun target (L5). With a Cancun target solc vs Solar is 31.6× (L4, 3 samples).

Cancun-target whole-project times (L4/L5, median of 3): solx 0.1.8 Morpho 3.009 s, Solady 8.916 s, OZ 5.244 s, v4 6.178 s, forge-std 3.059 s, PRBMath 2.373 s, Solmate 1.450 s, Solarray 0.531 s, Seaport fails ("cannot be compiled due to a combination of..." codegen limit), Tempo Zones fails (frontend is solc 0.8.34). Slang preview: Morpho 2.514 s, Solady 6.720 s, OZ 3.320 s, v4 4.106 s, Solmate 2.004 s, Solarray 0.099 s, Tempo Zones 8.620 s; Seaport, forge-std and PRBMath fail (invalid Sol module, stackification failure, MLIR translation error).

## Runtime gas, deployment gas, bytecode size (L1; identical to CI except where main moved)

| Reference | Corpus | Cases | Compile speedup | Runtime gas | Deploy gas | Runtime size | Gas wins |
|---|---|---:|---:|---:|---:|---:|---:|
| solc 0.8.37 | project-derived | 15 | 5.68× | −4.14% | −26.20% | −30.04% | 15/15 |
| solc 0.8.37 | micro | 12 | 4.24× | −17.55% | −19.29% | −34.20% | 12/12 |
| solc 0.8.37 | all | 27 | 4.99× | −10.35% | −23.20% | −31.92% | 27/27 |
| solx 0.1.8 | project-derived | 13 | 5.11× | −1.41% | −34.81% | −39.80% | 12/13 |
| solx 0.1.8 | all | 25 | 4.88× | −5.63% | −27.93% | −37.32% | 24/25 |
| oksolc 0.2.2 | project-derived | 11 | 4.11× | −2.66% | −28.76% | −33.50% | 11/11 |
| solx 0.2.0 Slang preview (L2) | project-derived | 13 | 5.10× | −1.71% | −42.41% | −47.72% | 10/13 |
| solx 0.2.0 Slang preview (L2) | all | 25 | 4.98× | −5.68% | −32.19% | −40.77% | 22/25 |

Gas is transaction receipt gas summed over each workload's calls, so it includes 21,000 base gas and calldata per transaction. oksolc emits the same bytecode as solc on every case it builds, so its gas and size equal solc's.

Losses: solx 0.1.8 wins OpenZeppelin P256 (440,533 vs 452,567, Solar +2.7%). The Slang preview wins Aave L2 encoder (−0.8%), OpenZeppelin governor (−0.1%) and P256 (−0.4%) while emitting 4.0×, 2.4× and 2.4× larger code on those cases.

### Per-workload runtime gas (L1)

| Workload | solc | solx 0.1.8 | Slang preview | Solar | vs solc |
|---|---:|---:|---:|---:|---:|
| factorial (solx blog example 1) | 224,168 | 209,744 | 209,561 | 186,088 | −17.0% |
| sum-array | 229,597 | 194,732 | 194,752 | 149,688 | −34.8% |
| arithmetic | 158,924 | 139,426 | 138,961 | 115,794 | −27.1% |
| division-words | 378,531 | 311,016 | 324,298 | 276,779 | −26.9% |
| calldata-loops | 728,604 | 459,067 | 459,600 | 379,541 | −47.9% |
| OpenZeppelin ERC20 mock | 381,297 | 380,853 | 380,449 | 379,759 | −0.4% |
| Arbitrum Nitro one-step proof | 366,604 | 346,838 | 355,988 | 338,254 | −7.7% |
| Aave L2 encoder | 635,207 | 615,321 | 598,390 | 603,251 | −5.0% |
| OpenZeppelin governor | 577,338 | 566,775 | 565,884 | 566,187 | −1.9% |
| OpenZeppelin P256 | 534,674 | 440,533 | 450,674 | 452,567 | −15.4% |
| Solady LibString | 1,989,555 | fails | fails | 1,720,404 | −13.5% |
| Solady encoding | 6,756,398 | 6,754,574 | 6,878,199 | 6,421,998 | −4.9% |
| Solady algorithms | 2,555,843 | 2,661,134 | 2,674,592 | 2,443,084 | −4.4% |
| Tempo Zones portal | 222,078 | 217,860 | 217,730 | 217,020 | −2.3% |

Tempo Zones portal deployment: 5,242,485 → 3,774,182 gas (−28.0%); runtime 23,994 → 17,208 bytes.

Factorial: Solar's MIR keeps `result` in a phi across the checked loop and emits one `sstore` after it (CI artifact `factorial/solar/mir.mir`); solc keeps the store in the loop, solx keeps one more storage access.

## Safe Solady vs Solady (upstream suites, `--fair`)

Upstream Solady built by solc 0.8.37 (via-IR) against the safe port built by Solar PR #1999, same inputs and memory, fuzz seed fixed, Brutalizer off.

| Metric | Value |
|---|---:|
| Tests passing on both legs | 1,558 of 1,561 collected (`testSafeMoveETHViaMover` fails on both legs with this forge; 2 invariant tests report no gas) |
| Tests where the safe port is cheaper | 1,479 (95%) |
| Geomean of per-test gas ratios | 0.812× (−19%) |
| Sum of gas | 1,348,141,853 vs 1,732,927,472 = 0.778× |
| Sum without the one 98M-gas outlier (ECDSA max-length revert) | 0.825× |

The PR description reports 0.8138× over 1,481 tests by sum (1.159B vs 1.425B) on another machine; forge splits fuzz runs per CPU, so sums move with core count. The geomean and win rate are the stable figures. Best suites: Base64 0.56×, LibBit 0.73×, LibSort 0.76×, LibString 0.82×, SignatureCheckerLib 0.84×. Worst: ReentrancyGuard 1.007×.

Differential matrix (`benchmarks/checked/benchmark.py --solc solc-0.8.37 --solar <PR #1999> --no-cache`, Cancun, 200 runs): 28,413 cases, 28,354 comparable. The safe port built by Solar matches the Python oracle on every case; all 177 mismatching executions (59 cases) are upstream Solady legs (`toNibbles`, `directReturn`, `split`, `uint8At`, `toHexString(uint256,uint256)`), excluded from gas. Opcode gas, excluding intrinsic gas, against the per-call minimum of solc legacy and via-IR:

| Metric | Value |
|---|---:|
| Sum, safe port (Solar) / upstream (best solc) | 0.5864× (PR #1999 reports 0.5863×) |
| Geomean per call | 0.630× |
| Calls where the safe port is cheaper | 25,674 of 28,354 (90.5%) |
| Safe port (Solar) / upstream Solady (Solar) | 0.876× |

Per library (sum): Base64 0.623×, ECDSA 0.951×, EfficientHashLib 0.845×, LibBit 0.663×, LibBytes 0.844×, LibSort 0.437×, LibString 0.975×, MerkleProofLib 0.877×, SafeCastLib 0.724×, SignatureCheckerLib 0.990×. The safe LibSort harness is over EIP-170 under both compilers (Solar 27,995 bytes); safe harnesses are larger than upstream in general, e.g. Base64 6,670 vs 935 bytes with Solar.

## Correctness claims

- Every Solar runtime leg in CI and L1 matches the reference outputs; no runtime mismatch.
- External Foundry suite (morpho-blue 145, solmate 55, solady 1,572, seaport 476, OpenZeppelin 343, v4-core 598 tests = 3,189) matches solc: reported in PR #1999 on its branch. [Re-run on the release commit before publishing.]

## Caveats to keep in mind when editing

- The 28× headline uses Solar's default 8 threads. Single-threaded it is 6.8×. The draft says so.
- solx "fails" on most whole projects only because it rejects pre-Cancun `evmVersion` values; L4/L5 rerun with Cancun for a like-for-like time.
- Safe Solady needs PR #1999 (open). Without it, drop section 3 or say "coming in the next release".
- `solarSafety` reporting and `@custom:solar-*` tags are also from PR #1999.
- Foundry integration today is through `FOUNDRY_SOLC` + `SOLC_WRAPPER=1`; there is no `forge` flag yet.
