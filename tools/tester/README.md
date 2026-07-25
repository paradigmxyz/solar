# solar-tester

Integration test support for the compiler.

`crates/solar/tests.rs` passes the freshly built `solar` binary to the UI,
MIR, EVM IR, standard JSON, and upstream compatibility test runners. Run these
suites through the `cargo tq` aliases, such as `cargo tq ui` or
`cargo tq solc-solidity`.

`crates/solar/tests/it/foundry.rs` registers the Foundry tests and delegates
their implementation to `solar_tester::foundry`. Run them with:

```console
cargo test -p solar-compiler --test it foundry:: -- --test-threads=1
```
