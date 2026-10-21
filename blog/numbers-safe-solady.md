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
