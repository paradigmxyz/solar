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
