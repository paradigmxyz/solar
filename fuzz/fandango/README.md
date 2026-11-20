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

Generated artifacts from the direct runners belong under `fuzz/fandango/out/`,
which is ignored. The unified `compiler-diff fuzz` command uses its chosen `--dir`.
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
uv tool run --quiet --python "$(cat .python-version)" --from 'fandango-fuzzer==1.1.1' fandango --help
```

Run ABI runtime differentials against a local anvil:

```bash
anvil --silent --port 8545

PYTHONHASHSEED=1 uv tool run --quiet --python "$(cat .python-version)" --from 'fandango-fuzzer==1.1.1' fandango fuzz \
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

Generate Solidity sources and compare their ABI and selectors through the shared
[compiler-diff campaign runner](../../tools/compiler-diff/README.md#fandango-campaigns):

```bash
uv run --project tools/compiler-diff compiler-diff --dir /tmp/compiler-fuzz fuzz \
  --seed 1 --count 32 \
  --compiler 'solc=/absolute/path/to/solc --standard-json' \
  --compiler 'solar=/absolute/path/to/solar --standard-json'
```

The runner records the grammar, seed and generated sources, then compiles and
compares each case. Repeat the command to reuse generation and cached attempts;
add `--continue-on-failure` to process later cases or `--symbolic-signature` to
check a selected function. Campaign artifacts stay under
`/tmp/compiler-fuzz/<version>/fuzz/<id>/`. Custom source grammars and arbitrary
named standard-JSON compilers use the same database and replay bundles.

The direct `run_solidity_sources.py` runner remains available for compile-acceptance
checks and the bounded CI lane; it does not compare ABI or documentation outputs.

Failures are saved under `fuzz/fandango/out/failures/` or
`fuzz/fandango/out/source-failures/`.

### Tool Entrypoints

Use these from the repository root:

```bash
fuzz/bin/solsmith --help
fuzz/bin/solreduce --help
```

They are committed thin wrappers around the implementation scripts in this
directory. That gives us command-like UX without breaking direct script usage
in existing automation.

For saved compiler artifacts and bounded mutation batches, use the unified
[population campaign command](/tools/compiler-diff/README.md#fandango-campaigns).
It snapshots the grammar and seed directory, records compiler attempts, compares
ABI/JSON artifacts, and can run bounded symbolic checks. Use the direct runtime
runner below to compare harness side effects in anvil.

Generate Solidity runtime harnesses and compare side effects:

```bash
mkdir -p fuzz/fandango/out/runtime-sources

PYTHONHASHSEED=1 uv tool run --quiet --python "$(cat .python-version)" --from 'fandango-fuzzer==1.1.1' fandango fuzz \
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

### Symbolic Solc-vs-Solar differential

The [compiler-diff CLI](../../tools/compiler-diff/README.md#execution-engines)
compares a contract compiled by Solc and Solar with Foundry's
`forge equivalence`, which symbolically checks whether two runtime bytecodes
behave the same on every input within its bounds. `fuzz/bin/solsymdiff` remains a
direct entry point. It needs a Forge built after foundry-rs/foundry#18065, an
SMT solver such as z3 or Bitwuzla, and anvil to run constructors; it prefers
the anvil next to the `--forge` binary.

Agents using this workflow should also follow the repository's
[agent guidance](../../AGENTS.md).

```bash
cargo build -p solar-compiler --bin solar

# One function.
uv run --project tools/compiler-diff compiler-diff symbolic -- \
  --source /absolute/path/to/Target.sol \
  --solar "$PWD/target/debug/solar" \
  --contract Target \
  --signature 'probe(uint256,bytes)'

# Every function and raw calldata, from arbitrary starting storage.
uv run --project tools/compiler-diff compiler-diff symbolic -- \
  --source /absolute/path/to/Target.sol \
  --solar "$PWD/target/debug/solar" \
  --contract Target \
  --all-functions --arbitrary-storage
```

Both compilers receive the same closed Standard JSON input and compiler
settings. solsymdiff then deploys both builds' creation code on a private
anvil, from the same account and nonce and so at the same address, the one
`forge equivalence` later runs the runtimes at, with
`--constructor-args 0x...` (default: the zero value of every constructor input)
and `--constructor-value` (default 0). Both deployments run in a block with the
same timestamp. The `<constructor>` check compares the two deployments'
success, revert data, logs with their emitters, and the deployed contract's
storage, balance, and nonce; a difference is a mismatch in `constructor`, and
`comparison` says which. Contracts the constructor creates and accounts it
calls or pays are not compared; a constructor that can do so lists that under
`limitations`. When both deployments
succeed, their deployed runtimes, with immutables filled in, are what the
remaining checks compare. `--runtime-only` compares the runtimes as compiled
instead; contracts with immutables require constructor execution.

Each remaining check is one `forge equivalence` run, with Solc's runtime as code
A and Solar's as code B. Both run at one address from the same starting state,
on shared symbolic calldata, sender, origin, call value, block values, and gas
price. On every pair of feasible paths Forge compares:

- success and the exact return or revert data; and
- when both succeed, the ordered logs and the storage, balance, nonce, and code
  of every account either build changed.

Gas is not priced, so code that spends more gas to do the same thing agrees,
but a build that runs out of memory gas where the other does not differs;
`--ignore-out-of-gas` leaves those inputs out. Each build reads its own
unconstrained `GAS`. Calls into unknown code answer as one fixed unknown function
of their inputs; a state-changing call into unknown code may re-enter, so it
makes the check incomplete unless `--promise-no-reent` promises that it does not.
The call value is symbolic unless `--value WEI` fixes it, so a missing
`callvalue` check on a nonpayable function is a difference too.

`--signature` runs one check of ABI-encoded calls of one function
(`forge equivalence --sig`). `--all-functions` runs one such check per function,
labelled with its signature, then one `<other calldata>` check on raw calldata of
fixed lengths: empty, one byte, short, selector-only, one word, and one byte
short of, exactly, and one word past every function's argument head. Forge
cannot fix the selector of raw calldata, so this check covers both calldata that
matches no selector, which reaches `fallback` and `receive`, and short or dirty
arguments of every function, which exercise the compilers' ABI decoding; a
counterexample names the function whose selector it starts with. Lengths above
`--max-calldata-bytes` (default 1024) are left out. Functions with inputs Forge
cannot generate are listed under `skipped`. Dynamic inputs take each length in
`--dynamic-lengths` (default `0,1,2,3`).

Storage starts empty. Forge does not take the constructor's storage as the
calls' starting state, so a call sees empty storage even where the constructor
wrote, while immutables keep their constructed values. `--arbitrary-storage`
starts every check from arbitrary storage instead, which covers states that only
earlier calls reach. Forge does not restrict that storage to valid encodings: a
`bool` other than 0 or 1, an enum value out of range, or dirty bytes beside a
packed field are possible starting states. Solidity code never stores them, and
the compilers may read them differently, so a mismatch in this mode can be an
encoding artifact rather than a miscompilation; Forge's JSON does not report the
starting storage, so reproduce such a counterexample before reporting it.

The result lists every check under `checks` with its `status`:
`bounded_agreement`, `mismatch`, or `incomplete`. The overall status is `mismatch`
if any check mismatched, otherwise `incomplete` if any was. A mismatch reports
`differs_in`: `call success`, `returndata`, `logs`, `storage`, `balance`, `nonce`,
`out of gas`, `constructor`, or `abi` when the two builds expose different
functions or selectors, which is reported without running Forge. Its
`counterexample` holds the `calldata`, the decoded `args` of a function check,
the `caller` and `value`, the block environment under `env`, each build's success,
output, and logs under `solc` and `solar`, and the storage slots, balances, and
accounts whose final values differ. Forge reports a counterexample only after
replaying it concretely. A model it cannot replay, such as one that depends on
`GAS` or on answers of unknown code, a bound that stops the search, an
unsupported operation, a Forge error, and a timeout each leave that check
incomplete with Forge's reason, without hiding the other checks. The result
schema is `solar:solsymdiff@v3`.

Forge compares the code of created contracts byte for byte, and code created
by Solc and Solar legitimately differs. A counterexample that differs only in
created code is therefore reported incomplete, and a target whose runtime has
`CREATE` or `CREATE2` lists that under `limitations`; compare a created
contract's own source to cover its behavior.

`--timeout` (default 60 s) bounds each compiler run and each check: it is Forge's
own timeout, and solsymdiff stops a Forge that runs a minute past it.
`--max-paths`, `--max-depth` (opcodes per path), `--max-solver-queries`, the
dynamic lengths, and the EVM version reach Forge through a `foundry.toml` in the
run directory, and `result.json` records them under `bounds`. The run directory
also keeps the compiler input, both compared runtimes (`solc.hex` and
`solar.hex`), and each check's Forge command and output (`check-N.json`). It
lives under `/tmp/solar-sourcify/<version>/engines/symbolic/<id>/` through the
unified CLI; select another base directory with `--dir` before `symbolic`. The
direct `fuzz/bin/solsymdiff` entry point defaults to `target/solsymdiff/` and
accepts `--output-root`.

Compared with the earlier harness (`solar:solsymdiff@v2`), which ran a generated
Foundry test under `forge test --symbolic`, these are no longer covered:

- call sequences: each check is one call, so `--depth` is gone; use
  `--arbitrary-storage` for states that need earlier calls;
- contracts the target creates whose code differs between the builds;
- the constructor's storage as the calls' starting state, and symbolic
  constructor arguments and value: the constructor runs once, concretely;
- arbitrary storage restricted to valid encodings; and
- fixed calls before the symbolic ones (`--prefix-calldata`).

To reuse recorded compilations, pass `--solc-attempt` and `--solar-attempt` with
absolute attempt-directory paths. In this mode, `--source` names a source unit
in the saved input, such as `src/Target.sol`. Both attempts must contain identical
inputs, an explicit `evmVersion`, and runtime immutable/link reference outputs;
attempts with creation bytecode run the constructor, and attempts without it
run in runtime-only mode. See the
[saved-artifact example](../../tools/compiler-diff/README.md#execution-engines).

For real projects, use `--project-root`, repeat `--include-path`, and pass
`--remapping prefix=target` as needed. Solc resolves the import closure once;
the tool then embeds all resolved sources before compiling with either
compiler, so the final compiler inputs are byte-for-byte identical.

`fuzz/fandango/run_symbolic_campaign.py` runs `--all-functions` over every
deployable contract of the given sources and writes `results.jsonl` as results
arrive plus a `summary.md` with mismatches, incomplete reasons, skipped
functions, and limitations. Its `--timeout` applies to each compiler run and
each check. It fails when a mismatch is found or a source has no deployable
contract.

A source ending in `.json` or `.json.gz` is a whole project: a Standard JSON
input with inline sources, such as the archives in `testdata/projects/`. The
runner compiles it once with each compiler, keeping the project's optimizer,
via-IR, EVM version, and remappings, and compares every deployable contract of
the units that `--exclude` does not match (tests, scripts, and test libraries
by default) from those saved attempts. A missing `evmVersion` becomes
`osaka`.

```sh
python3 fuzz/fandango/run_symbolic_campaign.py \
  testdata/projects/morpho-blue-1.0.0.json.gz \
  --out target/solsymdiff-morpho --solc solc --solar target/release/solar
```

Exit status 0 means no mismatch was found within the recorded bounds, 1 means a
concretely replayed mismatch or an ABI difference was found, and 2 means the
run was incomplete. This is a bounded check, not an unbounded equivalence
proof. A mismatch proves the two recorded builds differ; source minimization and
a Solidity-spec review still decide which compiler, if either, is wrong.

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
