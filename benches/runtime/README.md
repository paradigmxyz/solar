# Codegen benchmark corpus

This directory contains fixtures and workload documentation for the codegen benchmark. The shared
project archives live in `../../testdata/projects/`; archives group cases from the same upstream
project. The default `runtime` mode selects each entrypoint's transitive Solidity import closure and
omits the heavy full-project cases. The `compile-time` mode measures those cases by passing full
archived Standard JSON inputs to each compiler without deployment or runtime workloads. CI runs
both modes with `--mode runtime compile-time`.

Keeping the inputs here makes the benchmark reproducible from this checkout and removes the CI
dependency on a second repository and its recursive submodules.

Pass `--evm-version VERSION` to replace every archived Standard JSON target and benchmark a whole
corpus against one EVM version. Use `--solar-only` when the selected target is not supported by the
installed solc. When available, solc still provides helper contracts for cold-path runtime checks.

The default runs only our compiler. Pass `--solc PATH` to record a two-compiler baseline.
Pass `--solx PATH` to include [solx](https://github.com/NomicFoundation/solx) as a separate compiler,
with its own compilation, gas, runtime checks, and artifacts. CI pins solx 0.1.8 and installs and
runs it only on pushes to main. Its measurements appear alongside solc in the Markdown report.
Reference compiler failures remain in the raw results but do not produce report warnings or
trigger PR comments. Failures from our compiler and result mismatches involving it still do.

Use `--solar-only` to skip solc and solx benchmark compilation even when `--solc PATH` supplies a binary
for reference validation or helper contracts. The default skips the
reference solc compile for each case while retaining Solar compilation, gas measurements, and
runtime failure checks. A one-compiler run cannot make differential runtime claims, so successful
runtime comparisons are marked as skipped unless a matching reference result is supplied.

Pass `--reference-results PATH` to reuse matching solc and solx results from a prior
run. The benchmark copies reference compile, gas, and runtime data only when the input fingerprint
matches, then performs the normal cross-compiler runtime checks. PR CI uses the exact-base result
as the reference, so solc runs on the base revision instead of repeating unchanged work on the PR.
PR jobs never run solx, including when they must rebuild a missing baseline; solx columns appear
when matching results are available in the downloaded main artifact.

Pass `--artifacts PATH` to write a file tree for each runtime case and compiler. This extra compile
runs outside the timed samples. Solar emits MIR, creation and runtime EVM IR, disassembly, bytecode,
and raw Standard JSON input and output. Solc emits unoptimized `ir.yul` and optimized
`optimized-ir.yul` where available, disassembly,
bytecode, and raw Standard JSON input and output. When `--reference-results` points to a result next
to an `artifacts` directory, the matching reference files are copied into the new run. Solx artifacts
use the same output requests as solc, saving `ir.yul` and `optimized-ir.yul` when returned,
alongside disassembly, bytecode, and raw Standard JSON input and output. Solx 0.1.8 returns
`ir` but omits `irOptimized`.

Compare two runs with `benchmark-compare.py`, which also generates CI's Markdown report,
common benchmark JSON, job summary, and comment metadata:

```bash
uv run benches/runtime/benchmark-compare.py \
  target/codegen-bench/baseline target/codegen-bench/candidate \
  --report-output target/codegen-bench/comparison.md \
  --json-output target/codegen-bench/comparison.json \
  --diff-output target/codegen-bench/changes.patch
```

The script prints Markdown to stdout by default. `--report-output` also saves the same
report; omit it when you only need terminal output.
CI uses `--pr-comment-output` for a compact PR comment with gas and size changes and
a button to open the benchmark overview. The detailed report stays in the job summary
and artifacts. `--comment-output` writes the separate should-comment flag.

Inputs may be directories containing `results.json` or JSON paths. Artifacts default to
`artifacts/` beside each JSON. Use `--baseline-artifacts` and `--artifacts` for other paths.
Add `--tests factorial counter` to select cases, `--artifact mir` for MIR diffs, or
`--artifact evm-ir disasm bytecode` for backend output. `--compiler solc` or `--compiler solx`
inspects that reference compiler;
the default compares our compiler between runs. `--results PATH` without a baseline produces
a single-run CI report. Numeric regressions do not cause a nonzero exit status;
missing or invalid result inputs do. The shared CI schema (`--common-output`)
requires a complete, unfiltered run. `--compiler solc` requires two runs and shows
the solc comparison without the compiler-primary CI tables.

The comparison reports missing/failed cases and excludes incompatible inputs or runtime
workloads from deltas. The summary uses the geometric mean of candidate/baseline ratios,
with equal weight per benchmark, separately for each metric. Zero-valued pairs stay in
the per-case results and change counts but do not enter the mean. Runtime gas sums the
measured transactions within each benchmark, not across benchmarks. It includes per-call gas changes,
compile samples in JSON, artifact hashes, and file additions/removals, so equal bytecode sizes
do not hide changed bytecode. Missing artifacts are reported as unavailable, including the
whole-project cases that do not capture them. Compile time and RSS comparisons require matching
compiler labels and known build profiles; machine differences and timing noise still need review.
Artifact capture errors and runtime observation changes appear in the comparison's issues.

The workload definitions and helper fixtures were imported from
[`walnuthq/solidity-compiler-benchmarks`](https://github.com/walnuthq/solidity-compiler-benchmarks)
at commit `01209d2b8ac81645b92e3ef801b5bcdfd61bfd69`. The combined profile still contains each contract
from both compilers, both deployed artifacts, the same ordered transactions, and matching normalized
runtime observations.

| Case | Upstream source | Revision | Files |
| --- | --- | --- | ---: |
| `uniswap-v2-pair` | `Uniswap/v2-core` | `ee547b17853e71ed4e0101ccfd52e70d5acded58` | 10 |
| `openzeppelin-erc20-mock` | `OpenZeppelin/openzeppelin-contracts` | `openzeppelin-5.6.1` archive | 6 |
| `openzeppelin-vesting-wallet` | `OpenZeppelin/openzeppelin-contracts` | `openzeppelin-5.6.1` archive | 12 |
| `nitro-one-step-proof` | `OffchainLabs/nitro-contracts` | `0b8c04e8f5f66fe6678a4f53aa15f23da417260e` | 22 |
| `aave-l2-encoder` | `aave/aave-v3-core` | `782f51917056a53a2c228701058a6c3fb233684a` | 6 |
| `lilweb3-ens` | `m1guelpf/lil-web3` | `7346bd28c2586da3b07102d5290175a276949b15` | 1 |
| `lilweb3-flashloan` | `m1guelpf/lil-web3` plus `transmissions11/solmate` | `7346bd28c2586da3b07102d5290175a276949b15`, `e802bcf2fb24dda2bf7e513bea86d15c48b57486` | 2 |
| `lilweb3-fractional` | `m1guelpf/lil-web3` plus `transmissions11/solmate` | `7346bd28c2586da3b07102d5290175a276949b15`, `e802bcf2fb24dda2bf7e513bea86d15c48b57486` | 3 |
| `maple-erc20` | `maple-labs/erc20` | `baf791a9f894b0b319a2d42d5b9f8d30349ebaad` | 2 |

The OpenZeppelin cases share the canonical
`../../testdata/projects/openzeppelin-5.6.1.json.gz` archive; the file counts above are the sliced
closure for each case. This replaces the extracted OpenZeppelin
runtime archive used by earlier versions of the benchmark.

The Lil Web3 cases share `../../testdata/projects/lilweb3-runtime.json.gz`; the file counts above
are the sliced closure for each case.

The large OpenZeppelin and Solady cases use the pinned archives in `../../testdata/projects/`. The
normal benchmark suite also reads those archives from there.

The three additional micro contracts (`../../testdata/Arithmetic.sol`,
`../../testdata/Factorial.sol`, and `../../testdata/SumArray.sol`) came from the benchmark repository at the commit above. The
runtime suite reuses the existing `../../testdata/Counter.sol` source from the normal benchmark
suite. The Aave harness is embedded in `../../testdata/projects/aave-l2-encoder.json.gz`.
`fixtures/runtime/RuntimeFixtures.sol` provides local Apache-2.0 helpers with the same interfaces
used by the cold-path workloads. Embedded Solidity sources retain their SPDX identifiers.
