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

An embedding host that already selected a Foundry profile can pass it through with
`with_selected_profile("custom")`; the language server uses that profile when discovering workspace
sources and when running its automatically configured Forge lint checks.

When the host has already resolved Foundry workspace configuration (including `extends`), it can
provide the effective values directly:

```rust,no_run
# fn build_config() -> Result<(), Box<dyn std::error::Error>> {
# use solar_config::EvmVersion;
let workspace = std::env::current_dir()?.join("workspace");
let config = solar_lsp::LaunchConfig::default().with_foundry_workspace_config(
    solar_lsp::FoundryWorkspaceConfig::new(&workspace)
        .with_source_roots([workspace.join("src")])
        .with_flycheck_source_roots([workspace.join("src")])
        .with_include_paths([workspace.join("lib")])
        .with_evm_version(EvmVersion::Cancun),
);
# let _ = config;
# Ok(())
# }
```

The workspace root and path fields in this snapshot must be absolute. `LaunchConfig` validates and
lexically normalizes them without accessing the filesystem or resolving symlinks. Import
remappings are final `solar_config::ImportRemapping` values and are passed through unchanged; as
with `CompileOpts`, a relative remapping target is interpreted relative to the workspace base path.
The language server matches each snapshot only to the exact manifest directory, so multiple or
nested manifests remain isolated. The snapshot is launch-time state reused during rediscovery; the
host should build a new `LaunchConfig` for changed Foundry settings. An unmatched manifest
continues to use its local `foundry.toml` parser.

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
