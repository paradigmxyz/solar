# Solar Zed Extension

Solidity language support for [Zed](https://zed.dev) powered by the [Solar](https://github.com/paradigmxyz/solar) language server.

## Features

- Syntax highlighting with tree-sitter
- Code completion and diagnostics via Solar LSP
- Automatic Solar binary installation
- Code formatting with `forge fmt` (if installed)
- Cross-platform support

## Installation

For local development, install this directory as a dev extension:

```bash
rustup target add wasm32-unknown-unknown
```

1. Open Zed.
2. Run `zed: install dev extension` from the command palette.
3. Select `editors/zed`.

The extension first looks for `solar` on `PATH`. If it is not found, it
downloads the latest released `solar` binary for the current platform from the
GitHub release artifacts.

## Formatting

This extension automatically configures `forge fmt` as the default Solidity formatter if [Foundry](https://getfoundry.sh) is installed on your system.

- **Format on save**: Enable in Zed settings with `"format_on_save": "on"`
- **Manual formatting**: Use `Cmd+Shift+I` (macOS) or `Ctrl+Shift+I` (Linux/Windows)
- **No Foundry?**: Formatting will be disabled if `forge` is not found (no errors)
- **Custom formatter**: Override in your Zed settings if you prefer a different formatter

To disable formatting entirely, add this to your Zed settings:
```json
{
  "languages": {
    "Solidity": {
      "formatter": null
    }
  }
}
```

## License

Dual licensed under MIT or Apache-2.0.
