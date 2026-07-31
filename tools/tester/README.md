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
