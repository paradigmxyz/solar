# solar-lsp

Solar LSP definitions and implementation.

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
cargo bench -p solar-lsp --bench lsp --features bench
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
