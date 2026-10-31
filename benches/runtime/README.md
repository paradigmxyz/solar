# Codegen benchmark corpus

`benchmark.py` runs the codegen benchmarks; `benchmark-compare.py` compares runs. Local contracts
live in `../../testdata/runtime/`, project archives (one per upstream project) in
`../../testdata/projects/`. Inputs live in this checkout so runs are reproducible and CI needs no
second repository or its recursive submodules.

`--mode runtime` (default) compiles each entry point's transitive import closure and skips the heavy
full-project cases. `--mode compile-time` compiles those from full archived Standard JSON inputs,
without deployment or runtime workloads. CI runs `--mode runtime compile-time`.

The [compiler-diff CLI](../../tools/compiler-diff/README.md#execution-engines) runs the suite
through the entry point it shares with Sourcify, ABI/JSON, and symbolic checks:

```sh
uv run --project tools/compiler-diff compiler-diff runtime -- \
  --solar /absolute/path/to/solar \
  --mode runtime --suite micro --tests counter --gas --start-anvil
```

It writes logs, results, and artifacts to `/tmp/solar-sourcify/<version>/engines/runtime/<id>/`, or
to `--dir` given before `runtime`. Pass engine options after `--`, with absolute paths. `--gas`
enables execution checks; `--start-anvil` starts a local node. CI and performance work call the
scripts directly.

## Runner options

`--evm-version VERSION` overrides every case's EVM target.

`-j N` (`--jobs N`) runs N cases at once when compile time does not matter. It records no compile
times or suite timings, keeps results in case order, and refuses `--gas`, whose transactions share
one sender. Compile-time mode also needs `--ignore-compile-time`, which compiles each case once and
drops compile times from any run.

`--optimizer-runs N` overrides every case's `optimizer.runs` for all compilers. We optimize for size
below 200 runs and for gas from 200, so `--optimizer-runs 1` makes a size benchmark (`--gas` then
runs on the size-optimized code).

`--gas-kinds` (needs `--gas`) traces every gas transaction with `debug_traceTransaction`, so the
node must be anvil (tested with 1.8.6); CI passes it. Each gas result
then also records:

- `execution_gas`: gas of the executed steps before the refund, without the transaction's
  intrinsic gas. Receipt gas is the calldata floor (EIP-7623) on calls with little execution, which
  hides codegen changes, so compare this field on those calls.
- `gas_kinds`: `execution_gas` split by opcode kind. Kinds sum to `execution_gas`.
- `intrinsic_gas`, `gas_refund` (after the one-fifth cap), and `calldata_floor`, true when receipt
  gas is above steps plus intrinsic gas minus refund.

[`gas_kinds_tracer.js`](gas_kinds_tracer.js) defines the kinds: `stack` (DUP, SWAP, POP), `push`,
`control` (JUMP, JUMPI, JUMPDEST), `arith`, `compare`, `bitwise`, `memory`, `storage`, `transient`,
`calldata`, `returndata`, `keccak`, `call`, `log`, `env`, `halt`, and `other`. Memory expansion
(`mem_expansion`) leaves the step that grew memory, and a precompile's gas (`precompile`) leaves the
call that ran it. Three kinds regroup steps by role: `recipe` for a pure op on constants pushed in
the same block (`PUSH0 NOT`), `guard` for a JUMPI whose branch always reverts with its target push
and condition chain, and `fmp` for free-memory-pointer loads, stores, and bumps. The call is
checked against receipt gas; a trace that cannot explain it fails the run's `gas_kinds_status`
(without `--allow-failures`, the exit code too) and leaves receipt gas as measured. Reference
compilers that run live get the same fields.

`--artifacts PATH` writes a file tree per runtime case and compiler from an extra, untimed compile.
Each compiler directory has a `sources/` tree of every embedded Standard JSON source, keeping paths,
contents, line endings, and extensionless names; source URLs are not fetched. Paths must be
relative, with no empty, `.` or `..` parts, drive prefixes, backslashes, or control characters;
symlinks and file/directory collisions are capture errors. Each compiler saves disassembly,
bytecode, and raw Standard JSON input and output. Ours adds MIR and creation and runtime EVM IR.
Solc (not `solc-legacy`) and solx add unoptimized `ir.yul` and optimized `optimized-ir.yul` when
returned (solx 0.1.8 omits `irOptimized`). Solx adds creation and runtime LLVM IR before and after
optimization (`*.unoptimized.ll`, `*.optimized.ll`). If `--reference-results` names a result beside
an `artifacts` directory, the run copies the matching reference files.

## Tempo Zones gas

`tempo-zones-portal` measures implementation creation/runtime size, implementation deployment gas,
and two encrypted deposits through a fresh ERC-1167 proxy: the first deposit and a second deposit
into an existing queue. The separate `tempo-zones-project` case retains the full Foundry test input
for compile-time measurements.

```sh
uv run --project tools/compiler-diff compiler-diff runtime -- \
  --solar /absolute/path/to/solar --solc /absolute/path/to/solc \
  --mode runtime --tests tempo-zones-portal --gas --start-anvil
```

This case requires local Anvil. Setup impersonates the public native factory address to initialize
the proxy, registers an encryption key with a proof signed by public test key 1, and installs mock
TIP-20 and TIP-403 contracts. `fixtures/runtime/Zones.json` pins both helpers' creation bytecode from
solc 0.8.37, including source hash and compiler settings, so gas uses identical mocks even without
a live reference compiler. Rebuild it with Standard JSON using `RuntimeFixtures.sol`, the saved
`settings`, and solc 0.8.37; the bytecode objects must match `creation_bytecode`.
The sender starts with 100 million token units and approves the proxy; each deposit transfers
10 million units with zero deposit fee. Each call is a separate transaction with cold access state.
Post-call checks require two deposits, 20 million units in escrow, and 80 million with the sender,
and compare queue hashes and key counts across compilers. The 64-byte ciphertext exercises portal
validation and queueing; it does not test decryption or live Tempo precompile gas. Setup and proxy
deployment are outside call gas; `deploy_gas` measures the implementation, and `proxy_deploy_gas`
records the proxy separately. Compiler settings preserve the pinned Osaka target and metadata.

## Reference compilers

By default only our compiler runs, so no solc is needed; it also builds the cold-path helper
contracts unless a live solc comparison is selected. Such a run keeps compilation, gas, and runtime
failure checks but makes no differential claims: passing runtime comparisons show as skipped unless
a matching reference result exists.

- `--solc PATH` records a two-compiler baseline.
- `--solc-legacy` (needs `--solc`) also compiles with the `--solc` binary and `viaIR: false`, as
  compiler `solc-legacy`, so one run covers solc's legacy and via-IR pipelines. Every other setting
  matches `solc`. Cases that only compile with via-IR (stack too deep) fail as an ordinary reference
  compiler failure: no report warning, but the run exits 1 unless `--allow-failures` is set.
- `--solar-size` adds compiler `solar-size`: the same Solar binary with `optimizer.runs` forced to
  1, so a run holds our `-Ogas` (`solar`, at the case's own runs, normally 200) and `-Osize`
  results. It conflicts with `--optimizer-runs`, which would pin both to one objective. Its
  failures and mismatches are ours: they raise warnings like `solar`'s.
- `--solx PATH` adds [solx](https://github.com/NomicFoundation/solx) as a separate compiler with its
  own compilation, gas, runtime checks, and artifacts.
- `--oksolc PATH` adds [oksolc](https://github.com/okcontract/oksolc) the same way, run as
  `standard-json --no-cache --parallel --jobs 8 -`: eight workers match our default, and no cache
  keeps repeated samples measuring compilation. `--oksolc-jobs N` sets the workers. Inputs keep
  their settings; unsupported inputs stay failures.
- `--reference-results PATH` reuses solc, solc-legacy, solx, and oksolc results from a prior run
  without running them; it conflicts with `--solc` and `--solx`. It copies compile, gas, and runtime
  data only for matching input fingerprints, each compiler's own (`solc-legacy` has `viaIR: false`
  in its input, so its fingerprint differs from `solc`'s), then runs the usual cross-compiler
  checks. Live `--oksolc` results override saved ones.

Reference compiler failures stay in the raw results but raise no report warnings or PR comments;
failures of our compiler (`solar`, `solar-size`), and mismatches involving it, raise warnings, and
only `solar` decides whether a baseline comparison comments. Single-run reports show `solc-legacy`
and `solar-size` as columns beside solc, with every delta measured against `solar`. The
compile-time sums count only cases that every shown compiler compiled, so cases that `solc-legacy`
fails drop out of all columns.

In CI, PRs pass the exact-base result as `--reference-results`, so solc runs only on the base
revision. Solx 0.1.8 is pinned and runs only on pushes to main, never in PR jobs, even ones that
rebuild a missing baseline; its columns appear when the downloaded main artifact has matching
results. Oksolc runs on main, PRs, and manual runs; PRs add `--oksolc` to `--reference-results` so
they include oksolc before a main baseline has it. CI builds oksolc v0.2.2, revision
`c465a262edc835afff32951ee2d64e827bc16c3d`, with Zig 0.17.0, ReleaseFast, for the baseline CPU so the
cached binary runs on any runner of that architecture. The upstream build disables the browser UI by
default, so it needs no source patch, TypeScript, or Bun. The cache key covers revision, Zig
version, OS, architecture, and build settings; a hit skips the shallow checkout and build. Solx and
oksolc appear beside solc in the Markdown report. The website reads compiler columns and artifact
choices from the uploaded results, so oksolc shows in the overview and the PR comment's artifact
links even when the base run lacks it.

CI also runs `--solc-legacy` with every live solc run (the exact-base run and main) and
`--solar-size` on every run, so each PR compares Solar `-Ogas`, Solar `-Osize`, solc via-IR, and solc
legacy. A baseline artifact without `solc-legacy` results counts as incomplete, so PRs rebuild it
until main has one. The PR comment ends with one collapsible table per metric (runtime gas open, the
rest a click away), listing every benchmark with each compiler's value and difference from `solar`,
and a geometric-mean row.

## Comparing runs

`benchmark-compare.py` also produces CI's Markdown report, common benchmark JSON, job summary, and
comment metadata:

```bash
uv run benches/runtime/benchmark-compare.py \
  target/codegen-bench/baseline target/codegen-bench/candidate \
  --report-output target/codegen-bench/comparison.md \
  --json-output target/codegen-bench/comparison.json \
  --diff-output target/codegen-bench/changes.patch
```

- Inputs are JSON paths or directories holding `results.json`. Artifacts default to `artifacts/`
  beside each JSON; override with `--baseline-artifacts` and `--artifacts`.
- Markdown goes to stdout; `--report-output` also saves it. `--json-output` keeps exact values,
  compile samples, comparison exclusions, and artifact paths and hashes. `--diff-output` writes
  unified artifact diffs.
- `--tests NAME...` selects cases. `--artifact KIND...` limits artifact kinds to any of `mir`,
  `llvm-ir`, `evm-ir`, `disasm`, `bytecode`, and `json` (default: all).
- `--compiler solar-size|solc|solc-legacy|solx|oksolc` compares that compiler instead of ours, needs
  a baseline, and omits the compiler-primary CI tables. Use `--compiler solx --artifact llvm-ir` for
  solx LLVM IR.
- `--results PATH` alone makes a single-run CI report, the only kind with reference compiler tables.
  With a baseline, the detailed report shows only changed benchmarks, per-call gas changes, gas by
  opcode kind, and artifact details.
- `--common-output` writes the shared CI schema; it needs a complete, unfiltered run of our
  compiler.
- CI's `--pr-comment-output` writes a compact PR comment: gas and size overview, benchmarks changed
  against the base branch, and a button to the web overview. Neither overview shows comparison
  counts; the detailed report stays in the job summary and artifacts. `--comment-output` writes the
  should-comment flag.
- Numeric regressions exit zero; missing or invalid result inputs do not.

The report lists missing and failed cases, artifact capture errors, and runtime observation changes,
and leaves incompatible inputs or workloads out of deltas. The summary is the geometric mean of
candidate/baseline ratios, per metric (gas, size, time, RSS), with each benchmark weighted equally
regardless of contract size. Zero-valued pairs count in per-case results and change counts, not the
mean. Runtime gas sums comparable transactions within, not across, benchmarks. Artifact hashes and
file additions and removals expose changed bytecode of equal size. Missing artifacts show as
unavailable; whole-project cases capture none.

Runs recorded with `--gas-kinds` add execution gas beside runtime gas: a summary and per-case row
that sum the same comparable calls, a column after the gas column of the single-run table, and
execution columns in the per-call table, where † marks receipt gas billed at the calldata floor. A
collapsed table lists each opcode kind's execution gas delta, for all compared cases and for each
changed case. Older results lack the fields; their reports omit these columns and tables.

Compile time and RSS comparisons need matching compiler labels and known build profiles, but labels
do not prove comparable builds: review machine differences and timing noise, and note that building
test targets can unify extra dependency features under the same debug profile. Use matching Cargo
targets and features, record the build command, and freeze the baseline binary before building other
targets.

Calls with `comparison_exclusion_reason` still run and keep raw gas and failures in `results.json`;
reports sum only calls eligible on both sides and list excluded gas and reasons apart. LibString's
memory stress uses a fixed workload, so its calls are eligible too. Older
baselines use the candidate's exclusions; two old reports without this metadata keep their totals.

## Comparing local builds

Compare our base and candidate builds locally; do not install or run solc/solx unless asked. Record
the baseline before editing and reuse it while the commit, toolchain, flags, and corpus match: keep
its `results.json` and artifacts frozen, pass its directory to `benchmark-compare.py`, and run only
the candidate. While iterating, run affected cases with one compile sample; replace
`counter factorial` below with their IDs. Execution uses Foundry's `cast` and `anvil`.

```bash
bench_run() {
  cargo build -p solar-compiler --bin solar &&
  mkdir -p "$1/debug" &&
  cp target/debug/solar "$1/debug/solar" &&
  /usr/bin/time -p -o "$1/time.txt" \
    uv run benches/runtime/benchmark.py \
    --solar "$1/debug/solar" \
    --mode runtime --suite all --tests counter factorial --compile-repeats 1 \
    --gas --gas-profile hot --gas-kinds --start-anvil \
    --output "$1/results.json"
}
bench_run target/codegen-bench/baseline

# Make the code change, then continue.
bench_run target/codegen-bench/candidate
uv run benches/runtime/benchmark-compare.py \
  target/codegen-bench/baseline target/codegen-bench/candidate \
  --report-output target/codegen-bench/comparison.md \
  --json-output target/codegen-bench/comparison.json
```

Read failures, missing cases, and excluded comparisons first. Check per-case size and gas, including
per-call deltas and the execution-gas kind table; aggregate wins must not hide regressions or
missing results. For changed gas, size, behavior, or output fingerprints, capture artifacts
(`--artifacts`) from both saved builds and diff MIR (`mir.mir`), EVM IR (`creation.evmir`,
`runtime.evmir`), disassembly, and bytecode with
`--diff-output target/codegen-bench/changes.patch`; equal byte counts do not prove equal bytecode.
Narrow with `--tests NAME...` or `--artifact mir evm-ir`. Whole-project cases measure compilation
only; `testdata/projects/README.md` and [Workloads](#workloads) pin their inputs and upstream
commits.

To check that a change leaves compiler output identical across the whole corpus, make compile-only
runs of both builds with release binaries and no `--gas`, using
`--mode runtime compile-time --suite all -j 8 --ignore-compile-time`. The comparison
lists each case whose Standard JSON output, bytecode included, differs as `compiler output
fingerprint changed` under "Artifact changes and availability"; no such rows means identical
output. When a comparison ignores compile time, gas and size runs (`--optimizer-runs 1`) can run at
the same time, each with `--start-anvil` and its own `--rpc-url` port, such as
`http://127.0.0.1:8546`.

Keep baseline binaries, results, and artifacts immutable; use fresh candidate directories, debug
builds, and the existing target directory. Save evidence outside directories due for cleanup, then
remove requested temporary worktrees with `git worktree remove`; never clean another task's files.
Measure compiler time separately, with repeated samples, matching build profiles, and no concurrent
benchmarks or heavy builds. One sample does not prove a compiler-speed change; repeat only affected
cases and suspected regressions.

Once a codegen change settles, run both corpora once: UI fixtures for size and `-Osize` coverage,
and the full runtime and project corpus with
`--mode runtime compile-time --suite all --compile-repeats 1` and no `--tests`. Reuse the matching
baseline; after fixes repeat affected checks, and broaden only when changes or failures invalidate
earlier coverage. When tuning a pipeline, move or remove one pass group at a time, record its order
and both corpora's results under `target/codegen-bench/`, and keep IR snapshots canonical.
`-Ztime-passes` on a large contract finds repeated passes that still change IR; one `changed=false`
result does not prove a pass redundant.

## Workloads

Workloads and helper fixtures, including the `../../testdata/Arithmetic.sol`, `Factorial.sol`, and
`SumArray.sol` micro contracts, come from
[`walnuthq/solidity-compiler-benchmarks`](https://github.com/walnuthq/solidity-compiler-benchmarks)
at `01209d2b8ac81645b92e3ef801b5bcdfd61bfd69`. The combined profile still holds each contract from
both compilers, both deployed artifacts, the same ordered transactions, and matching normalized
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
| `tempo-zones-portal` | `tempoxyz/zones` plus `tempoxyz/tempo-std` | `700069eb8a40fce7e1f7a4bf0fc5703bcc692c43`, `96f882963ce3b0e0abc7ec2b41d03d42457877bb` | 23 |
| `solady-encoding` | `Vectorized/solady` plus `Encoding.sol` | `solady-0.1.26` archive | 3 |
| `solady-algorithms` | `Vectorized/solady` plus `Algorithms.sol` | `solady-0.1.26` archive | 5 |
| `tempo-zones-project` (compile-time only) | `tempoxyz/zones` | `700069eb8a40fce7e1f7a4bf0fc5703bcc692c43` | 67 |

File counts are each case's sliced closure. Archives below are in `../../testdata/projects/`. The
OpenZeppelin cases share `openzeppelin-5.6.1.json.gz`, which replaces the earlier extracted
OpenZeppelin runtime archive; `lilweb3-flashloan` and `lilweb3-fractional` share
`lilweb3-runtime.json.gz`; `aave-l2-encoder.json.gz` embeds the Aave harness. The large OpenZeppelin
and Solady cases, like the normal benchmark suite, read the pinned archives there. `counter` reuses
the normal suite's `../../testdata/Counter.sol`. `fixtures/runtime/RuntimeFixtures.sol` holds local
Apache-2.0 helpers with the interfaces the cold-path workloads use. Embedded sources keep their SPDX
identifiers.

`counter-loop` runs shared checked-arithmetic helpers in a storage-backed accumulator loop, timing
10, 100, and 1,000 iterations as separate calls after the increment/subtract setup of the
Solidity/Solcore comparison. CI runs it in `--suite all`, including the `hot` gas profile.

`openzeppelin-p256` calls the pinned library's software verifier directly, bypassing the native
P256 precompile. It measures a valid signature, a wrong hash, a zero signature component, and an
off-curve public key, using the library's RIP-7212 probe vector.

`minimal-proxy` (`../../testdata/MinimalProxy.sol`) has a payable high-level fallback that delegates
to an immutable implementation its constructor deploys, with assembly only to forward revert data.
Both gas profiles measure storage writes, reads, and empty, short, and 1 KiB byte echoes through the
proxy; runtime checks compare the stored value and returned bytes across compilers. Runtime size
covers the proxy alone; creation gas and size include the implementation.

`openzeppelin-governor` measures proposals of zero, one, two, and eight elements. Nonempty ones
exercise address cleanup, word-array copies, and nested bytes tails of 0, 1, 31, 32, and 33 bytes.
Each checks the returned proposal hash against solc. Keep per-call results visible: empty proposals
skip encoder loops, and transaction gas floors can hide execution-cost changes on calldata-heavy
inputs; with `--gas-kinds`, read their execution gas.

`solady-lib-string` adds byte-to-hex conversion, every single-byte ASCII input, and rune counting
for empty, one-byte, 31/32/33-byte ASCII, and longer multibyte UTF-8 strings. These pinned upstream
tests assert their own results, including against reference implementations, and expose loop and
bounds-check costs the original six-function profile missed; keep them beside the replacement and
conversion calls. Byte conversion covers empty input and lengths 1, 31, 32, 33, 64, and 65 in both
prefix modes; ASCII differential calls put high-bit bytes at the first byte, a word boundary, and
the last byte. These checks dirty nearby memory and verify the helper restores its temporary writes,
so their gas includes memory stress and assertion setup, not the encoder alone. For this case only,
the runner replaces the pinned `_brutalizeMemory` test helper with
[`FairBrutalizer.sol`](../../testdata/runtime/FairBrutalizer.sol). It dirties scratch space and sixteen
words at the free memory pointer with a fixed, nonuniform pattern, without changing the pointer or
the zero slot. This avoids the upstream helper's gas-dependent offsets and code-size-dependent
copies. Every compiler receives the same patched source; the library and its assertions stay
unchanged, and the runner rejects an unexpected upstream helper body. Whole-project Solady builds
and the other runtime cases keep their original sources. Rebuild baselines from before this patch:
their input fingerprints differ and are not comparable. Keep these per-call results beside the
fixed hex and exhaustive single-byte workloads, so aggregates cannot hide the original gaps.

`solady-algorithms` uses [`Algorithms.sol`](../../testdata/runtime/Algorithms.sol) with unchanged
pinned LibString, LibSort, and Base64. Its 85 calls cover unsigned decimal digit boundaries, signed
limits, insertion and quicksort lengths around their cutoff, sorted/reversed/equal/nonuniform
arrays, and padded Base64 tails. It checks return values apart from gas; the wrappers have no
assertions or gas-dependent memory stress.

`solady-encoding` uses [`Encoding.sol`](../../testdata/runtime/Encoding.sol) with the same Solady
and plain ABI calls. It measures both hex prefix modes and ASCII classification over nonuniform
inputs of 0, 1, 15, 16, 31, 32, 33, 63, 64, 65, and 256 bytes, with high-bit bytes at the start, a
word boundary, and the end; more boundaries reach 1024 bytes, including long all-ASCII scans and
high-bit bytes near both ends. All 153 calls check returned values against solc. It isolates
encoding and ABI costs from upstream assertions and memory brutalization; report both it and
`solady-lib-string`, since they exercise different behavior.

These local workloads in `../../testdata/runtime/` target specific optimizations. Report each apart
from the pinned project corpus; they show targeted gains, not general superiority over solc.

- `verified-words` (`VerifiedWords.sol`): the Lean-proved word rules, as mixed bitwise expressions
  and signed negation in hot loops, with edge-value return checks.
- `compiler-optimizations` (`CompilerOptimizations.sol`): aggregate SSA across branches and loops,
  joined bounds, shared constant-argument helpers, packed storage updates, and overwrites on both
  branch arms. It measures fresh and repeated writes and checks returned values and final storage
  against solc. It is a focused regression workload; keep the project corpus comparison when judging
  its gains.
- `word-recipes` (`WordRecipes.sol`): mixed arithmetic, common-mask factoring, packed-byte
  extraction, and a deployment/runtime tradeoff for a large comparison constant. Compare both
  optimization objectives.
- `seeded-words` (`SeededWords.sol`): mixed bitwise subtraction, complemented arithmetic, and mask
  absorption found in the offline seed trees, checking zero iterations and wrapping inputs as well
  as hot loops.
- `division-words` (`DivisionWords.sol`): checked products, quotients, and remainders in hot loops
  for fixed-point scaling, fee routing and deduction, kept shares, lot thresholds and whole lots,
  week, period, and day rounding, and nested unit conversion. Return checks include the scaled
  products' overflow limits and maximal timestamps.
- `calldata-loops` (`CalldataLoops.sol`): loops over calldata arrays as batch entry points read
  them: totals, products by a fixed price, maxima, address searches, and dot products. Each call
  repeats its pass so execution, not the EIP-7623 calldata floor, sets the gas used. Return checks
  include empty arrays, zero rounds, and the exact product limit.

## Reproducing oksolc via-IR failures

These steps recheck the three full-project inputs in [oksolc issue
#8](https://github.com/okcontract/oksolc/issues/8) with revision
`d5ff7399356b1ad238c839278a334e13e0e2dc81` on Ubuntu 24.04 x86-64, starting from [Solar PR
#1601](https://github.com/paradigmxyz/solar/pull/1601). Put Zig 0.16.0 and uv on `PATH`; peak RSS
needs GNU `/usr/bin/time`. No Solar build, Foundry, or JavaScript tools are needed. Build the same
compiler-only binary as CI:

```sh
git clone --depth 1 --branch dani/bench-oksolc https://github.com/paradigmxyz/solar.git solar-oksolc-repro
cd solar-oksolc-repro
mkdir -p target/oksolc-repro/source
git -C target/oksolc-repro/source init
git -C target/oksolc-repro/source fetch --depth 1 https://github.com/okcontract/oksolc.git d5ff7399356b1ad238c839278a334e13e0e2dc81
git -C target/oksolc-repro/source checkout --detach FETCH_HEAD
zig build --build-file target/oksolc-repro/source/build.zig build-cli \
  -Doptimize=ReleaseFast -Dcpu=baseline -j4 \
  --prefix "$PWD/target/oksolc-repro/install"
```

Generate inputs from the branch's project archives, changing only `settings.viaIR` to `true`;
optimizer settings, EVM targets, sources, and output selections stay as they are:

```sh
uv run --no-project --python "$(cat .python-version)" python - <<'PY'
import hashlib
import json
import sys
from pathlib import Path
sys.path.insert(0, "benches/runtime")
import benchmark

selected = {"seaport-1.6-project", "solady-0.1.26-project", "openzeppelin-5.6.1-project"}
root = Path("target/oksolc-repro/inputs")
root.mkdir(parents=True, exist_ok=True)
for case in benchmark.TEST_CASES:
    if case.test_id not in selected:
        continue
    text, _, _ = benchmark.compiler_input(case, None)
    payload = json.loads(text)
    payload["settings"]["viaIR"] = True
    text = json.dumps(payload)
    (root / f"{case.test_id}.json").write_text(text)
    print(case.test_id, hashlib.sha256(text.encode()).hexdigest())
PY
```

Run each request directly, with the persistent cache off and eight workers. Inputs embed their
sources, so no import checkout is needed:

```sh
mkdir -p target/oksolc-repro/outputs
for case in seaport-1.6-project solady-0.1.26-project openzeppelin-5.6.1-project; do
  status=0
  /usr/bin/time -v -o "target/oksolc-repro/outputs/$case.time.txt" \
    target/oksolc-repro/install/bin/oksolc standard-json --no-cache --parallel --jobs 8 - \
    < "target/oksolc-repro/inputs/$case.json" \
    > "target/oksolc-repro/outputs/$case.json" \
    2> "target/oksolc-repro/outputs/$case.stderr" || status=$?
  printf '%s: exit status %s\n' "$case" "$status"
done
```

Check stderr and the output JSON's `errors` array; exit status zero does not mean success.

| Input | Sources | EVM target | Optimizer runs | Observed result |
| --- | ---: | --- | ---: | --- |
| `seaport-1.6-project` | 386 | london | 4294967295 | Yul stack-depth error for `var_parameters_offset` |
| `solady-0.1.26-project` | 208 | paris | 1000 | Compiles successfully |
| `openzeppelin-5.6.1-project` | 390 | osaka | 200 | `Yul optimizer failed: MissingReference` |

Expected input SHA-256 hashes:

```text
seaport-1.6-project       a0ab0392e351b34cdcf06a9b981e556c41124119dc0333830ee317c23fa5c5c5
solady-0.1.26-project     c6f7fda591cc00d880fcd21e18da99b4fab4bcd2fc9309cea78b853b62a2f53f
openzeppelin-5.6.1-project 666aacb77d7fdf356de4fcabc93a230e6daccb642e1857b41c8c727ee2cf33d6
```

These reproduce whole projects; they are not reduced cases. On 2026-10-01 we retested all 32
CI-selected cases with this revision, using oksolc alone with no output comparison. With original
settings, 22 compiled; ten needed via-IR, as did two runtime helpers. With `viaIR: true` on every
input and helper, 30 compiled and all 23 runtime cases passed their execution checks and hot gas
calls. CI keeps each case's original settings.

[Upstream PR #12](https://github.com/okcontract/oksolc/pull/12) fixes Solady's internal failure.
OpenZeppelin now returns a Yul diagnostic instead of exiting 137, with 2,654,486,528 bytes peak RSS
in this run. The upstream maintainer reports that Seaport gives the same diagnostic with solc; see
[issue #8](https://github.com/okcontract/oksolc/issues/8#issuecomment-5902965134).
