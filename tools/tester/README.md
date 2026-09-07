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
