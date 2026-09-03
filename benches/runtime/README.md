# Codegen benchmark corpus

This directory contains the codegen benchmark runner and workload documentation. Local
benchmark contracts live in `../../testdata/runtime/`. The shared
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

Pass `--optimizer-runs N` to replace every case's `optimizer.runs`. We optimize for size below
200 runs and for gas from 200 up, so `--optimizer-runs 1` turns the same corpus into a size
benchmark. The override applies to every selected compiler, and runtime checks and gas calls
still run against the size-optimized code when `--gas` is given.

The default runs only our compiler. Pass `--solc PATH` to record a two-compiler baseline.
Pass `--solx PATH` to include [solx](https://github.com/NomicFoundation/solx) as a separate compiler,
with its own compilation, gas, runtime checks, and artifacts. CI pins solx 0.1.8 and installs and
runs it only on pushes to main. Its measurements appear alongside solc in the Markdown report.
Reference compiler failures remain in the raw results but do not produce report warnings or
trigger PR comments. Failures from our compiler and result mismatches involving it still do.

Use `--solar-only` to skip solc and solx benchmark compilation even when `--solc PATH` supplies a binary
for reference validation or helper contracts. A one-compiler run retains compilation, gas
measurements, and runtime failure checks, but cannot make differential runtime claims, so
successful runtime comparisons are marked as skipped unless a matching reference result is supplied.

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
CI uses `--pr-comment-output` for a compact PR comment with a gas and size overview,
changed benchmarks compared with the base branch, and a button to open the web overview.
Neither overview includes comparison counts. The detailed report stays in the job summary
and artifacts. `--comment-output` writes the separate should-comment flag.
With a baseline, the detailed report shows only changed benchmarks against that baseline,
plus per-call gas changes and artifact details. Reference compiler tables appear only in
single-run reports.

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
| `solady-encoding` | `Vectorized/solady` plus `Encoding.sol` | `solady-0.1.26` archive | 3 |
| `solady-algorithms` | `Vectorized/solady` plus `Algorithms.sol` | `solady-0.1.26` archive | 5 |

The OpenZeppelin cases share the canonical
`../../testdata/projects/openzeppelin-5.6.1.json.gz` archive; the file counts above are the sliced
closure for each case. This replaces the extracted OpenZeppelin
runtime archive used by earlier versions of the benchmark.

The Lil Web3 cases share `../../testdata/projects/lilweb3-runtime.json.gz`; the file counts above
are the sliced closure for each case.

The large OpenZeppelin and Solady cases use the pinned archives in `../../testdata/projects/`. The
normal benchmark suite also reads those archives from there.

Governor measures empty proposals and proposals with one, two, and eight elements.
The nonempty cases exercise address cleanup, word-array copies, and nested bytes
tails of 0, 1, 31, 32, and 33 bytes. Each workload also checks the returned proposal
hash against solc. Keep per-call results visible: empty proposals do not exercise
encoder loops, and transaction gas floors can hide execution-cost changes on
inputs with substantial calldata.

LibString also exercises byte-to-hex conversion, every possible single-byte ASCII
input, and rune counting for empty, one-byte, 31/32/33-byte ASCII, and longer
multibyte UTF-8 strings. These pinned upstream tests assert their own results,
including comparison against reference implementations. Keep these workloads
alongside replacement and conversion calls: they expose loop and bounds-check
costs that the original six-function profile missed.

The byte conversion workloads also cover empty input and lengths 1, 31, 32, 33,
64, and 65 with both prefix modes. ASCII differential calls exercise high-bit
bytes at the first byte, a word boundary, and the final byte. These upstream
checks dirty surrounding memory and verify that the helper restores its temporary
writes. Their gas includes the upstream memory-brutalization and assertion setup,
so it does not measure the encoder in isolation. The brutalizer seeds a pseudorandom
memory offset from `gas()` and also copies `codesize()` bytes; changing generated code
can select a more expensive stress path even when the library itself gets cheaper.
Keep those per-call changes visible instead of treating their sum as isolated library
cost. They are separate from the fixed
hex and exhaustive single-byte workloads, so aggregate results cannot hide the
original gaps.

`solady-algorithms` uses [`Algorithms.sol`](../../testdata/runtime/Algorithms.sol) with unchanged pinned
LibString, LibSort and Base64 sources. Its 85 calls cover unsigned decimal digit
boundaries, signed limits, insertion and quicksort lengths around their cutoff,
sorted/reversed/equal/nonuniform arrays, and padded Base64 tails. Return values
are checked separately from gas measurements; the wrappers contain no assertions
or gas-dependent memory stress.

`solady-encoding` uses [`Encoding.sol`](../../testdata/runtime/Encoding.sol) with the same pinned Solady
library and ordinary ABI calls. It measures both hex prefix modes and ASCII
classification over nonuniform inputs of 0, 1, 15, 16, 31, 32, 33, 63, 64, 65,
and 256 bytes, plus high-bit bytes at the beginning, word boundary, and end.
Additional boundaries extend through 1024 bytes, including long all-ASCII scans
and high-bit bytes near both ends. All 153 calls compare their returned values
against solc. This isolates encoding
and ABI costs from upstream assertions and memory brutalization; keep both cases
in reports, since they exercise different behavior.

The three additional micro contracts (`../../testdata/Arithmetic.sol`,
`../../testdata/Factorial.sol`, and `../../testdata/SumArray.sol`) came from the benchmark repository at the commit above. The
runtime suite reuses the existing `../../testdata/Counter.sol` source from the normal benchmark
suite. The Aave harness is embedded in `../../testdata/projects/aave-l2-encoder.json.gz`.
`fixtures/runtime/RuntimeFixtures.sol` provides local Apache-2.0 helpers with the same interfaces
used by the cold-path workloads. Embedded Solidity sources retain their SPDX identifiers.

`verified-words` is a synthetic workload in `../../testdata/runtime/VerifiedWords.sol`
for the SMT-checked word rules. It measures mixed bitwise expressions and signed
negation in hot loops, with edge-value return checks. Report its results
separately from the pinned project corpus; it demonstrates targeted reductions,
not a general advantage over solc.

`compiler-optimizations` uses the local
`../../testdata/runtime/CompilerOptimizations.sol` workload to exercise aggregate SSA
across branches and loops, joined bounds, shared constant-argument helpers,
packed storage updates, and overwrites on both branch arms. It measures both
fresh and repeated writes and checks returned values and final storage against
solc. It is a focused regression workload; retain the project corpus comparison
when evaluating its improvements.

`word-recipes` uses `../../testdata/runtime/WordRecipes.sol` to measure mixed arithmetic,
common-mask factoring, packed-byte extraction and a deployment/runtime tradeoff
for a large comparison constant. Keep its targeted hot-loop results separate from
the pinned projects and compare both optimization objectives.

`seeded-words` uses `../../testdata/runtime/SeededWords.sol` for mixed bitwise
subtraction, complemented arithmetic and mask absorption discovered from the
offline seed trees. It checks zero iterations and
wrapping inputs as well as hot loops. These targeted results are separate from
the pinned project corpus and do not establish general superiority over solc.
