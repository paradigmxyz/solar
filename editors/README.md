# Editor Integrations

This directory contains editor integrations for the compiler.

The extensions are thin wrappers around `solar lsp`; the language server
implementation lives in `crates/lsp`. They are kept outside the Rust workspace
so the normal compiler build and test commands do not build editor tooling.

## Prerequisites

Build or install the `solar` binary before using the editor integrations:

```bash
cargo build -p solar-compiler --bin solar
```

For local development, either add `target/debug` to `PATH` or configure the
extension to point at the built binary.

## VS Code

Open the VS Code extension project and run it in an Extension Development Host:

```bash
cd editors/vscode
npm install
npm run compile
code .
```

Press `F5` in VS Code and open a Solidity file in the new Extension Development
Host window. If `solar` is not on `PATH`, configure:

```json
{
  "solarLsp.serverPath": "/absolute/path/to/solar/target/debug/solar"
}
```

The VS Code extension can also format Solidity files with `forge fmt` when
Foundry is installed.

## Zed

Install the WebAssembly target used by Zed extensions:

```bash
rustup target add wasm32-unknown-unknown
```

Install the Zed extension as a dev extension:

1. Open Zed.
2. Run `zed: install dev extension` from the command palette.
3. Select `editors/zed`.

The Zed extension first looks for `solar` on `PATH`. If it is not found, it
downloads the latest released `solar` binary for the current platform from the
GitHub release artifacts.

The Zed extension configures `forge fmt` as the Solidity formatter when Foundry
is available.
