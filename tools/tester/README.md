# solar-tester

Integration test support for the compiler.

`crates/solar/tests.rs` passes the freshly built `solar` binary to the UI, MIR,
EVM IR, standard JSON, and upstream compatibility runners. The Foundry runner is
a separate test in this crate that runs in the workspace's default
`cargo nextest run` and discovers every project under `tests/foundry` with a
`foundry.toml`. Run one suite with a `cargo tq` alias, such as `cargo tq ui`,
`cargo tq solc-solidity`, or `cargo tq foundry`. Set `SOLAR_FOUNDRY_PROJECT` to
run one discovered project.

Arguments after a UI-mode suite name (`ui`, `mir`, `evm-ir`, `standard-json`,
`solc-solidity`, `solc-yul`) filter tests by path substring, as in
`cargo uitest tests/ui/typeck` or `cargo uibless tests/ui/typeck`. Filtered runs
use `cargo test`, since nextest would read the filters as test names.

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

With `SOLAR_RUN_CALL_MIR` set, every `run-call` and `run-call-fail` directive also
runs through the MIR interpreter (`solar_codegen::interpret`) on the final MIR of
the called contract, which the runner obtains by compiling the test again with
`-Zdump=mir-final`, along with the frames the backend takes from the heap for
internal calls. The interpreter starts from the storage, balances, code, and
heap start the EVM had just before the call, and must end the same way, return
the same data, emit the same logs, and write the same storage. A disagreement
fails the test: it is a bug in the backend or in the interpreter. Calls the
interpreter cannot run are skipped: calls to other contracts, contract creation,
`gas`, and deployed code that is not the compiled runtime, such as a contract
with immutables.

`SOLAR_RUN_CALL_MIR=1` reports only disagreements. Any other value names a file
that receives one line per call, `checked`, `skipped` with the reason, or
`mismatch`:

```console
SOLAR_RUN_CALL_MIR=target/run-call-mir.log TESTER_MODE=ui cargo test -p solar-compiler --test tests
```

A test written in lowered MIR, such as those in `tests/ui/codegen/mir/interp/`,
exists to run its calls both ways, so its directives are always checked, and a
call the interpreter cannot run fails the test instead of being skipped. The
runner compiles the module like a contract named after the file and the module.
MIR has no ABI, so a directive names the function by its signature, followed by
its outputs when it returns values, as in `add(uint256,uint256)(uint256) 2, 3 => 5`.

To look into a disagreement, run the call by hand with
[solar-mir-interp](../mir-interp/README.md): pass it the test's
`-Zdump=mir-final` output and the call, and add `--trace` to see every operation
the interpreter runs.

## Compiler artifact comparisons

[compiler-diff](../compiler-diff/README.md) handles local or Sourcify
standard-JSON inputs, saved ABI/JSON comparisons, and runtime or symbolic
checks. Its reports and replay bundles help reduce failures into UI or Foundry
regression tests here.

## Debug-info differential suite

`SOLDB=/path/to/soldb/target/debug/soldb cargo tq debug-diff` compiles and runs
local debug-info comparisons against solc and between our ETHDebug and legacy
source-map formats. See [the suite guide](../../tests/debug-diff/README.md) for
checkpoints, required tools, saved reports, and known compiler gaps.

## External Foundry suite

`cargo tq foundry-external [name]` runs curated real-world Foundry projects
(morpho-blue, solmate, solady, seaport, openzeppelin-contracts,
uniswap-v4-core) as a differential suite: both compilers run each project's own
tests with a fixed fuzz seed, solc's passing tests are the oracle, and artifacts
are audited for parity. The test is `#[ignore]`d, never runs in CI, and needs
the network on first use.

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
- `SOLAR_FOUNDRY_REPORT_DIR`: also write per-project JSON reports.
- `SOLAR_FOUNDRY_COMPILER`: Solar executable to use instead of the latest local
  build; relative paths start at the workspace root. A debug build enables MIR
  and EVM IR validation by default.
- `SOLAR_FOUNDRY_OPTIMIZATION`: compile the Solar leg with `none`, `gas`, or
  `size`, keeping the project's solc optimizer settings, so the unoptimized
  Solar leg can run projects that need solc's optimizer to compile.
