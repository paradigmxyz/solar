# Fresh completion analysis cost

A completion can only use new declarations once analysis publishes them. This
investigation measures that synchronous prerequisite separately from the warm
query. It follows the freshness work in [#1501](https://github.com/paradigmxyz/solar/pull/1501)
and does not change request scheduling or completion behavior.

## Reproduce

```sh
cargo bench -p solar-lsp --bench lsp --features bench -- \
  'lsp/fresh-completion' --sample-size 20 --warm-up-time 1 --measurement-time 3
```

`fresh-completion-after-edit` alternates a state variable and its reference
between `marker0` and `marker1`, publishes a production analysis epoch, and
queries the scope inside `target`. Every iteration asserts that exactly the new
name appears. A cached response cannot pass. `fresh-completion-warm` queries the
already-published snapshot without an edit or analysis.

Both groups keep the edited file fixed while adding 0, 8, or 32 unrelated
contracts, each with 64 functions. These contracts have no imports. The two
layouts place them either in the edited file's workspace or in separate sibling
workspaces. Initial analysis checks diagnostics, completion, and a callable in
every unrelated file, outside timing. Workspace discovery and fixture creation
also stay outside timing.

The edited measurement includes VFS replacement, disk source reads and dependency
validation, compiler work, symbol-table construction and aggregation, publication,
completion, response validation, and response destruction. It uses one compiler
thread. It excludes debounce, worker queueing, cancellation, JSON-RPC, and editor
rendering. These are local synthetic scaling measurements, not end-to-end latency,
p95 estimates, or a before/after optimization claim.

## Measurements

Measured on 2026-09-17 from `54f209af1` plus these benchmark changes, with
`rustc 1.96.1`, the repository's bench profile (optimized with debug information),
and an AMD EPYC 4585PX host. Two consecutive runs of the command above produced
these Criterion point estimates, rounded to milliseconds:

| Unrelated contracts | Layout | After edit, run A | After edit, run B |
| --- | --- | ---: | ---: |
| 0 | Same workspace | 0.087 ms | 0.091 ms |
| 8 | Same workspace | 5.202 ms | 4.393 ms |
| 8 | Separate workspaces | 1.075 ms | 1.142 ms |
| 32 | Same workspace | 16.456 ms | 19.508 ms |
| 32 | Separate workspaces | 5.306 ms | 7.520 ms |

Warm query point estimates were 0.199–0.272 microseconds across both runs and
all layouts. The host was shared, and the larger workloads show substantial
variation. The consistent result is growth with unrelated indexed code and a
lower cost when unchanged batches can be reused; these numbers do not establish
a precise speedup or isolate the contribution of each compiler phase. Profiling
runs are excluded from this table.

## Why unrelated files matter

`GlobalStateSnapshot::analysis_batches_cancellable` adds every discovered source
file to its workspace's batch. `run_analysis` can reuse unchanged batches across
workspaces, but a changed batch runs parsing, AST lowering, semantic analysis, and
`SymbolTables::build` together. There is no per-file or per-body semantic reuse in
this path.

Even separate workspaces are not free. The changed-epoch path validates cached
inputs and loader observations, clones reused batch outputs, merges symbol tables,
and publishes one aggregate. `latest_analysis` waits for that aggregate epoch;
reordering batches alone cannot release a waiting completion early.

This explains why warm completion measurements miss the cost. It also means that
simply moving the requested workspace to the front of the batch loop is not a
complete latency fix.

## Next implementation

Keep the freshness check. First separate a request's semantic snapshot from the
workspace-wide publication barrier: analyze the requested file and its transitive
imports at the captured input revision, then answer file-local requests from that
snapshot. Preserve the full index for workspace references, rename, and diagnostics;
a partial result must not replace it or mark the whole epoch complete.

The request snapshot needs the same compiler options, remappings, open dependency
overlays, and loader observations as normal analysis. Invalidation must cover
import changes, dependency edits, configuration changes, and failed or superseded
workers. Tests should keep a valid completion when an unrelated file has an error,
but reject a stale completion when an imported library changes. Completion modes
that suggest workspace-wide auto-imports need the full index as well as the local
snapshot, with explicit freshness rules.

After that boundary is correct, measure repeated body-only edits and share
unchanged syntax, declaration resolution, and body results. Reusing arena-owned
compiler nodes across sessions needs an ownership and invalidation design; adding
a file-text cache alone does not supply semantic reuse. A query database may help
later, but this experiment does not justify a compiler-wide rewrite.

Before claiming an end-to-end improvement, add a stdio workload that sends
`didChange` followed immediately by completion and checks the new name on every
response. Cover concurrent edits and cancellation, and report latency across
independent sessions. The existing warmed completion benchmark cannot substitute
for that workload.
