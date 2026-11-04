# Native pass plugins

Build a shared library, export `solar_pass_plugin_v1`, and load it into the
compiler with `-Zload-pass-plugin=/path/to/library`. Linux uses `.so`, macOS
uses `.dylib`, and Windows uses `.dll`. Loading a library executes native code
with the compiler's privileges; only load libraries you trust.

Loading is command-line only, including with `--standard-json`.
`settings.solar.unstable` can select registered passes in a pipeline, but
cannot load a library. Command-line plugin paths remain active across the
Standard JSON settings reset.

```sh
cargo build -p solar-pass-plugin-example
solar -Ogas -Zload-pass-plugin=target/debug/libsolar_pass_plugin_example.so C.sol
```

The example in `tools/pass-plugin-example` exports a MIR `add x, 0`
simplification and an EVM IR `NOT; NOT` elimination. It uses only this SDK,
not compiler internals. Out-of-tree Rust plugins depend on `solar-pass-plugin`
and build with `crate-type = ["cdylib"]`. C and C++ plugins use
`include/solar_pass_plugin.h` and implement the same JSON protocol.

The entry point receives a borrowed JSON request and a C response writer.
It writes exactly one response before returning: JSON on success, or a
diagnostic on failure. The host owns the copied response and unloads the
library after the compilation. Calls into each library are serialized within
one compilation. Separate compilations in an embedding process may call the
same library concurrently; plugins must synchronize any shared global state.
Pointers, allocations, Rust trait objects, and compiler objects never cross
the boundary. The `dispatch` helper handles parsing and contains Rust panics
when the plugin was built with panic unwinding.

Registration starts with `{"action":"describe","api_version":1}`. Return a
`Manifest` containing the same API version, a plugin name and version, and
its pass definitions. Names use ASCII letters, digits, `_` and `-`; a pass
is addressed as `plugin::pass`. Duplicates and incompatible APIs are errors.
The public Rust types in `src/lib.rs` define version 1 of the wire schema.

Passes with `enabled_by_default` run when their library is loaded, even at
`-O none`. Semantic MIR passes run before semantic optimization; lowered MIR
passes run before the lowered optimization pipeline; EVM IR passes run
before the machine optimization pipeline. These hooks keep required lowering,
instruction selection, legalization, and assembly under compiler control.
Explicit pipelines run only the passes they name:

```sh
solar -Zload-pass-plugin=./libacme.so -Zmir-pipeline=acme::fold,sccp input.mir
solar -Zload-pass-plugin=./libacme.so -Zevm-ir-pipeline=acme::compact input.evmir
```

A run request carries the selected EVM version, optimization objective, pass
name, a text dump, and a structured snapshot of active instructions. MIR
snapshots include types, operands, scalar constants, and each function's
objective. IDs are local to that invocation. The plugin returns ordered
edits against those IDs, not a replacement text dump or a claimed `changed`
flag. EVM instruction indices refer to the current block at each edit, so
independent edits are usually easiest to return from the end backwards.

Version 1 supports replacing MIR SSA uses, rewriting operand-only MIR
operations with the same result representation, and splicing EVM instruction
windows. It preserves declarations, signatures, and CFG identities. New EVM
instructions can copy an old instruction (including its relocation) or create
an opcode/push. A new instruction can inherit debug metadata from a source
inside its replaced window; otherwise its location is unknown. Protected
instruction pairs cannot be split. Creating new blocks, functions, operation
definitions, or verbatim bytecode is outside version 1.

Edits are applied to a private candidate and verified before committing it.
Invalid types, dominance, stack behavior, target requirements, and phase
violations fail compilation, including with `-Zvalidate-ir=false`. Changed
MIR invalidates the normal analyses and backend records. External passes are
not skipped by the built-in no-change cache. Timing and pass dumps identify
the qualified pass name.

Verification establishes valid IR, not equivalence. Authors must preserve
observable effects, including gas and memory-size observations, and test their
transformations against the interpreters and runtime tests. Library crashes
can terminate the compiler. Plugin binaries and configuration are build inputs:
when using external build caches, invalidate them after changing a plugin.
