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
