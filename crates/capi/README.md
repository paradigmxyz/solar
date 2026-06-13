# solar-capi

Compiler C API and soljson-compatible JavaScript wrapper.

This crate builds the compiler as a `cdylib` and exposes a
Solidity-compatible C API. The same ABI is used by the WebAssembly build and
the JavaScript wrapper.

The public ABI is documented in
[`include/libsolc.h`](include/libsolc.h). Keep that header as the source of
truth for function signatures, callback behavior, and memory ownership rules.

For JavaScript and browser use, build the wasm distribution from the repository
root:

```bash
bash scripts/wasm/dist-wasm.sh
```

That produces `target/dist/solar-wasm.tar.gz`, containing the packed
`soljson.js`, the raw `solar.wasm`, and `soljson-wrapper.js`.
