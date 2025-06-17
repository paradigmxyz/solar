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

- ⚡ Instant compiles and low memory usage
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
solar = { version = "=0.1.4", package = "solar-compiler", default-features = false }
```

Or through the CLI:

```bash
cargo add "solar-compiler@=0.1.4" --rename solar --no-default-features
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
    curl -LsSf https://paradigm.xyz/solar/v0.1.0/install.sh | sh
    powershell -c "irm https://paradigm.xyz/solar/v0.1.0/install.ps1 | iex"
    ```

You can also build Solar from source:
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
