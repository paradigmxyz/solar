Scripts and tools for fuzzing the compiler.

Run commands from the repository root unless a tool says otherwise.

Convenience entrypoints live under `fuzz/bin/`:

- `fuzz/bin/solsmith`: generates typed Solidity runtime harnesses.
- `fuzz/bin/solreduce`: reduces a replayable runtime mismatch.
- `fuzz/bin/solsymdiff`: automatically scans a contract's supported pure
  function surface for bounded, replay-confirmed Solc-vs-Solar runtime
  differences.

See `fandango/` for the combined Fandango + Foundry fuzzing flow. Fandango
and SolSmith generate ABI values and Solidity programs; Foundry's builtin
fuzzer executes generated targets with cheatcodes.

The Foundry differential path follows the same oracle shape as the builtin
fuzzer/cheatcode approach: it installs the solc and compiler runtimes at
different addresses, fuzzes calls through Foundry, records logs and state diffs
with `vm.recordLogs()` and `vm.startStateDiffRecording()`, then compares
success/revert status, returndata, normalized logs, and normalized state
side effects.

For the symbolic lane, start with:

```bash
fuzz/bin/solsymdiff \
  --source path/to/Target.sol \
  --contract Target
```

It requires `solc`, a symbolic-enabled `forge`, `anvil`, and Z3 on `PATH`.
Omit `--signature` for the automatic contract campaign; add
`--signature 'probe(uint256)'` to focus one function. See
`fandango/README.md` for supported scope, bounds, result statuses, artifacts,
and replay.
