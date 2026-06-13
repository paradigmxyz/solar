# solar

[![Crates.io](https://img.shields.io/crates/v/solar-compiler.svg)](https://crates.io/crates/solar-compiler)
[![Downloads](https://img.shields.io/crates/d/solar-compiler)](https://crates.io/crates/solar-compiler)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](/LICENSE-MIT)
[![Apache-2.0 License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](/LICENSE-APACHE)
[![Actions Status](https://github.com/paradigmxyz/solar/workflows/CI/badge.svg)](https://github.com/paradigmxyz/solar/actions)
[![Telegram Chat](https://img.shields.io/endpoint?color=neon&logo=telegram&label=chat&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fparadigm%5Fsolar)][tg-url]

Blazingly fast, modular and contributor friendly Solidity compiler, written in Rust.

<p align="center">
    <picture align="center">
        <img alt="Solar cover" src="/assets/cover.png">
    </picture>
</p>

## Features and Goals

> [!CAUTION]
> Solar is under active development and is not yet feature complete.
> Use it to speed up your development workflows and tooling.
> Please do not use it in production environments.

- ⚡ Instant compiles and low memory usage ([benchmarks](./benches))
- 🔍 Expressive and useful diagnostics
- 🧩 Modular, library-based architecture
- 💻 Simple and hackable code base
- 🔄 Compatibility with the latest Solidity language breaking version (0.8.*)

<p align="center">
    <picture align="center">
        <img alt="Terminal screenshot showing Solar is 40x faster than solc at generating ABI using hyperfine" src="/assets/benchmark.png">
    </picture>
</p>

## Getting started

Solar is available through a command-line interface, or as a Rust library.

### Library usage

You can add Solar to your Rust project by adding the following to your `Cargo.toml`:

```toml
[dependencies]
solar = { version = "=0.1.8", package = "solar-compiler", default-features = false }
```

Or through the CLI:

```bash
cargo add "solar-compiler@=0.1.8" --rename solar --no-default-features
```

You can see examples of how to use Solar as a library in the [examples](/examples) directory.

### Binary usage

Pre-built binaries are available for macOS, Linux and Windows on the [releases page](https://github.com/paradigmxyz/solar/releases)
and can be installed with the following commands:
- On macOS and Linux:
    ```bash
    curl -LsSf https://paradigm.xyz/solar/install.sh | sh
    ```
- On Windows:
    ```powershell
    powershell -c "irm https://paradigm.xyz/solar/install.ps1 | iex"
    ```
- For a specific version:
    ```bash
    curl -LsSf https://paradigm.xyz/solar/v0.1.8/install.sh | sh
    powershell -c "irm https://paradigm.xyz/solar/v0.1.8/install.ps1 | iex"
    ```

You can also use [`cargo binstall`](https://github.com/cargo-bins/cargo-binstall):
- Latest version:
    ```bash
    cargo binstall solar-compiler
    ```
- For a specific version:
    ```bash
    cargo binstall solar-compiler@0.1.8
    ```

Or build Solar from source:
- From crates.io:
    ```bash
    cargo install solar-compiler --locked
    ```
- From GitHub:
    ```bash
    cargo install --git https://github.com/paradigmxyz/solar --locked
    ```
- From a Git checkout:
    ```bash
    git clone https://github.com/paradigmxyz/solar
    cd solar
    cargo install --locked --path crates/solar
    ```

Once installed, check out the available options:

```bash
solar -h
```

Here's a few examples:

```bash
# Compile a single file and emit ABI to stdout.
solar Counter.sol --emit abi

# Compile a contract through standard input (`-` file).
echo "contract C {}" | solar -
solar - <<EOF
contract HelloWorld {
    function helloWorld() external pure returns (string memory) {
        return "Hello, World!";
    }
}
EOF

# Compile a file with a Foundry project's remappings.
solar $(forge re) src/Contract.sol
```

### C API, WASM, and JavaScript usage

The `solar-capi` crate exposes the compiler through a Solidity-compatible C API.
That same ABI is also the boundary used by the WebAssembly build and the
soljson-compatible JavaScript wrapper.

The C header is the source of truth for the ABI:
[`crates/capi/include/libsolc.h`](/crates/capi/include/libsolc.h). The API
accepts Standard JSON input and returns Standard JSON output, matching the
interface used by Solidity's `libsolc` and raw `soljson.js` builds. Like
Solidity, client code owns memory explicitly through the C API described in the
header.

Solidity distributes JavaScript compiler builds as `soljson.js` files. Modern
builds are WebAssembly modules loaded through a JavaScript wrapper, with the
wasm bytes made available as `Module.wasmBinary`. The JavaScript package
`solc-js` then layers a higher-level API over that raw module.

This repository ships the wasm distribution as `solar-wasm.tar.gz` in releases.
The archive contains:

- `soljson.js`: a packed, soljson-compatible JavaScript file with wasm embedded.
- `solar.wasm`: the raw WebAssembly module.
- `soljson-wrapper.js`: the JavaScript wrapper for loading `solar.wasm`
  yourself.

There are two ways to use the wasm and JavaScript distribution:

1. Download `solar-wasm.tar.gz` from a release and extract the file you need.
   Use `soljson.js` for the most solc-js-compatible path, or use
   `solar.wasm` with `soljson-wrapper.js` when you want to control wasm loading.

2. Build the same files from source:

```bash
rustup target add wasm32-unknown-unknown
bash scripts/wasm/dist-wasm.sh
```

This produces the same files under `target/dist/`. The script builds with an
exported, growable WebAssembly table so JavaScript callbacks can be installed,
then packs the wasm bytes into `soljson.js`.

Use the packed `soljson.js` directly:

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

For custom wasm loading, use `soljson-wrapper.js` from the release archive or
[`crates/capi/soljson.js`](/crates/capi/soljson.js) from source.

In Node, load the separate wasm bytes through the same `Module.wasmBinary`
hook used by the packed file:

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
assign `globalThis.Module = { wasmBinary }`, and then load the wrapper script.

The wrapper exposes the solc-js-style Standard JSON compile entry point plus
metadata helpers. Legacy low-level solc-js entry points are intentionally set to
`null`.

## Roadmap

You can find a more detailed list in the [pinned GitHub issue](https://github.com/paradigmxyz/solar/issues/1).

- [ ] Front-end
  - [x] Lexing
  - [x] Parsing
  - [ ] Semantic analysis
    - [x] Symbol resolution
    - [ ] Type checker
    - [ ] Static analysis
- [ ] Middle-end
- [ ] Back-end

## Semver Compatibility

Solar's versioning tracks compatibility for the binaries, not the API.
If using this as a library, be sure to pin the version with a `=` version requirement operator.

## Supported Rust Versions (MSRV)

Solar always aims to stay up-to-date with the latest stable Rust release.

The Minimum Supported Rust Version (MSRV) may be updated at any time, so we can take advantage of new features and improvements in Rust.

## Contributing

Contributions are welcome and highly appreciated. To get started, check out the
[**contributing guidelines**](/CONTRIBUTING.md).

## Support

Having trouble? Check out the existing issues on [**GitHub**](https://github.com/paradigmxyz/solar/issues),
or feel free to [**open a new one**](https://github.com/paradigmxyz/solar/issues/new).

You can also ask for help on [Telegram][tg-url].

[tg-url]: https://t.me/paradigm_solar

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
</sub>
