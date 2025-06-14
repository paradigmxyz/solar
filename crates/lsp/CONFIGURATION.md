# Solar LSP Configuration

The Solar Language Server Protocol (LSP) implementation provides a fast and feature-rich language server for Solidity development. This document outlines the available configuration options that can be used to customize the server's behavior.

## General Configuration

To configure the Solar LSP server, you'll need to set configuration options through your editor's LSP client. The exact method varies by editor:

- **VS Code**: Use settings.json or the settings UI
- **Neovim**: Configure through your LSP client setup (e.g., nvim-lspconfig)
- **Emacs**: Configure through your LSP client (e.g., lsp-mode, eglot)
- **Vim**: Configure through your LSP plugin (e.g., vim-lsp, coc.nvim)

Please consult your editor's documentation for specific instructions on how to configure language servers.

## Configuration Options

### `solar.workspaceRoot`
- **Type**: `string | null`
- **Default**: `null`
- **Description**: Explicitly sets the root path of the workspace. When `null`, the server will use the workspace folder provided by the LSP client during initialization.

### `solar.maxConcurrentRequests`
- **Type**: `number | null`
- **Default**: `4`
- **Description**: Maximum number of concurrent requests the server will handle. Higher values may improve responsiveness for large projects but will use more system resources.

### `solar.loggingLevel`
- **Type**: `string | null`
- **Default**: `"info"`
- **Description**: Controls the verbosity of server logging. Valid values are `"trace"`, `"debug"`, `"info"`, `"warn"`, and `"error"`. Lower levels include all higher level messages.

### `solar.enableSemanticTokens`
- **Type**: `boolean | null`
- **Default**: `true`
- **Description**: Enables semantic syntax highlighting. When enabled, the server provides detailed token information that allows editors to apply more precise syntax highlighting based on semantic meaning rather than just syntax patterns.

### `solar.enableCompletion`
- **Type**: `boolean | null`
- **Default**: `true`
- **Description**: Enables code completion suggestions. When enabled, the server provides intelligent autocompletion for Solidity keywords, functions, variables, and other language constructs.

### `solar.enableHover`
- **Type**: `boolean | null`
- **Default**: `true`
- **Description**: Enables hover information. When enabled, hovering over symbols in the editor will display detailed information such as type signatures, documentation, and other relevant details.

## Example Configuration

### VS Code (settings.json)
```json
{
  "solar.maxConcurrentRequests": 8,
  "solar.loggingLevel": "debug",
  "solar.enableSemanticTokens": true,
  "solar.enableCompletion": true,
  "solar.enableHover": true
}
```

### Neovim (Lua)
```lua
require('lspconfig').solar.setup({
  settings = {
    solar = {
      maxConcurrentRequests = 8,
      loggingLevel = "debug",
      enableSemanticTokens = true,
      enableCompletion = true,
      enableHover = true
    }
  }
})
```

## Notes

- Configuration changes may require restarting the language server to take effect
- Some editors may cache configuration values, requiring an editor restart
- Invalid configuration values will fall back to their default values
- The server will log configuration parsing errors to help diagnose issues