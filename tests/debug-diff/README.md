# Debug-info differentials

Run the compiler's debug artifacts through the `soldb debug-diff` command:

```console
cargo build --manifest-path /path/to/soldb/Cargo.toml -p soldb
SOLDB=/path/to/soldb/target/debug/soldb cargo tq debug-diff
```

Use the `feat/debug-diff` soldb branch containing the comparison hardening,
checkpoint, and `run --save-trace` changes. This suite is opt-in because that
tool is not a compiler dependency or a released CI prerequisite yet. The
default solc is the one on `PATH`; `--solc /path/to/solc` selects another build.
Use the version pinned in `.github/workflows/ci.yml` for reproducible results.
Put runner options after `--`, for example `cargo tq debug-diff -- --mode steps`.

Each case is compiled with `none`, `gas`, and `size` optimization. Both
compilers receive identical Standard JSON source contents and EVM settings.
`soldb run` deploys each creation program and executes the call in local REVM,
then saves the complete trace. No Anvil, RPC, or funded account is needed.

Two comparisons run for each case. The `formats` comparison uses all source
stops to check our ETHDebug and legacy source-map outputs against the same
execution. The `solc` comparison checks explicit executable source lines,
identified by `// debug-check: name` markers. Both sides must reach every
checkpoint; a missing checkpoint is a failure, even if both sides omit it.
The default coverage mode ignores stop order and repetition. This does not
assert equality of every source span or reconstructed call frame.

The baseline suite covers creation, arithmetic, both branch arms, and a
storage read, with exact call-result assertions. `--comparison formats`
runs without solc. `--mode steps` compares ordered stops, and `--mode spans`
also compares their precise ranges and modifier depth. `--all-stops` removes
the checkpoint filter from the solc leg, including declaration/prologue stops
that differ across the compilers.

All inputs, compiler outputs, adapter artifacts, execution traces, comparison
reports, versions, and settings remain under `target/debug-diff/`. A command
failure or comparison difference exits nonzero; there is no blessing or
automatic acceptance of differences. Change the location with `--output`.
Reports are regression evidence for debug information, not a proof of semantic
equivalence. Use `fuzz/bin/solsymdiff` for compiler semantic differentials.

## Known compiler gaps

The initial integration also found missing source metadata around `require`
and a return of an unchanged argument. These cases are kept separately from
the passing baseline and are not marked as passing or silently skipped:

```console
SOLDB=/path/to/soldb/target/debug/soldb cargo tq debug-diff -- \
  --suite tests/debug-diff/known-gaps.json \
  --output target/debug-diff-known-gaps
```

At base `0ecceaed9`, the successful unoptimized `checked` call misses the
return checkpoint. Its `gas` and `size` versions, including the revert path,
have no source steps in either emitted format. This command therefore fails
and preserves the exact reports. Full solc comparisons with `--all-stops`
also report declaration stops that we do not currently emit. These are
compiler debug-info differences; the runner must continue to expose them.
