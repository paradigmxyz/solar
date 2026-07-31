# solar-tester

Integration test support for the compiler.

`crates/solar/tests.rs` passes the freshly built `solar` binary to the UI,
MIR, EVM IR, standard JSON, upstream compatibility, and Foundry test runners.
Running the test entry point without a mode runs all suites. Run individual
suites through the `cargo tq` aliases, such as `cargo tq ui` or
`cargo tq solc-solidity`.

Run only the Foundry suite with:

```console
cargo tq foundry
```
