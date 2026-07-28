# Solar VS Code Extension

Solidity language support for [Code](https://code.visualstudio.com/) powered by the [Solar](https://github.com/paradigmxyz/solar) language server.

## Local development

Build the `solar` binary from the repository root:

```bash
cargo build -p solar-compiler --bin solar
```

Then compile and run the extension:

```bash
cd editors/vscode
npm install
npm run compile
code .
```

Press `F5` in VS Code to open an Extension Development Host, then open a
Solidity file in that new window.

If `solar` is not on `PATH`, configure the extension with an absolute path to
the local binary:

```json
{
  "solarLsp.serverPath": "/absolute/path/to/solar/target/debug/solar"
}
```

Formatting uses `forge fmt`, so install Foundry or disable
`solarLsp.formatOnSave`.

## CodeLens

CodeLens annotations are enabled by default for function selectors, reference
counts, and contract inheritance. Select a selector to copy it, a reference
count to open Peek References, or inheritance information to open Type
Hierarchy.

Use `solarLsp.codeLens.enable` to disable all annotations, or configure
`solarLsp.codeLens.selectors`, `solarLsp.codeLens.references`, and
`solarLsp.codeLens.inheritance` separately. These settings are sent when the
language server starts, so changing one restarts the client. Lenses refresh
after the workspace is reanalyzed. Test and run actions are not yet supported.

## License

Dual licensed under MIT or Apache-2.0.
