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

The current suite measures in-memory project analysis, edits, and queries. Loading manifests and
corpora from disk, resolving anchors, constructing requests, and preflight correctness checks stay
outside the timed closure. Use stable `lsp/<operation>/<case>` names and add new cases instead of
renaming existing benchmark IDs.

To add a scenario, prepare the project outside the timed closure, resolve its request anchors, run
the request once and assert the expected response, then register only the analysis, edit, or query
as the measured operation. Full filesystem, JSON-RPC, and process latency belongs in a future
walltime benchmark.

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
  request with fresh document caches. The edited case appends whitespace while retaining analysis,
  exercising signature help before reanalysis. Preparing and destroying snapshots is untimed.
- `single-workspace-reverted-edit` measures applying and undoing an edit followed by a complete
  production analysis epoch, including dependency validation and publication. `single-workspace-open-indexed`
  opens a disk-identical root after initial indexing; setup and destruction are untimed.
  Both cover 256 generated callers with a disk import and the tracked Unifap router's import
  closure copied beneath an excluded `lib/` directory. `single-workspace-cold`,
  `single-workspace-changed`, and `single-workspace-unchanged` provide first-analysis, changed-text,
  and unchanged-epoch controls. These include synchronous filesystem validation and compiler work;
  they exclude protocol transport, debounce, and blocking-pool scheduling.
