# solar-lsp

Solar LSP definitions and implementation.

## Workspace indexing

Workspace indexing discovers Solidity files throughout the project, independently of build
entry-point directories and open editor tabs. Dependencies are loaded through imports;
closing a file restores its disk contents without removing it from the project index.
Indexing exclusions still apply. Foundry settings supply import resolution, compiler options,
and build entry points for flycheck. Explicitly configured source directories remain included,
including directories outside the project root.

## Source change debounce

The server waits 150 ms after the latest source change before starting analysis. Each new
change restarts the wait. This applies to document edits, closes, and watched source-file
changes; opening a document starts analysis without this delay.

Set `initializationOptions.sourceChangeDebounce` to a non-negative integer in milliseconds
when starting the server. Use `0` to disable the wait. Missing or invalid values use 150 ms.
Restart the server to apply a new value.

```json
{
  "initializationOptions": {
    "sourceChangeDebounce": 150
  }
}
```

## Embedding

Use the public `solar_lsp::launch` entry point to run the same language server implementation
inside another Tokio application:

```rust,no_run
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let config = solar_lsp::LaunchConfig::default()
    .with_default_forge_path(std::env::current_exe()?);
solar_lsp::launch(config).await?;
# Ok(())
# }
```

An embedding executable that also provides Forge commands can use its own path as the default, as
shown above. Other hosts should supply the path to their Forge executable instead.

The caller owns the Tokio runtime and process-global setup. `launch` owns process stdin and stdout
until the LSP session exits, reserves stdout for JSON-RPC frames, and returns transport or protocol
errors to the caller. A client-provided `initializationOptions.forgePath` overrides the launch
default; when neither is configured, Forge is resolved as `forge` through `PATH`.

## Benchmarks

Run the LSP benchmarks locally with:

```console
cargo bench -p solar-lsp --bench lsp --bench lsp_diagnostic --features bench
```

These Criterion/CodSpeed benchmarks measure in-memory project analysis, edits, and queries. Loading
manifests and corpora from disk, resolving anchors, constructing requests, and preflight correctness
checks stay outside the timed closure. Use stable `lsp/<operation>/<case>` names and add new cases
instead of renaming existing benchmark IDs.

To add a scenario, prepare the project outside the timed closure, resolve its request anchors, run
the request once and assert the expected response, then register only the analysis, edit, or query
as the measured operation. The pending-request benchmark below covers analysis scheduling and
waiting; JSON-RPC transport and process latency remain outside these in-process benchmarks.

The benchmark groups intentionally keep separate timing boundaries:

- `analysis-build` preserves the historical single-source workload for comparable BASE results.
- `project-analysis` and `project-analysis-after-edit` measure compiler and symbol-table rebuilds.
- `project-edit-application` measures UTF-16 document edit application without analysis.
- `symbol-table-queries` measures synchronous query kernels, not complete LSP request latency.
- `call-hierarchy-request` measures warm prepare, incoming, and outgoing requests through the
  production handlers on the tracked Unifap project. Parameters are cloned into owned requests;
  snapshot lookup, response construction, and response destruction are timed. Transport, JSON
  encoding, analysis, and waiting for in-progress analysis are excluded.
- `call-hierarchy-expand` follows the router's transfer helper to both liquidity callers, expands
  their outgoing calls, and follows the helper to the ERC20 interface. Results are per five-request
  burst. Preflight checks pin callable identities, grouped call-site ranges, and expanded targets.
- `call-hierarchy-first-request` includes lazy query-index construction in the first prepare after
  analysis. Cloning the semantic snapshot and destroying the snapshot and response are untimed.
  Compare this group against its own baseline, separately from repeated request latency.
- `code-lens` measures repeated queries, including response destruction. The separate
  `code-lens-first-request` group clones an unqueried analysis snapshot outside timing and
  includes the first query's reference-count initialization; snapshot and response destruction
  stay outside timing. Compare each group's results against its own baseline.
- `open-document-selection-range` measures repeated selection queries through the VFS snapshot,
  including UTF-16 conversion and response construction. It covers start, middle, and end positions
  in an unchanged document and multiple cursors, excluding transport and blocking-pool scheduling.
- `open-document-selection-range-cold` includes the first request's parsing and index construction;
  preparing and destroying the open document stays outside timing.
- `rename` includes the production handler, source validation, blocking task, and edit construction.
  Its `optimism-predeploys` case renames the `getName` argument at all 31 occurrences in the original
  self-contained Predeploys module extracted from `testdata/Optimism.sol`. The corresponding
  `project-analysis/optimism-predeploys` case measures fresh analysis. These cover one real module;
  the full flattened Optimism corpus contains conflicting dependency versions and is used only for
  source-level workloads, such as folding and selection ranges.
- `signature-help` measures repeated requests at one cursor through the production handler.
  `signature-help-moving-cursor` cycles through arguments and calls in generated contracts and the
  tracked Unifap router, including snapshot lookup, position conversion, and response destruction.
  Results are per burst; throughput counts requests. Transport and compiler analysis are excluded.
- `signature-help-first-request` and `signature-help-first-after-edit` measure an early or late
  request with fresh document caches and current analysis. The edited case appends whitespace and
  reanalyzes the project before timing starts, retaining its compiler options and dependency
  overlays. These groups exclude reanalysis, scheduling, and edit-to-response latency; preparing
  and destroying snapshots is untimed. Historical pre-reanalysis results are not comparable with
  the edited cases after this freshness change.
- `single-workspace-reverted-edit` measures applying and undoing an edit followed by a complete
  production analysis epoch, including dependency validation and publication. `single-workspace-open-indexed`
  opens a disk-identical root after initial indexing; setup and destruction are untimed.
  Both cover 256 generated callers with a disk import and the tracked Unifap router's import
  closure copied beneath an excluded `lib/` directory. `single-workspace-cold`,
  `single-workspace-changed`, and `single-workspace-unchanged` provide first-analysis, changed-text,
  and unchanged-epoch controls. These include synchronous filesystem validation and compiler work;
  they exclude protocol transport, debounce, and blocking-pool scheduling.

### Pending requests

Run the standalone walltime benchmark with:

```console
cargo bench -p solar-lsp --bench lsp_pending --features bench
```

Each sample starts immediately before the production `didChange` handler and ends when a
subsequent hover or definition request returns. It includes edit application, production analysis
scheduling, blocking-pool dispatch, compiler and symbol-table work, publication, and response
construction. The default 150 ms source-change debounce remains configured; the navigation request
can interrupt that wait through the normal interactive-analysis path. The benchmark does not
insert an artificial delay or subtract debounce from the result.

The four cases cover hover and definition on 256 generated functions and the tracked Unifap
router's import closure. Each edit alternately inserts or removes a leading newline in the queried
source. Every request must return `Pending` on its first poll, then match the expected response for
the edited source, including its shifted ranges. Initial analysis, temporary project setup,
request preparation, response validation and destruction, and worker cleanup are outside timing.
JSON-RPC encoding and transport are excluded. Compiler sessions use one thread and the runner
uses a Tokio current-thread runtime with the production blocking pool.
Each sample begins with an idle worker. Requests queued behind an already-running, superseded
analysis are not covered by these cases.

The runner prints JSON to stdout with raw `samples_ns` and nearest-rank `p50_ns`/`p95_ns` for each
case, plus the sample count, warmup count, compiler thread count, and configured debounce. Set
`SOLAR_LSP_BENCH_SAMPLES` to override the default 30 measured samples per case, or
`SOLAR_LSP_BENCH_WARMUP` to override the default five warmups. The sample count must be positive; zero
warmups are allowed. These percentiles describe the combined edit-to-response latency in one
local run, without attributing time to individual phases or establishing a statistical regression.
Run on an otherwise idle machine and retain the JSON when comparing revisions. This target is
separate from CodSpeed simulation and is not a CI merge gate.
