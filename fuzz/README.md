Scripts and tools for fuzzing the compiler.

Run commands from the repository root unless a tool says otherwise.

Convenience entrypoints live under `fuzz/bin/`:

- `fuzz/bin/solsmith`: generates typed Solidity runtime harnesses.
- `fuzz/bin/solreduce`: reduces a replayable runtime mismatch.

See `fandango/` for the combined Fandango + Foundry fuzzing flow. Fandango
and SolSmith generate ABI values and Solidity programs; Foundry's builtin
fuzzer executes generated targets with cheatcodes.

The Foundry differential path follows the same oracle shape as the builtin
fuzzer/cheatcode approach: it installs the solc and compiler runtimes at
different addresses, fuzzes calls through Foundry, records logs and state diffs
with `vm.recordLogs()` and `vm.startStateDiffRecording()`, then compares
success/revert status, returndata, normalized logs, and normalized state
side effects.
