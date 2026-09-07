# LSP cleanup log

Starting point: `origin/main` at `1b8272b4`, on `dani/lsp-cleanup`.
The prior `dani/symbolic-audit` branch remains intact.

Scope: every file in `crates/lsp`, `benches/lsp`, and `tools/lsp-bench`,
plus LSP configuration, CLI integration, integration tests, and workflows.
Review reuse, simplification, efficiency, and abstraction boundaries. Keep
protocol behavior, benchmark IDs, workloads, and timing boundaries stable.

## Baseline and review

- Started the existing Criterion suite with the dev profile, ten samples,
  one-second warmup and measurement, saving baseline `lsp-cleanup-main`.
  Build and measurement output: `target/lsp-cleanup-baseline.log`.
- Cargo Crap is not installed. Skipped its complexity-only scan; no tool or
  coverage installation is needed for this cleanup.
- The simplify skill's reuse, simplification, and efficiency reviews run in
  separate agents; the primary agent reviews abstraction boundaries.

## Shared LSP text and range helpers

- Moved identical point-aware containment, range ordering, range size, and
  byte-range conversion helpers into `proto`; kept completion's inclusive
  endpoint rule separate.
- Reused one pre-sized rope copy in code actions, folding, NatSpec completion,
  request handling, and the VFS source cache. The legacy comparison benchmark
  keeps its private copy so it does not need a new public API.
- All 1,065 LSP tests passed (one skipped). Scoped Clippy and typecheck passed
  with an existing `drop_non_drop` warning in a workspace test. Formatting ran
  with the configured toolchain; its stable rustfmt ignores nightly options.
- Cargo repaired a stale indexmap reference in Cargo.lock during the baseline
  build. This incidental resolution change is excluded from cleanup commits.

## Benchmark configuration and report construction

- Reused dataclass context serialization and one protocol builder in the
  Python adapter. Kept artifact validation and statistical rules intact.
- Reused the cross-server runner's YAML/schema helpers, merged identical
  path-validation arms, and looked up each workload's fixture once.
- Reused the existing fixture test builder in three tests.
- The Python suite passed 101 tests with six Node-dependent skips; Node is
  absent. Ruff format and lint passed. Both Rust packages passed 1,200 tests
  with one skip, and scoped Clippy/typecheck passed with the existing warning.

## Benchmark process handling

- Reused the writer's close operation at each failure path and the existing
  artifact digest checker, preserving diagnostic text.
- Moved finished observations out of the process instead of cloning retained
  traces, and borrowed configuration request items instead of cloning JSON.
- All 135 benchmark harness tests passed after the final ownership changes.
  Review confirmed process cleanup never reads the moved observations.
