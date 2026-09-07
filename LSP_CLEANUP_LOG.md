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
- The Python suite ran 101 tests: 95 passed and six Node-dependent tests
  were skipped because Node is absent. Ruff format and lint passed. Both Rust packages passed 1,200 tests
  with one skip, and scoped Clippy/typecheck passed with the existing warning.

## Benchmark process handling

- Reused the writer's close operation at each failure path and the existing
  artifact digest checker, preserving diagnostic text.
- Moved finished observations out of the process instead of cloning retained
  traces, and borrowed configuration request items instead of cloning JSON.
- All 135 benchmark harness tests passed after the final ownership changes.
  Review confirmed process cleanup never reads the moved observations.

## Review coverage and measurement follow-up

- Completed full-file reads across LSP production modules, tests and client
  fixtures, Criterion support, both benchmark runners and their fixtures,
  manifests, schemas, adapter patch, and LSP benchmark workflows. Also checked
  the CLI/configuration entrypoints and compiler integration tests.
- Kept parser recovery paths, hierarchy compatibility rules, and workflow
  validation boundaries separate where they have different semantics.
- The first full Criterion comparison retained all 37 benchmark IDs. Some
  results moved in both directions, including unchanged kernels. Retained
  per-case results in `target/lsp-cleanup/first-comparison.json`.
- Built retained main/candidate binaries in this checkout for alternating
  measurements; restored candidate sources from byte-for-byte backups after
  building main. Used the exact starting SHA to avoid an unrelated local
  branch named `origin/main`.
- Other compiler work runs on this host. Follow-up measurements alternate
  binaries, then pin them to one CPU; no project builds run during the final
  pinned comparison.

## Pinned performance check

Used the dev profile, CPU 0, main/candidate/candidate/main order, 20 samples
per run, and one-second warmup and measurement targets. Values below average
the two per-run means; they are local checks, not release-performance claims.

| Benchmark | Main (ns) | Candidate (ns) | Change |
| --- | ---: | ---: | ---: |
| `lsp_analysis-build/256` | 63829796.0 | 63287936.0 | -0.85% |
| `lsp_analysis-build/64` | 16869015.7 | 16809159.3 | -0.35% |
| `lsp_analysis-build/repeated-calls` | 9511651.7 | 9344020.8 | -1.76% |
| `lsp_incremental-analysis/cold` | 64754082.6 | 65196280.1 | +0.68% |
| `lsp_incremental-analysis/reverted-edit` | 47136.6 | 47625.2 | +1.04% |
| `lsp_incremental-analysis/unchanged` | 3976.9 | 3964.0 | -0.32% |
| `lsp_symbol-table-aggregation/1` | 559.2 | 646.3 | +15.56% |
| `lsp_symbol-table-aggregation/4` | 7091775.7 | 7182902.4 | +1.28% |
| `lsp_workspace-discovery/foundry-10k-import-only` | 6193974.2 | 5746037.3 | -7.23% |
| `lsp_workspace-path-containment-query/16-workspaces-containment-query` | 8438.2 | 8840.1 | +4.76% |
| `lsp_workspace-path-queries/16-workspaces-1024-queries` | 11452468.9 | 11532152.0 | +0.70% |
| `lsp_workspace-path-single-query/16-workspaces-single-query` | 40241.2 | 39276.5 | -2.40% |

The unchanged single-batch aggregation control had one noisy candidate run
(765.7 ns versus 526.8 ns in the other candidate run). The unchanged containment
control also shifted. Retained both results and ran a focused control recheck.
Full raw samples and estimates remain under `target/criterion`; comparison
JSON and run logs are under `target/lsp-cleanup`.

The 30-sample control recheck used two-second measurement targets on CPU 0:
- `lsp_symbol-table-aggregation/1`: 656.6 ns main, 622.3 ns candidate (-5.24%).
- `lsp_workspace-path-containment-query/16-workspaces-containment-query`: 8622.0 ns main, 8761.6 ns candidate (+1.62%).

The affected analysis, query, and collection workloads show no material
slowdown in these local checks. Unchanged microbenchmarks still vary; do not
interpret the shared-host dev measurements as precise release-speed changes.

## Workspace collection and unused path operations

- Shared eager/flycheck collection and publication between single-workspace
  and multi-workspace refresh. Cancellation still leaves prior state intact,
  and multi-workspace refresh still collects every result before publishing.
- Used map-entry loading for cached Foundry configuration, preserving cached
  errors while removing repeated lookups and a path clone.
- Removed unused VFS path-manipulation methods and their private helper chain.
  Kept path representation, normalization, formatting, ordering, and equality.
- Removed an unnecessary test closure drop. Final validation ran 1,200 Rust
  tests successfully (one skipped); formatting and scoped Clippy/typecheck
  passed. Python checks ran 101 tests (95 passed, six Node-dependent skips),
  and Ruff formatting/lint passed.

## Analysis, rendering, and benchmark support

- Reused the HIR type visitor and derived analysis-path clone instead of
  maintaining copies of those implementations.
- Shared plain-text and Markdown NatSpec traversal, preserving snapshots and
  allocation behavior.
- Shared benchmark path-query execution and edit-source lookup. Index building
  and document conversion remain on their original sides of timing boundaries.
- Covered by the final Rust suite, lint/typecheck, and recorded main/candidate
  measurements above.

## Shared test transport and request setup

- Shared nineteen identical paired LSP transports through `spawn_lsp_pair`.
  Buffer capacity, server/client spawn order, routers, task ownership, and
  shutdown behavior remain unchanged. Raw one-sided protocol tests stay local.
- Reused completion-change and selection-range setup in request fixtures.
- Kept the existing manual file-read helper because the repository disallows
  `fs::read_to_string`; the shorter replacement introduced a lint warning.
- The final Rust tests and warning-free scoped Clippy run cover these changes.

## Focused runner and fixture cleanup

- Shared setup/measured phase classification and result storage, keeping exact
  errors, crash precedence, setup prefixes, and fallback observations.
- Shared probe anchor preparation and document capability checks. Request order,
  validation order, and measured request boundaries remain unchanged.
- Centralized optional step probes and fixture file traversal, retaining path
  validation, ignored directories, and symlink checks.
- Shared seven runner fixture setups and 21 integration CLI invocations.
- Added phase-result coverage within the existing test module. All 1,202 scoped
  Rust tests passed (one skipped); scoped Clippy/typechecking passed, and all
  37 Criterion benchmark preflights succeeded. Formatting and diff checks passed.
- Reuse, simplification, efficiency, and altitude reviews cover this pass.
  Cargo Crap remains unavailable, so the complexity-only scan was skipped.
  No new timing comparison was run; preflights verify workloads, not speed.

## Process ownership and report cleanup

- Workers return bounded output buffers through their join handles, removing
  shared mutex buffers and a final stderr copy. All workers still share the
  original drain deadline; timeout and panic paths remain failures.
- Shared process-group/cgroup setup and descendant termination. Removed an
  unnecessary boxed stdout reader.
- Shared lifecycle result-to-check conversion and report metric insertion;
  escaped Markdown row fields are now formatted once per group.
- The scoped Rust tests, Clippy/typecheck, formatting, and benchmark preflights
  above cover this change, including a worker result/panic test.

## Benchmark construction and Python runner cleanup

- Removed the single-use source builder and stored constant analysis epoch.
  Generated source strings, anchor order, and query positions remain the same.
- Shared open-document preparation across four benchmark constructors.
- Shared config/result artifact reads and validation while retaining validation
  stages and error order. Removed derived verdict state and unreachable checks.
- Shared workflow job, step, and script extraction between existing test suites.
  Kept distinct subprocess mocks explicit to keep their trust inputs visible.
- Python discovery ran 101 tests: 95 passed, six Node-dependent cases skipped.
  Ruff lint passed; changed regions were formatted without unrelated test churn.
  Rust checks and all 37 benchmark preflights passed as recorded above.
