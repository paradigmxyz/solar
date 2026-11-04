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
cargo build -p solar-compiler -p solar-pass-plugin-example
target/debug/solar -Ogas -Zload-pass-plugin=target/debug/libsolar_pass_plugin_example.so C.sol
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

Version 1 supports SSA rewrites, instruction splices, new internal MIR
functions, and new blocks in both IRs. Existing declarations and signatures
remain unchanged; a pass can redirect control flow into its new code.

`AddMirFunctions` declares every new signature before building any body, so
helpers can call each other or existing functions by ID. Function names are
unique ASCII identifiers. New function IDs
start at the current function count. `AddMirBlocks` appends blocks to one
function. Block IDs start at its current block count, or zero for a new
function. `SetMirTerminator` changes its control flow. A `SplitMirBlock`
moves the suffix and terminator into a new block, installs a jump, and repairs
successor phi predecessors; splitting through phi nodes is rejected.

New MIR instructions use operand-only operations, phis, calls, or a copy of
an existing operation with new operands. `Type` describes parameters, results,
and scalar constants. Generated integers use `i1`, `i160`, or `i256`; struct
IDs refer to the existing type table. `MirOperand`
distinguishes existing value IDs, arguments, typed constants, and response-local
result IDs. Local IDs are scoped to a function and must be defined once per
response. All results in an added block batch are reserved before its bodies,
allowing forward references and loop phis. `SpliceMirInstructions` inserts these
instructions into existing code; `ReplaceMirValueWith` connects their results
to old uses. Unresolved locals, incorrect types, and invalid dominance are errors.
Lowered switches require distinct scalar constants as their case values.

EVM IR has no typed function objects. `AddEvmBlocks`, `SplitEvmBlock`, and
`SetEvmTerminator` build CFGs and callable helpers using explicit stack
arguments and return-address pushes. `BlockAddress` constructs a block
relocation, and a helper may declare its `CallEffect`. IDs start at the current
block count; textual labels are assigned by the host. Each added batch reserves
all blocks before building their bodies, allowing forward edges and cycles.
Instructions can copy existing code, including debug context and relocations,
from the same block or another block, or create an opcode/push.

Copies retain known debug context; generated instructions and transfers are
unmapped. Splits move the original terminator's context with it. Protected
EVM instruction pairs cannot be split or copied individually. New operation
definitions, type-table entries, and verbatim bytecode remain outside this API.

The example's `create-mir-helper`, `create-lowered-helper`, and
`create-evm-helper` passes demonstrate these edits. They deliberately introduce
calls to exercise construction, rather than claim a gas or size improvement.

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
