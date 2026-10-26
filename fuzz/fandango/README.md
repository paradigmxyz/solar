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
exposes Foundry's symbolic executor to compare a contract compiled by Solc and
Solar over bounded symbolic inputs. `fuzz/bin/solsymdiff` remains a direct
entry point.

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

# Every function and every two-call sequence, from a symbolic msg.sender.
uv run --project tools/compiler-diff compiler-diff symbolic -- \
  --source /absolute/path/to/Target.sol \
  --solar "$PWD/target/debug/solar" \
  --contract Target \
  --all-functions --depth 2 --symbolic-caller
```

Both compilers receive the same closed Standard JSON input and compiler
settings. Each generated Foundry property takes a state snapshot, runs the
Solc creation code with the constructor arguments and value, makes its calls,
records the outcome, and reverts to the snapshot. It then runs the Solar
creation code the same way, at the same address. Both builds therefore observe
the same `address(this)`, external self-calls, log emitters, and created-contract
addresses. The property compares:

- the constructor's success and revert data;
- every call's success and exact returndata, at any length;
- ordered logs, including emitter, topics, and data, of the constructor and of
  every call that succeeded; a reverted top-level call's logs are discarded on
  chain, so they are not compared;
- for the target and every contract it created with `CREATE`: every storage
  slot either build wrote, read back after the last call, plus the final
  balance, nonce, and whether code is present; slots neither build wrote keep
  their initial zero, and created code itself is not compared because it
  legitimately differs between compilers; and
- the final balances of the harness, every symbolic caller, and a symbolic
  `observer` account, so Ether sent to any account must agree.

Every call and the constructor carry a symbolic Ether value, so a missing
`callvalue` check on a nonpayable function is a difference too. A mismatch
reports what differs in `differs_in`, for example `storage`, `returndata`,
`logs`, `call success`, `balance`, or `abi` when the two builds expose
different functions or selectors; an ABI difference is reported without
running Forge. The result schema is `solar:solsymdiff@v2`.

Constructor arguments are symbolic by default. `--constructor-args 0x...`
fixes them to ABI-encoded values, which keeps solver queries small when
immutables feed arithmetic. `--runtime-only` installs the runtime without
running the constructor; contracts with immutables require constructor
execution.

`--signature` selects one function. `--all-functions` instead generates, per
function, one property with ABI-encoded symbolic arguments and one that sends
the function's selector followed by arbitrary bytes of lengths around its
argument head, which exercises the compilers' ABI decoding with short data,
dirty values, and bad offsets. It adds one property that sends calldata matching
no selector (empty, short, selector-only, and one-word lengths), which reaches
`fallback` and `receive`. Functions with inputs the symbolic executor cannot
generate are listed under `skipped`. `--depth N` compares every call sequence
of up to `N` calls whose earlier calls are state-changing functions, each with
its own symbolic arguments, caller, and value; `--max-checks` caps the number
of generated sequences. The result lists every sequence under `checks`. The
overall status is `mismatch` if any sequence mismatched, otherwise `incomplete`
if any sequence was incomplete; a sequence Forge cannot classify is reported
incomplete on its own without hiding the others.

`--arbitrary-storage` gives every fixed storage slot of the target a symbolic
starting word, written after the constructor and before the first call, so a
property covers states that no generated call sequence reaches. Solidity code
never stores a `bool` other than 0 or 1, an enum value outside its members, or
nonzero bytes outside a slot's declared fields, and compilers may treat such
storage differently, so the property assumes those encodings away using Solc's
storage layout and AST; the covered starting states are the validly encoded
ones. A slot that holds a user-defined value type or an internal function
pointer starts empty, because its encoding is not visible in the layout. The
words are test inputs, so a counterexample reports them under `starting_storage`
and Foundry replays them exactly. Mapping and dynamic-array
contents, and `bytes` and `string` slots, start empty, because their contents
live at hashed slots that concrete replay cannot reproduce. The mode needs a
source compiled by this tool and rejects saved attempts, which carry no AST.
Because a slot only one build wrote holds the other build's starting value,
Solc runs a second time to read its final value at every slot either build
wrote.

Pure functions are enabled with `--signature` by default. `--include-view`,
`--include-stateful`, and `--include-payable` allow the other mutabilities;
`--all-functions` and `--depth` above one include every mutability.
`--symbolic-caller` makes each call's `msg.sender` symbolic; without it, the
harness is the caller. Top-level `address` inputs, symbolic callers, and the
observer are assumed to be zero or above `0x100`: otherwise the solver can pick
a precompile whose symbolic input Foundry does not model, which leaves the
whole run incomplete even though both builds pass it the same address.
`--allow-precompile-addresses` drops that assumption. A counterexample reports
`constructor_args` and `constructor_value`, every call under `calls` with its
calldata, `caller`, and `value`, the `observer`, and repeats the last call at
the top level.

Repeat `--prefix-calldata 0x...` to prepend fixed zero-value calls before the
symbolic calls. Prefix calldata includes the selector and ABI-encoded
arguments; `0x` targets receive or fallback. Prefix success, returndata, logs,
and storage writes are compared like every other call.

All calls of a property run in one Foundry test transaction, while on chain
they are separate transactions. A target that ends a property without code,
because SELFDESTRUCT deleted it as a same-transaction creation, is reported
incomplete. When either build uses `TSTORE`, the constructor and the calls count
as one sequence, and a sequence of more than one is reported incomplete,
because transient storage would persist between them.

Calls forward all remaining gas because Foundry's symbolic executor does not
model gas and reports explicit gas caps, including those of Solidity's
`transfer` and `send`, as incomplete. Calls to addresses without code are
modeled as calls to empty accounts (`symbolic_call_targets = true`); in this
closed world the only code belongs to the target, the contracts it creates,
the harness, cheatcodes, and active precompiles. Calls into pre-existing
external code and the behavior of created contracts are outside this lane;
compare a created contract's own source to cover its behavior. Contracts
created with `CREATE2` or by created contracts are not compared, and a target
whose runtime uses `CREATE2` lists that under `limitations`. Foundry records
the logs of inner frames that revert inside a successful call and does not say
which frame emitted each log, so it cannot drop them. A build with a `LOG`
opcode and any call or create opcode is therefore reported incomplete.

A generated symbolic bound is raised as needed to hold both builds' bytecode
and is recorded in `result.json`. A mismatch is reported only after Foundry
concretely replays the counterexample and the replay fails in one of the
harness's comparisons; a replay that fails elsewhere, for example at a
`vm.assume` because replay cannot materialize arbitrary storage read at
symbolic mapping keys, is reported incomplete. The generated project, compiler input,
and `result.json` are kept under
`/tmp/solar-sourcify/<version>/engines/symbolic/<id>/` through the unified CLI;
select another base directory with `--dir` before `symbolic`. The direct
`fuzz/bin/solsymdiff` entry point defaults to `target/solsymdiff/` and accepts
`--output-root`.

To reuse recorded compilations, pass `--solc-attempt` and `--solar-attempt` with
absolute attempt-directory paths. In this mode, `--source` names a source unit
in the saved input, such as `src/Target.sol`. Both attempts must contain identical
inputs, an explicit `evmVersion`, and runtime immutable/link reference outputs;
attempts with creation bytecode run the constructor, and attempts without it
run in runtime-only mode.
Solc still compiles the harness. See the [saved-artifact example](../../tools/compiler-diff/README.md#execution-engines).

For real projects, use `--project-root`, repeat `--include-path`, and pass
`--remapping prefix=target` as needed. Solc resolves the import closure once;
the tool then embeds all resolved sources before compiling with either
compiler, so the final compiler inputs are byte-for-byte identical.

Programs whose result depends on remaining gas, transaction-local access
warmth, or other dynamic gas context remain outside the sound comparison
scope; Foundry reports gas observations as incomplete rather than proving them.

The default dynamic input lengths are `0,1,2,3`. Use `--dynamic-lengths` to
change all dynamic inputs or repeat `--input-length INDEX=LENGTHS` for a
specific top-level array, `bytes`, or `string` argument of the selected
`--signature`; overrides apply only to calls of that function. `--max-paths`,
`--max-solver-queries`, `--max-depth`, `--max-calldata-bytes` (per call; the
harness multiplies it by the number of symbolic calls and the constructor),
`--symbolic-timeout`, and `--exploration-order` expose the search bounds an
external campaign runner needs.

`fuzz/fandango/run_symbolic_campaign.py` runs `--all-functions` over every
deployable contract of the given sources and writes `results.jsonl` as results
arrive plus a `summary.md` with mismatches, incomplete reasons, skipped
functions, and limitations. It fails when a mismatch is found or a source has
no deployable contract.

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
