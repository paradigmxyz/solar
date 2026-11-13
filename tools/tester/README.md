# solar-tester

Integration test support for the compiler.

`crates/solar/tests.rs` passes the freshly built `solar` binary to the UI, MIR,
EVM IR, standard JSON, and upstream compatibility runners, and
`crates/solar/tests/it/foundry.rs` passes it to the Foundry runner. The Foundry runner is
a separate test that runs in the workspace's default
`cargo nextest run` and discovers every project under `tests/foundry` with a
`foundry.toml`. Run one suite with a `cargo tq` alias, such as `cargo tq ui`,
`cargo tq solc-solidity`, or `cargo tq foundry`. Set `SOLAR_FOUNDRY_PROJECT` to
run one discovered project.

The Foundry runner runs solc once per project, with the project's own optimizer
settings, as the baseline. It then runs the compiler at `-O none`, `-O gas`, and
`-O size`, whatever the project's `foundry.toml` says, and every level must pass
every test. A level that fails its assertions is named in the output, and the
other levels are still judged. Set `SOLAR_FOUNDRY_OPTIMIZATION` (see below) to
run one level, such as `SOLAR_FOUNDRY_OPTIMIZATION=none cargo tq foundry`. Only
gas use and code size may differ between levels. The gas and size comparisons
printed for each level are against solc at the project's own optimizer settings,
which is unoptimized for most projects.

Arguments after a UI-mode suite name (`ui`, `mir`, `evm-ir`, `standard-json`,
`solc-solidity`, `solc-yul`) filter tests by path substring, as in
`cargo tq ui tests/ui/typeck` or `cargo bless ui tests/ui/typeck`. Filtered runs
use `cargo test`, since nextest would read the filters as test names. `cargo t`
runs every suite and `cargo bless` blesses every UI-mode suite, including
standard JSON.

## Runtime directives

UI tests can run one isolated entry-point call per `run-call` or `run-call-fail`
directive. Each deploys a fresh contract, so calls never share state. Use
Foundry projects under `tests/foundry/` for multi-transaction sequences,
persistent state, multiple actors or contracts, event assertions, cheatcodes,
and complex setup.

- `//@ run-call: add 1, 2 => 3` ABI-encodes and calls the named function, then
  compares its ABI-encoded return values. Omit `=>` when no return data is
  expected. Calldata and return data may be raw hex. Comma-separated settings
  follow a semicolon, as in `add 2; constructor=[40], gas=100000, value=3 => 45`:
  `constructor=[...]` gives ABI-encoded constructor arguments, `gas` the call
  transaction's gas limit, and `value` its value in wei. Numbers may be decimal
  or `0x`-prefixed. Deployment and `setUp()` use the default gas limit and zero
  value.
- `//@ run-call-fail: fail()` requires the call to fail. Add `=> 0x...` to check
  exact revert data.

Both use the EVM version from `--evm-version`. Calls to `test*` functions first
run a zero-argument `setUp()` if the contract defines one.

## MIR interpreter check

In a test written in lowered MIR, such as those in `tests/ui/codegen/mir/interp/`,
every `run-call` and `run-call-fail` directive also runs through the MIR
interpreter (`solar_mir_interpreter`) on the final MIR of the called
contract, which the runner obtains by compiling the test again with
`-Zdump=mir-final`. The interpreter starts from the storage, balances, and code
the EVM had just before the call, places memory where the backend placed it, and
must end the same way, return the same data, emit the same logs, and write the
same storage. A disagreement fails the test: it is a bug in the backend or in
the interpreter. So does a call the interpreter cannot run: calls to other
contracts, contract creation, `gas`, and deployed code that is not the compiled
runtime, such as a contract with immutables. The runner compiles the module like
a contract named after the file and the module.
MIR has no ABI, so a directive names the function by its signature, followed by
its outputs when it returns values, as in `add(uint256,uint256)(uint256) 2, 3 => 5`.

## EVM IR interpreter check

With `SOLAR_RUN_CALL_EVM_IR` set, every `run-call` and `run-call-fail` directive
also runs through the EVM IR interpreter (`solar_evmir_interpreter`)
on the runtime EVM IR of the called contract as stack scheduling emitted it,
before the EVM IR pipeline, which the runner obtains by compiling the test again
with `-Zdump=evm-ir-runtime-scheduled`. The interpreter starts from the storage,
balances, and code the EVM had just before the call, and must end the same way,
return the same data, emit the same logs, and write the same storage. The EVM ran
that program after the EVM IR pipeline and assembly, so a disagreement fails the
test: it is a bug in an EVM IR pass, the assembler, or the interpreter. Calls the
interpreter cannot run are skipped: calls to other contracts, contract creation,
`gas`, immutables, results that depend on where the assembler places code, and
deployed code that is not the compiled runtime. So are calls that run out of gas
or stack, which the interpreter does not model or the pipeline may change.

`SOLAR_RUN_CALL_EVM_IR=1` reports only disagreements and harness faults, such as a
failed dump. Any other value names a file that receives one line per call,
`checked`, `skipped` with the reason, `mismatch`, or `failed`:

```console
SOLAR_RUN_CALL_EVM_IR=target/run-call-evm-ir.log TESTER_MODE=ui cargo test -p solar-compiler --test tests
```

A test written in EVM IR, such as those in `tests/ui/codegen/evm-ir/interp/`,
exists to run its calls both ways, so its directives are always checked, on the
test's own text, and a call the interpreter cannot run fails the test instead of
being skipped. The runner compiles the program, after the test's
`-Zevm-ir-pipeline`, as the runtime code of a contract named after the file and
the module, whose creation code only deploys it. EVM IR has no ABI, so a
directive names the function by its signature, followed by its outputs when it
returns values, as in `add(uint256,uint256)(uint256) 2, 3 => 5`. A test that
names a pipeline keeps its output, without the compiler's JSON; one that runs
none prints nothing.

## Compiler artifact comparisons

[compiler-diff](../compiler-diff/README.md) handles local or Sourcify
standard-JSON inputs, saved ABI/JSON comparisons, and runtime or symbolic
checks. Its reports and replay bundles help reduce failures into UI or Foundry
regression tests here.

## Debug-info differential suite

`SOLAR_DEBUGIFY_DIR="$PWD/target/debugify/origins" TESTER_MODE=ui,standard-json
cargo nextest run -p solar-compiler --test tests --no-capture` audits source debug information
around every MIR and EVM IR pass in all UI tests. Test diagnostics and snapshots stay unchanged;
explicit `-Zdebugify=check` tests still reject gaps. Frontend-only and rejected inputs do not reach
IR passes.

Run `python3 scripts/debugify_report.py target/debugify/origins --output target/debugify` for a
summary by record and test/revision. The headline is the number of gaps, LLVM's `not-generate`
count of instructions without a source origin or a reason for lacking one; lost origins are listed
as information. Raw JSON-line files contain the invocation, source-file bounds, each pass's lost
spans, and per record the gap count, an exact opcode histogram and up to 100 gap sites. The report
rejects missing or damaged audits, and renders against a baseline summary from before gaps existed
as "no gap baseline".

The `Debugify` workflow runs this audit on pull requests and pushes to main. A
pull request comment lists the records and tests whose gaps changed against the
exact base commit's audit, like the codegen benchmark comment, with lost origins
as information. A base without an audit gets a report without changes, and one
audited before gaps existed gets absolute gap counts.

`SOLDB=/path/to/soldb/target/debug/soldb cargo tq debug-diff` compiles and runs
local debug-info comparisons against solc and between our ETHDebug and legacy
source-map formats. See [the suite guide](../../tests/debug-diff/README.md) for
checkpoints, required tools, saved reports, and known compiler gaps.

## External Foundry suite

`cargo tq foundry-external [name]` runs curated real-world Foundry projects
(morpho-blue, solmate, solady, seaport, openzeppelin-contracts,
uniswap-v4-core) as a differential suite: both compilers run each project's own
tests with a fixed fuzz seed, solc's passing tests are the oracle, and artifacts
are audited for parity. The compiler leg follows each project's own optimizer
settings unless `SOLAR_FOUNDRY_OPTIMIZATION` pins a level. The test is
`#[ignore]`d, never runs in CI, and needs the network on first use.

`tools/tester/src/foundry/external.rs` pins each project to a full commit hash.
Checkouts in `target/foundry-external/checkouts/` are reused without network. A
failed fetch skips the project, so offline runs degrade instead of failing.
`forge` downloads each project's own solc for the baseline leg.

Add a project (git submodules only for dependencies) when bugs show only at
whole-project scale: dispatch and ABI breadth, deep inheritance,
assembly-heavy libraries, EIP-170 pressure. Reduce what you can to minimal
in-repo `tests/foundry/` projects or `run-call` UI tests, and land a reduced
regression test with every fix this suite surfaces. Skip entries need a reason;
sustained divergences move to [SOLC_DIVERGENCE.md](../../docs/SOLC_DIVERGENCE.md).

Solmate needs Forge v0.3.0 on `PATH`: newer versions reject its `testFail*`
cases before running them. The other projects use a current Forge; OpenZeppelin
needs Osaka support. Solady disables transaction isolation because its ETH mover
tests share one transaction across several calls.

OpenZeppelin's ERC7579 tests read the EntryPoint and SenderCreator bytecode
files from `node_modules/hardhat-predeploy/bin` in its checkout. Install
`hardhat-predeploy@1.0.1` with the version and integrity from that checkout's
lockfile; the runner grants read access to that directory.

Seaport at `SOLAR_FOUNDRY_OPTIMIZATION=none` fails 16 tests that pass at `gas`
and `size`. `FuzzInscribers._getStorageSlotForOrderHash` uses `vm.accesses` to
assert that `getOrderStatus` reads storage with exactly one `SLOAD`, which holds
only in an optimized build; an unoptimized build reads the packed slot once per
field. Treat those failures as expected at `none`: the test assumes an
optimized build.

A green run can still hide tests that fail under both compilers; check the
reported baseline failures before claiming full coverage.

Environment variables:

- `SOLAR_FOUNDRY_PROJECT`: run one curated project, like the `name` argument.
- `SOLAR_FOUNDRY_EXTERNAL_MANIFEST`: TOML manifest that replaces the curated
  list, for out-of-repo projects. Each `[[project]]` table has `name` and either
  `repo` and `rev` (fetched) or `path` (an existing local directory, relative to
  the manifest). Optional keys: `mode` (`"test"` or `"build"`), `profile`
  (Foundry profile for both compiler legs), `prebuild_profiles` (Foundry
  profiles each leg builds with its own compiler before `forge test`, for
  suites that deploy those profiles' artifacts; a leg whose prebuild fails
  runs no tests), `solc_version` (solc version the compiler leg emulates, needed
  when sources pin an exact `pragma solidity`), `skip_tests` and
  `skip_contracts` (arrays of `{ pattern, reason }`; `reason` is required), and
  `notes`.
- `SOLAR_FOUNDRY_REPORT_DIR`: also write per-project JSON reports. The `solar`
  entry maps each optimization level (`none`, `gas`, `size`, or `project` when
  the project's own settings apply) to its run.
- `SOLAR_FOUNDRY_SKIP`: skip the default Foundry suite. Without it, a missing
  `forge` or Solar binary fails the suite instead of passing silently.
- `SOLAR_FOUNDRY_COMPILER`: Solar executable to use instead of the one Cargo
  builds for the test; relative paths start at the workspace root. A debug
  build enables MIR and EVM IR validation by default.
- `SOLAR_FOUNDRY_OPTIMIZATION`: run only the Solar leg at `none`, `gas`, or
  `size`, keeping the project's solc optimizer settings for the baseline, so
  the unoptimized Solar leg can run projects that need solc's optimizer to
  compile. Without it, the default suite runs all three levels and the external
  suite follows each project's own settings.

## Foundry coverage parity

`cargo tq foundry-coverage [project]` measures `forge coverage` across a matrix
of compilers and settings, so coverage parity between the compiler and solc can
be tracked and regressions caught. It is local-only like the external suite and
never runs in CI. `project` is an in-repo project under `tests/foundry` or an
external project from `foundry-external`; without it, every in-repo project
runs. Set `SOLAR_FOUNDRY_COVERAGE_EXTERNAL=1` to add every external project that
runs tests. External projects reuse the checkouts, pinned solc versions, test
fixes, skips, and prebuild profiles of `foundry-external`, and
`SOLAR_FOUNDRY_EXTERNAL_MANIFEST` applies as well.

Each project runs `forge coverage --report debug` in seven legs:

| Leg               | Compiler | Settings                          |
| ----------------- | -------- | --------------------------------- |
| `solc-legacy`     | solc     | forge's default: optimizer off    |
| `solc-legacy-opt` | solc     | optimizer on, runs 200            |
| `solc-ir-min`     | solc     | `--ir-minimum`                    |
| `solc-ir-opt`     | solc     | `viaIR` and optimizer on, runs 200 |
| `solar-none`      | compiler | optimizer off                     |
| `solar-gas`       | compiler | optimizer on, runs 200            |
| `solar-size`      | compiler | optimizer on, runs 1              |

Forge disables the optimizer and `viaIR` for coverage. The optimized legs run
`forge coverage --keep-optimizer` and set `FOUNDRY_OPTIMIZER`,
`FOUNDRY_OPTIMIZER_RUNS`, and `FOUNDRY_VIA_IR`. A forge whose
`forge coverage --help` lacks `--keep-optimizer` skips those legs with a message.
Fuzz and invariant limits are fixed and equal across legs (16 fuzz runs, 4
invariant runs of depth 16), and external projects keep their fixed fuzz seed.
Fuzzed code can still hit different counts per compiler, because forge's fuzz
dictionary reads constants from the bytecode. Legs of one project run in
parallel, except for projects with prebuild profiles.

The debug report lists every coverage item with its hits, then the anchors per
contract. An item is keyed by source path, kind (with branch and path IDs, or
the function name), and byte range. An item without an anchor in a leg has no
debug info there, and an anchored item with no hits was never reached. The
printed summary gives, for each leg, its items, anchored items, and hit items,
and against `solc-legacy` the items solc anchors that the leg does not, the
items solc hits that the leg does not, and the items both hit with different
counts. Each compiler leg also prints the same counts against its solc peers:
`solar-none` against `solc-ir-min`, and `solar-gas` and `solar-size` against
`solc-legacy-opt` and `solc-ir-opt`.

Coverage differences never fail the run. A leg fails the run only when forge
produces no report for it; a leg whose tests fail still counts, with status
`tests-failed`.

The runner writes one `coverage-<project>.json` per project to
`SOLAR_FOUNDRY_REPORT_DIR`, or `target/foundry-coverage` by default. It holds
each leg's status, totals, and differences, and every item with its anchored
flag and hits in each leg; an item that a leg does not list is `null` there.
