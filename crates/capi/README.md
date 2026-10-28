# solar-capi

Compiler C API and soljson-compatible JavaScript wrapper.

This crate builds the compiler as a `cdylib` and exposes a
Solidity-compatible C API. The same ABI is used by the WebAssembly build and
the JavaScript wrapper.

The public ABI is documented in
[`include/libsolc.h`](include/libsolc.h). Keep that header as the source of
truth for function signatures, callback behavior, and memory ownership rules.

The API accepts Standard JSON input and returns Standard JSON output, matching
Solidity's `libsolc` and raw `soljson.js` interfaces. Client code owns memory
explicitly through the C API described in the header.

## WebAssembly and JavaScript

Download `solar-wasm.tar.gz` from a [release](https://github.com/paradigmxyz/solar/releases),
or build it from the repository root:

```bash
rustup target add wasm32-unknown-unknown
scripts/wasm/dist-wasm.sh
```

The build produces `target/dist/solar-wasm.tar.gz`. Both distributions contain:

- `soljson.js`: a packed, soljson-compatible JavaScript file with wasm embedded.
- `solar.wasm`: the raw WebAssembly module.
- `soljson-wrapper.js`: the wrapper for loading the wasm yourself.

The build exports a growable WebAssembly table for JavaScript callbacks. The
wrapper exposes the solc-js-style Standard JSON compile entry point and metadata
helpers; unsupported legacy low-level entry points are `null`.

### Packed module

Use `soljson.js` directly in Node:

```js
const solar = require("./soljson.js");

const output = solar.compile(JSON.stringify({
  language: "Solidity",
  sources: {
    "A.sol": { content: 'import "B.sol"; contract A is B {}' },
  },
  settings: { outputSelection: { "*": { "*": ["abi"] } } },
}), {
  import(path) {
    if (path === "B.sol") {
      return { contents: "contract B {}" };
    }
    return { error: `source not found: ${path}` };
  },
});
```

### Separate wasm file

Use `soljson-wrapper.js` from the archive, or [`soljson.js`](soljson.js) from
source, to load the wasm through `Module.wasmBinary`:

```js
const fs = require("node:fs");

globalThis.Module = {
  wasmBinary: fs.readFileSync("./solar.wasm"),
};
const solar = require("./soljson-wrapper.js");
delete globalThis.Module;

const output = solar.compile(JSON.stringify({
  language: "Solidity",
  sources: {
    "A.sol": { content: 'import "B.sol"; contract A is B {}' },
  },
  settings: { outputSelection: { "*": { "*": ["abi"] } } },
}), {
  import(path) {
    if (path === "B.sol") {
      return { contents: "contract B {}" };
    }
    return { error: `source not found: ${path}` };
  },
});
```

In browsers, serve `solar.wasm` and `soljson-wrapper.js`, fetch the wasm bytes,
assign `globalThis.Module = { wasmBinary }`, then load the wrapper script.
