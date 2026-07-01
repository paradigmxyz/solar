# Fandango Fuzzing

This directory contains bounded Fandango-based and Foundry-based differential
tests. Fandango and SolSmith generate ABI values and Solidity programs; Foundry
then fuzzes selected generated programs with cheatcodes for logs and state
diffs.

- `abi-values.fan` generates ABI values for `AbiVectorFixture.sol`. The runner
  compiles the fixture with solc and this compiler, installs both runtimes into
  anvil, then compares exact runtime behavior.
- `solidity-source.fan` generates complete Solidity source files. The source
  runner compiles each generated `.sol` file with solc and this compiler and
  records compile-result disagreements.
- `solidity-runtime-source.fan` generates complete Solidity programs with a
  fixed `setup/run/observe` harness. The runtime runner fuzzes deterministic
  input sequences through that harness and compares side effects.
- `runtime-corpus/` contains seed programs for the runtime-source grammar.
  Fandango uses these as an initial population, then mutates/crosses them with
  grammar-generated programs. Keep this corpus focused on small bug-shaped
  runtime harnesses.
- `solsmith.py` is the first typed generator layer. It emits the same runtime
  harness from type-aware statement builders and records feature metadata for
  each generated source. Prefer the `fuzz/bin/solsmith` wrapper for local use.
- `reduce_runtime_failure.py` is the runtime reducer behind the
  `fuzz/bin/solreduce` wrapper. It shrinks replayable source-runtime failures
  while preserving the original oracle mismatch.
- `write_foundry_target.py` and `run_foundry_target.py` bridge generated
  Fandango/SolSmith harnesses into Foundry. The generated target installs solc
  and compiler runtimes with `vm.etch`, lets Foundry's builtin fuzzer drive the
  calls, then compares returndata, logs, and normalized state diffs with
  cheatcodes.

Generated artifacts belong under `fuzz/fandango/out/`, which is ignored.
Promote only minimized, stable failures into `corpus.jsonl` or `tests/ui/`.

## CI

The `fandango` CI job is report-only. It uses Fandango/SolSmith for generation
and Foundry for the builtin-fuzzer differential lane. It runs:

- the committed ABI corpus,
- a small bounded sample from `abi-values.fan`, and
- small bounded samples from `solidity-source.fan` and
  `solidity-runtime-source.fan`, and
- a small bounded sample from `solsmith.py`, and
- replayed runtime regressions from `runtime-regressions/*.json`, and
- a small bounded Foundry fuzz differential over generated SolSmith sources.

The ABI runner compares:

- raw `eth_call` return bytes or revert bytes,
- transaction receipt status,
- emitted log topics and data, excluding contract address, and
- the contract storage root via `eth_getProof`.

Gas and bytecode size are not compared here; benchmark jobs report those.

The Foundry differential target uses `vm.recordLogs()` and
`vm.startStateDiffRecording()` / `vm.stopAndReturnStateDiff()`. It normalizes
the two implementation addresses before hashing logs and account/storage
accesses, because solc and this compiler run at different addresses. Storage
writes are folded as final per-slot side effects so the oracle is not sensitive
to semantically irrelevant write ordering.

The `solidity-runtime-source.fan` grammar is intentionally small. It is closer
to a mutation-guided runtime corpus than a full Solidity generator. `solsmith.py`
is the main path for broader valid-by-construction program generation.

## Run Locally

Use `uv tool run` with the pinned Fandango version:

```bash
uv tool run --quiet --from 'fandango-fuzzer==1.1.1' fandango --help
```

Run ABI runtime differentials against a local anvil:

```bash
anvil --silent --port 8545

PYTHONHASHSEED=1 uv tool run --quiet --from 'fandango-fuzzer==1.1.1' fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 32 \
  --separator $'\n' \
  --progress-bar off \
  | python3 fuzz/fandango/encode_abi_vectors.py --seed 1 \
  | python3 fuzz/fandango/run_abi_vectors.py \
      --max-vectors 256 \
      --max-calldata-bytes 4096 \
      --timeout 20
```

Generate Solidity sources and compile-check each file:

```bash
mkdir -p fuzz/fandango/out/sources

PYTHONHASHSEED=1 uv tool run --quiet --from 'fandango-fuzzer==1.1.1' fandango fuzz \
  -f fuzz/fandango/solidity-source.fan \
  --random-seed 1 \
  -n 32 \
  --directory fuzz/fandango/out/sources \
  --filename-extension .sol \
  --progress-bar off

python3 fuzz/fandango/run_solidity_sources.py \
  --source-dir fuzz/fandango/out/sources \
  --max-sources 256 \
  --timeout 20 \
  --verbose
```

Failures are saved under `fuzz/fandango/out/failures/` or
`fuzz/fandango/out/source-failures/`.

### Tool Entrypoints

Use these from the repository root:

```bash
cargo build -p solar-compiler --bin solar
fuzz/bin/solsmith --help
fuzz/bin/solreduce --help
```

They are generated into ignored `fuzz/bin/` by the build and are thin wrappers
around the implementation scripts in this directory. That gives us command-like
UX without tracking generated files or breaking direct script usage in existing
automation.

Generate Solidity runtime harnesses and compare side effects:

```bash
mkdir -p fuzz/fandango/out/runtime-sources

PYTHONHASHSEED=1 uv tool run --quiet --from 'fandango-fuzzer==1.1.1' fandango fuzz \
  -f fuzz/fandango/solidity-runtime-source.fan \
  --initial-population fuzz/fandango/runtime-corpus \
  --random-seed 1 \
  --population-size 24 \
  --mutation-rate 0.4 \
  --crossover-rate 0.4 \
  -n 16 \
  --directory fuzz/fandango/out/runtime-sources \
  --filename-extension .sol \
  --progress-bar off

python3 fuzz/fandango/run_source_runtime.py \
  --source-dir fuzz/fandango/out/runtime-sources \
  --max-sources 32 \
  --cases-per-source 8 \
  --timeout 20 \
  --verbose
```

### Use SolSmith

SolSmith is the type-aware Solidity generator. It is the CSmith/Yarpgen-style
path in this directory: instead of sampling raw text from a grammar, it builds
small valid programs from typed fragments and emits the fixed
`setup/run/observe` harness expected by the runtime oracle.

The normal workflow is:

1. Generate SolSmith sources with a fixed seed.
2. Run the anvil runtime differential on every generated source.
3. Optionally hand one generated source to Foundry, where Foundry's builtin
   fuzzer drives calldata, caller, and environment values while cheatcodes
   compare returndata, logs, and normalized state diffs.

Fandango remains useful for ABI-value sampling and grammar/corpus mutation.
SolSmith is the stronger path when we need valid-by-construction Solidity
programs with explicit feature coverage.

Quick CSmith-style loop:

```bash
cargo build -p solar-compiler --bin solar

fuzz/bin/solsmith \
  --seed 1 \
  --count 16 \
  --require-default-features \
  --out-dir fuzz/fandango/out/solsmith-sources

python3 fuzz/fandango/run_source_runtime.py \
  --source-dir fuzz/fandango/out/solsmith-sources \
  --cases-per-source 8 \
  --timeout 20

fuzz/bin/solreduce \
  --failure fuzz/fandango/out/runtime-failures/runtime-0-example.json \
  --out fuzz/fandango/out/reduced/example.sol
```

The reducer command is only needed after the runtime runner saves a failure
JSON.

```bash
mkdir -p fuzz/fandango/out/solsmith-sources

fuzz/bin/solsmith \
  --seed 1 \
  --count 16 \
  --require-default-features \
  --out-dir fuzz/fandango/out/solsmith-sources \
  --metadata fuzz/fandango/out/solsmith-metadata.json

python3 fuzz/fandango/run_source_runtime.py \
  --source-dir fuzz/fandango/out/solsmith-sources \
  --max-sources 32 \
  --cases-per-source 8 \
  --timeout 20 \
  --verbose
```

Use `run_foundry_target.py` below to run one generated source through the
Foundry differential target.

Replay a runtime failure JSON:

```bash
python3 fuzz/fandango/run_source_runtime.py \
  --replay-failure fuzz/fandango/out/runtime-failures/runtime-0-example.json \
  --timeout 20 \
  --verbose
```

Reduce a replayable runtime failure:

```bash
fuzz/bin/solreduce \
  --failure fuzz/fandango/out/runtime-failures/runtime-0-example.json \
  --out fuzz/fandango/out/reduced/example.sol \
  --max-attempts 512 \
  --max-rounds 12 \
  --timeout 20 \
  --verbose
```

The reducer is oracle-driven: it tries statement chunks, structured
branch/loop rewrites, expression simplifications, literal shrinking, and unused
helper removal, but keeps only candidates that still reproduce with
`run_source_runtime.py --replay-failure`. It needs a running anvil endpoint and
can be slow: each candidate may compile and execute both runtimes. Candidate
acceptance preserves the original failure kind, label, and calldata, but the
reduced program should still be reviewed before promotion.

Promote a reduced source into a replay corpus location:

```bash
python3 fuzz/fandango/promote_runtime_failure.py \
  --failure fuzz/fandango/out/reduced/example.json \
  --name example \
  --mode regression
```

Create a Foundry fuzz target for one generated harness:

```bash
python3 fuzz/fandango/write_foundry_target.py \
  --source fuzz/fandango/out/solsmith-sources/solsmith-0000.sol \
  --out-dir fuzz/fandango/out/foundry-target

cd fuzz/fandango/out/foundry-target
forge test
```

Run the generated Foundry target with solc and this compiler:

```bash
python3 fuzz/fandango/run_foundry_target.py \
  --source fuzz/fandango/out/solsmith-sources/solsmith-0000.sol \
  --solc solc \
  --solar target/debug/solar \
  --fuzz-runs 64 \
  --verbose
```

This compiles the generated harness with both compilers, embeds both runtime
bytecode blobs into one Foundry test, installs them at fixed addresses with
`vm.etch`, and lets Foundry fuzz calldata/caller/environment values while the
test compares return bytes, revert behavior, logs, and normalized state diffs.

## Scaling

PR CI uses bounded counts. Scheduled/manual CI runs can scale the same script
with environment variables:

- `FANDANGO_ABI_SEED_COUNT`
- `FANDANGO_ABI_VALUES_PER_SEED`
- `FANDANGO_SOURCE_COUNT`
- `FANDANGO_RUNTIME_SOURCE_COUNT`
- `FANDANGO_SOLSMITH_COUNT`
- `FANDANGO_RUNTIME_CASES`
- `FANDANGO_FOUNDRY_TARGETS`
- `FANDANGO_FOUNDRY_FUZZ_RUNS`

## Extend

For ABI fuzzing:

1. Add value shapes to `abi-values.fan`.
2. Add or update methods in `AbiVectorFixture.sol`.
3. Teach `encode_abi_vectors.py` how to encode the new shape and whether it is
   a `call` or `tx` vector.
4. Add minimized stable cases to `corpus.jsonl` when they should run on every PR.

For source fuzzing:

1. Add valid-by-construction source shapes to `solidity-source.fan`.
2. Add stateful harness shapes to `solidity-runtime-source.fan` when runtime
   behavior is the useful signal.
3. Add small, stable runtime seed programs to `runtime-corpus/` when they should
   guide Fandango's initial population.
4. Add type-aware generation features to `solsmith.py` when grammar snippets are
   not precise enough, then expose them through `fuzz/bin/solsmith` options if
   they need user-facing controls.
5. Extend the Foundry differential target when a new runtime effect is better
   compared inside Foundry than through JSON-RPC replay.
6. Keep grammars inside the subset both solc and this compiler are expected to
   compile.
7. Promote confirmed source mismatches into ordinary `tests/ui/` cases.

## Constraints

- ABI runtimes are installed with `anvil_setCode`, so constructors do not run.
  The fixture must not depend on constructor state, `immutable`s, or preset
  storage.
- Runtime-source differential runners also install runtimes with
  `anvil_setCode`; generated harnesses must initialize state through `setup`.
- Generated Foundry targets use zero call value because the current harness is
  non-payable. Add payable harness support before fuzzing nonzero value.
- Keep the ABI fixture independent of block and transaction environment values
  such as `block.timestamp`, `block.number`, `block.basefee`, and `tx.gasprice`.
- `corpus.jsonl`, `runtime-corpus/`, `runtime-regressions/`, and SolSmith with
  a fixed seed are deterministic. Seeded Fandango samples are bounded sampling
  and may drift with Fandango or Python behavior; use them to find cases worth
  promoting.
