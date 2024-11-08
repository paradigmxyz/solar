# solar

[![Crates.io](https://img.shields.io/crates/v/solar-compiler.svg)](https://crates.io/crates/solar-compiler)
[![Downloads](https://img.shields.io/crates/d/solar-compiler)](https://crates.io/crates/solar-compiler)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](/LICENSE-MIT)
[![Apache-2.0 License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](/LICENSE-APACHE)
[![Actions status](https://github.com/paradigmxyz/solar/workflows/CI/badge.svg)](https://github.com/paradigmxyz/solar/actions)

Blazingly fast, modular and contributor friendly Solidity compiler, written in Rust.

<p align="center">
    <picture align="center">
        <img alt="Terminal screenshot showing Solar is 40x faster than solc at generating ABI using hyperfine." src="/assets/benchmark.png">
    </picture>
</p>

## Features and Goals

- ⚡ Instant compiles and low memory usage
- 🔍 Expressive and useful diagnostics
- 🧩 Modular, library-based architecture
- 💻 Simple and hackable code base
- 🔄 Compatibility with the latest Solidity language breaking version (0.8.*)

## Getting started

Solar is available through a command-line interface, or as a Rust library.

### Library usage

You can add Solar to your Rust project by adding the following to your `Cargo.toml`:

```toml
[dependencies]
solar = { version = "0.1.0", package = "solar-compiler" }
```

Or through the CLI:

```console
$ cargo add solar-compiler --rename solar
```

You can see examples of how to use Solar as a library in the [examples](/examples) directory.

### Binary usage

Pre-built binaries are available for macOS, Linux and Windows on the [releases page](https://github.com/paradigmxyz/solar/releases)
and can be installed with the following commands:

```console
# On macOS and Linux.
curl -LsSf https://paradigm.xyz/solar/install.sh | sh

# On Windows.
powershell -c "irm https://paradigm.xyz/solar/install.ps1 | iex"

# For a specific version.
curl -LsSf https://paradigm.xyz/solar/v0.1.0/install.sh | sh
powershell -c "irm https://paradigm.xyz/solar/v0.1.0/install.ps1 | iex"
```

You can also build Solar from source:

```console
# From crates.io.
$ cargo install solar-compiler --locked

# From GitHub.
$ cargo install --git https://github.com/paradigmxyz/solar --locked

# From a Git checkout.
$ git clone https://github.com/paradigmxyz/solar
$ cd solar
$ cargo install --locked --path crates/solar
```

Once installed, check out the available options:

```console
$ solar -h
```

Here's a few examples:

```console
# Compile a single file and emit ABI to stdout.
$ solar Counter.sol --emit abi

# Compile a contract through standard input (`-` file).
$ echo "contract C {}" | solar -
$ solar - <<EOF
contract HelloWorld {
    function helloWorld() external pure returns (string memory) {
        return "Hello, World!";
    }
}
EOF

# Compile a file with a Foundry project's remappings.
$ solar $(forge re) src/Contract.sol
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
