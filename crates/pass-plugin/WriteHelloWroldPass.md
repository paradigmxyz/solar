# Write a hello world pass

This pass counts `add` operations in lowered MIR and prints the result for each
function. It returns no edits, so it leaves the program unchanged.

## Create the plugin

Create a Rust library outside the compiler repository. In its `Cargo.toml`,
point the SDK dependency at the `crates/pass-plugin` directory of your compiler
checkout:

```toml
[package]
name = "hello-pass"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["dylib"]

[dependencies]
solar-pass-plugin = { path = "/path/to/solar/crates/pass-plugin" }
```

Replace `/path/to/solar` with the actual checkout path. Put this in
`src/lib.rs`:

```rust
use solar_pass_plugin::{API_VERSION, Ir, Manifest, Pass, Plugin, Request, Response, Snapshot};

struct Hello;

impl Plugin for Hello {
    fn manifest(&self) -> Manifest {
        Manifest {
            api_version: API_VERSION,
            name: "hello".into(),
            version: "0.1.0".into(),
            passes: vec![Pass {
                name: "count".into(),
                ir: Ir::Mir,
                phase: Some("lowered".into()),
                enabled_by_default: false,
            }],
        }
    }

    fn run(&self, request: &Request) -> Result<Response, String> {
        let Snapshot::Mir { functions, .. } = &request.module else {
            return Err("expected MIR".into());
        };

        for function in functions {
            let count = function.instructions.iter().filter(|inst| inst.operation == "add").count();
            eprintln!("{}: {count} add operations", function.name);
        }

        Ok(Response::default())
    }
}

solar_pass_plugin::export_plugin!(Hello);
```

The manifest registers the pass as `hello::count`. The `lowered` phase tells
the compiler which MIR it accepts. `enabled_by_default: false` means it runs
only when selected in a pipeline. Each request contains a snapshot of one MIR
module; its functions contain the active instructions. `Response::default()`
returns an empty edit list.

## Build and run

Build both the compiler and the plugin with the same Rust toolchain, SDK
checkout, target, profile and Rust flags. The loader rejects incompatible
builds. Build the compiler from its checkout with `cargo build -p solar-compiler`,
and the plugin from its own directory with `cargo build`.

Create `hello.mir` in the plugin directory:

```text
@module Hello
@phase lowered
fn @entry() [entry] {
  bb0:
    v0 = calldataload 0
    v1 = add v0, 1
    mstore 0, v1
    returndata 0, 32
}
```

On Linux, run the pass with:

```sh
/path/to/solar/target/debug/solar -Ogas \
  -Zload-pass-plugin=target/debug/libhello_pass.so \
  -Zmir-pipeline=hello::count --emit=bin hello.mir
```

The pass prints `entry: 1 add operations` to stderr. On macOS the library is
`target/debug/libhello_pass.dylib`; on Windows it is
`target/debug/hello_pass.dll`.

For a pass that changes MIR or EVM IR, return edits in `Response` instead of an
empty list. See the [SDK guide](README.md) and the
[example plugin](../../tools/pass-plugin-example/src/lib.rs) for the available
edits and pipeline hooks.
