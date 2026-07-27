# solar-tester

Integration test support for the compiler.

`crates/solar/tests.rs` passes the freshly built `solar` binary to the UI,
MIR, EVM IR, standard JSON, and upstream compatibility test runners. Run these
suites through the `cargo tq` aliases, such as `cargo tq ui` or
`cargo tq solc-solidity`.

The same entry point runs the Foundry suite when `TESTER_MODE=foundry`. Use:

```console
cargo tq foundry
```
