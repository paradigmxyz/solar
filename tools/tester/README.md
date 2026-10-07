# solar-tester

Integration test support for the compiler.

`crates/solar/tests.rs` passes the freshly built `solar` binary to the UI,
MIR, EVM IR, standard JSON, and upstream compatibility test runners. The
Foundry runner is a separate test in this crate, and runs as part of the
workspace's default `cargo nextest run`. Run individual suites through the
`cargo tq` aliases, such as `cargo tq ui` or `cargo tq solc-solidity`.
It discovers every project under `tests/foundry` that contains a `foundry.toml`.

Run only the Foundry suite with:

```console
cargo tq foundry
```

Set `SOLAR_FOUNDRY_PROJECT` to run one discovered project while debugging.

## Runtime directives

UI tests can execute one isolated entry-point call with `run-call` and
`run-call-fail`. Each directive deploys a fresh contract, so calls never share
state. Use Foundry projects under `tests/foundry/` for multi-transaction
sequences, persistent state, multiple actors or contracts, event assertions,
cheatcodes, and complex setup.

`//@ run-call: add 1, 2 => 3`: Deploy a fresh contract, ABI-encode and call the
named function, then compare its ABI-encoded return values. Omit `=>` when no
return data is expected. Raw calldata and return data may be written as hex.
Add settings after a semicolon, for example
`add 2; constructor=[40], gas=100000, value=3 => 45`. Settings are
comma-separated. `constructor=[...]` supplies ABI-encoded constructor
arguments, `gas` sets the call transaction's gas limit, and `value` sets its
value in wei. Numeric settings accept decimal and `0x`-prefixed integers.
Deployment and `setUp()` use the default gas limit and zero value.

`//@ run-call-fail: fail()`: Like `run-call`, but require the call to fail.
Add `=> 0x...` to check exact revert data. Both directives use the EVM version
selected by `--evm-version`. Calls to functions named `test*` run a
zero-argument `setUp()` first when the contract defines it.

## Compiler artifact comparisons

Use [compiler-diff](../compiler-diff/README.md) for local or Sourcify standard-JSON
inputs, saved ABI/JSON comparisons, and runtime or symbolic checks. Its reports
and replay bundles help reduce failures into the UI or Foundry regression suites
in this crate.

## Debug-info differential suite

`SOLDB=/path/to/soldb/target/debug/soldb cargo tq debug-diff` compiles and
executes local debug-info comparisons with solc and between our ETHDebug and
legacy source-map formats. See [the suite guide](../../tests/debug-diff/README.md)
for checkpoints, required tools, saved reports, and known compiler gaps.

## External Foundry suite

`cargo tq foundry-external [name]` runs curated real-world Foundry projects
(morpho-blue, solmate, solady, seaport, openzeppelin-contracts,
uniswap-v4-core) as a differential suite: both compilers run each project's
own tests with a fixed fuzz seed, solc's passing tests are the oracle, and
artifacts are audited for parity. It is local-only and never runs in CI: the
test is `#[ignore]`d and needs the network on first use.

Projects are pinned to full commit hashes in
`tools/tester/src/foundry/external.rs` and fetched into
`target/foundry-external/checkouts/`; later runs reuse the checkout with zero
network. Fetch failures skip the project, so offline runs degrade instead of
failing. `forge` resolves and downloads each project's own solc for the
baseline leg.

Add a project (pinned to a full commit hash, git submodules only for
dependencies) when whole-project scale is what finds the bugs: dispatch and
ABI breadth, deep inheritance, assembly-heavy libraries, EIP-170 pressure.
Keep writing minimal in-repo `tests/foundry/` projects or `run-call` UI tests
for anything that can be reduced: external projects never run in CI, and a
reduced regression test must land with any fix they surface. Skip entries
require a reason; sustained divergences graduate to
[SOLC_DIVERGENCE.md](../../docs/SOLC_DIVERGENCE.md).

Use Forge v0.3.0 on `PATH` for Solmate: newer versions reject its `testFail*`
cases before running them. The other projects use a current Forge; OpenZeppelin
requires Osaka support. Solady keeps transaction isolation disabled, as its
ETH mover tests rely on several calls sharing one transaction.

OpenZeppelin's ERC7579 tests also read the two EntryPoint and SenderCreator
bytecode files from `node_modules/hardhat-predeploy/bin` in its checkout.
Prepare `hardhat-predeploy@1.0.1` using the version and integrity in that
checkout's lockfile. The runner grants read access to that directory.
A green differential run can still contain tests that fail under both
compilers; check the reported baseline failures before claiming full coverage.

Environment variables:

- `SOLAR_FOUNDRY_PROJECT`: run one curated project (same as the positional
  `name` argument).
- `SOLAR_FOUNDRY_EXTERNAL_MANIFEST`: path to a TOML manifest that replaces the
  curated list, for out-of-repo projects. Entries are `[[project]]` tables
  with `name` plus either `repo` and `rev` (fetched) or `path` (an existing
  local directory, resolved relative to the manifest). Optional keys: `mode`
  (`"test"` or `"build"`), `profile` (Foundry profile for both compiler legs),
  `solc_version` (emulated solc version for the compiler leg, needed when
  sources pin an exact `pragma solidity`), `skip_tests`, `skip_contracts`
  (arrays of `{ pattern, reason }`; the reason is mandatory), and `notes`.
- `SOLAR_FOUNDRY_REPORT_DIR`: also write per-project JSON reports.
- `SOLAR_FOUNDRY_COMPILER`: use this Solar executable instead of the latest
  local build. Relative paths start at the workspace root. A debug build
  enables MIR and EVM IR validation by default.
- `SOLAR_FOUNDRY_OPTIMIZATION`: compile the Solar leg with `none`, `gas`, or
  `size` while keeping the project's solc optimizer settings. This lets the
  unoptimized Solar leg run projects that need solc's optimizer to compile.
