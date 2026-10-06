# solar-codegen

Solidity MIR (Mid-level Intermediate Representation) and EVM code generation for Solar.

## Architecture

```text
HIR (from solar-sema) -> Lowering -> MIR -> Code Generation -> EVM Bytecode
```

### MIR Structure

- **Module**: Top-level container with functions, data segments, and storage layout
- **Function**: SSA-form functions with basic blocks, values, and instructions
- **BasicBlock**: Sequence of instructions ending with a terminator
- **Instruction**: Operations (arithmetic, memory, storage, control flow)
- **Value**: SSA values (instruction results, arguments, immediates, phi nodes)

### Operation Schema and Rewrite Rules

`src/mir/op_schema.rs` declares every MIR operation once: its typed payload,
named operands, mnemonic, result kind, phase legality, effects, and traits.
Metadata sits directly above the operation inside `define_mir_ops!`:

```rust,ignore
#[mir_op(
    mnemonic = "add",
    result = I256,
    phases = PhaseSet::ALL,
    effect = Pure,
    traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE),
    side_effects = false,
    category = None
)]
#[commutative(a, b)]
#[builder(add)]
Add(a: ValueId, b: ValueId),
```

The declaration generates operand traversal, parser constructors for operations
with only value operands, default result types, typed builder methods, and the
`Op` view used by rewrite rules. The generic printer uses the same mnemonic and
operand order; operations with attributes retain custom syntax. Attribute-dependent
spellings use `#[mnemonic(pattern => name)]` alongside the declaration.
`#[commutative(a, b)]` generates both the commutativity trait and canonical operand
ordering, including modular arithmetic where the modulus must stay in place.
Structural result checks and operation phase legality consume the schema,
including the gate that advances MIR from `semantic` to `lowered`. The scheduler consumes the
schema's rematerialization trait; constant folding uses the shared opcode selector.

Equivalent local rewrites use `Instruction::replace_kind` or
`Instruction::rewrite_operands`. These preserve result identity, provenance, and
semantic obligations while invalidating memory regions, storage aliases, and
effect overrides. The caller still proves equivalence; the helpers do not prove
an optimization correct or permit changing the result representation.

ISLE sources are grouped by compiler layer:

| Directory | Purpose |
| --- | --- |
| `isle/mir/` | MIR optimization: e-graph identities and word-sequence rewrites. |
| `isle/mir-to-evm/` | MIR-to-EVM opcode selection and selection using the physical stack. |
| `isle/evm-ir/` | Scheduled EVM IR peepholes, stack rewrites, and late word rewrites. |

`isle/mir/prelude.isle` declares the generated operation view, and
`isle/mir/extractors.isle` adds value-definition extractors for MIR simplification.
The opcode table in `src/backend/evm/op.rs` generates the constants in
`isle/evm-ir/prelude.isle`. `isle/mir-to-evm/select.isle` uses the two vocabularies to select
single EVM opcodes and scheduling shapes, without access to value definitions.
The emitter and target cost model call that same selector.

`build.rs` compiles the rule sets to Rust with `cranelift-isle`. Local identities
in `isle/mir/egraph` run inside the existing Rust e-graph algorithm; EVM IR window
patterns live in `isle/evm-ir/peephole.isle`. Global analysis, profitability, stack
scheduling, complex lowering, and assembly remain in Rust. The schema snapshot
tests check the generated vocabularies, and the selector snapshot checks its
opcode mappings and stack contracts against both operation tables.

The e-graph owns scalar identities, checked constant evaluation, passing-check
removal, and fixed aggregate projection folding. Late `const-fold` shares its
rules but accepts only immediate results, so it cannot extend nonconstant live
ranges before stack scheduling. Constants stay on the right of commutative
operations and comparisons, with comparison predicates reversed when needed.
Matching, node insertion, and final materialization share this ordering, so
rules need not repeat constant-left variants.

The e-graph overlaps pure-expression CSE, but it does not replace the `cse`
pass's alias-sensitive memory, storage, and call reuse. SCCP still propagates
constants over executable CFG edges; range analysis, PRE, and LICM still supply
facts and choose placement. Keep these algorithms in Rust and use ISLE for
bounded local identities. This gives us typed matchers, overlap checks, and
one rule source for optimization and offline checking without implying that
every Rust rewrite belongs in the DSL. See the repository's
[rule-writing guidance](../../AGENTS.md#operation-schema-and-isle-rules).

### Library addresses and relocations

Unresolved library addresses use `LibraryId` indices into a module-owned table of
source-qualified names. MIR `library_address` produces `i160`. MIR, EVM IR
(`push_library`), and the compact assembler carry the same IDs. The primitive
assembler emits a fixed-width `PUSH20` slot and records its library directly;
placeholder bytes carry no identity. Embedded creation and runtime bytecode carry
their library tables and relocations; lowering remaps their IDs into the parent
module's table.
Data pooling shares bytes only when the library identities and offsets also match.
MIR and EVM IR text declare libraries and data in `@libraries` and `@data` sections after the
module header, and refer to both by declared name:

```text
@libraries
  Library_0: "source.sol:Library"

@data
  Child_creation_code_0: creation_code "child.sol:Child"
  literal_1: hex"..." library_relocations [2: Library_0]
```

Instructions refer to a library as `library_address Library_0` in MIR and
`push_library Library_0` in EVM IR.

### Optimization search and costs

The offline rule tool can mine bounded pure trees from real MIR artifacts,
rank them by occurrence-weighted target cost, search for cheaper equivalents,
and verify the emitted ISLE. See [the discovery and proof guide](../../scripts/evm-rules/README.md).
Generated candidates still require scheduled-code measurements before inclusion.
Subtree abstraction exposes generic shift-count and repeated-mask patterns inside
larger expressions; the proof quantifies over every value of each abstract input.
The in-compiler e-graph remains bounded and acyclic; this is not full equality saturation.

`-Ogas --optimize-runs=N` sets the expected execution count used by lifetime
decisions without changing the chosen optimization objective. Standard JSON
continues to use `settings.optimizer.runs`. The existing `inline` pass weights
eligible calls in statically counted loops and leaves conditional or unknown-bound
calls at their ordinary estimate. Broad inlining remains an explicitly selected
pass; the default pipeline retains its narrower measured inlining policies.

The physical planner compares equal-length windows with the same final stack,
including useful one-instruction windows before effects or unsupported operations.
Load PRE can split critical edges in gas mode, charging the new transfer and
preserving phi inputs; it inserts no read on a bypass path. Size mode retains
the existing unsplit-edge policy. Constant folding prices the removed opcode's
dynamic work, including exponent bytes, and requires non-increasing gas and bytes.

The `late-word` EVM IR pass applies mined low-mask identities after outlining and
stack cleanup. It checks either a closed count computation or the independence
of a protected shift base through stack permutations. Pricing the physical
replacement avoids changes to earlier sharing decisions that can turn a local
MIR size reduction into larger final bytecode.

CI checks the compiled word rules and a separate pure physical-stack subset.
Lean proves every rule against EVM semantics written in Lean, and its kernel
checks each proof.
These proofs cover the modeled rules and explicit trusted contracts, not global
memory transformations, the complete backend, or whole-program correctness.

### Key Types

- `ValueId`, `InstId`, `BlockId`, `FunctionId`: Index types for SSA values
- `MirType`: Types used in MIR (UInt, Address, MemPtr, StoragePtr)
- `InstKind`: Instruction variants (Add, Sub, SLoad, SStore, Call, etc.)
- `Terminator`: Block terminators (Jump, Branch, Return, Revert)

## Optional backends

`--codegen-backend` selects `evm` (the default), `yul`, `sonatina`, `sir`, or
`llvm`. The CLI enables the four optional Cargo features by default:

| Backend | Cargo feature | Toolchain |
| --- | --- | --- |
| Yul | `codegen-yul` | `solc`, or the executable named by `SOLAR_SOLC` |
| Sonatina | `codegen-sonatina` | Statically linked Sonatina Rust libraries |
| Sensei IR | `codegen-sir` | Statically linked Plank Rust libraries |
| LLVM IR | `codegen-llvm` | Statically linked solx EVM LLVM through its Inkwell bindings |

The library enables none of these features by default. To build the CLI without
LLVM, disable defaults and select the desired features explicitly, for example:

```sh
cargo build -p solar-compiler --no-default-features \
  --features cli,mimalloc,tracing,codegen-yul,codegen-sonatina,codegen-sir
```

LLVM builds require the EVM-enabled LLVM 21 fork, including LLD. Stock LLVM does
not provide this target. Set `LLVM_SYS_211_PREFIX` to its build or installation
directory before running Cargo. The native build used for this integration is
[NomicFoundation/solx-llvm at 9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a](https://github.com/NomicFoundation/solx-llvm/tree/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a).
For an existing checkout of that revision:

```sh
cmake -S /path/to/solx-llvm/llvm -B target/llvm-evm \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
  -DLLVM_TARGETS_TO_BUILD= -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=EVM \
  -DLLVM_DEFAULT_TARGET_TRIPLE=evm -DLLVM_ENABLE_PROJECTS=lld \
  -DLLVM_ENABLE_RTTI=ON -DLLVM_INCLUDE_TESTS=OFF \
  -DLLVM_INCLUDE_BENCHMARKS=OFF -DLLVM_INCLUDE_EXAMPLES=OFF \
  -DLLVM_INCLUDE_UTILS=OFF -DLLVM_INCLUDE_RUNTIMES=OFF \
  -DLLVM_BUILD_TOOLS=OFF -DLLD_BUILD_TOOLS=OFF -DLLVM_ENABLE_ZLIB=OFF \
  -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF
cmake --build target/llvm-evm --parallel
cmake --build target/llvm-evm --target llvm-config --parallel
export LLVM_SYS_211_PREFIX="$PWD/target/llvm-evm"
cargo build -p solar-compiler --bin solar
```

Sonatina, SIR, and LLVM currently require Osaka. All four adapters lower data
sections, child-contract creation, constructor arguments, typed immutables,
internal frames, multi-value calls, memory copies, external calls, logs, and
ordinary environment operations. Opaque metadata stays at the runtime's end.
Yul and LLVM use native immutable relocations; Sonatina and SIR use trailing
immutable words patched during deployment and loaded with `CODECOPY`.

Alternative backends require external library addresses in Standard JSON
`settings.libraries`. They reject unresolved references, including references in
embedded child bytecode; they do not emit native linker relocations yet.

Use `-Zdump=backend-ir` to inspect the converted IR. Sonatina, SIR, and LLVM dumps
include separate runtime and deployment modules. SIR embeds the compiled runtime
as a deployment data segment. The built-in backend alone provides source maps
and EVM IR dumps. Unsupported operations produce diagnostics without switching
backends.

Native spills use compiler-private memory below contract memory. The adapters
translate memory accesses while keeping Solidity pointer values and the visible
`MSIZE` unchanged. Yul uses the boundary returned by `memoryguard`; the other
backends measure native storage and repeat planning if translation needs more
slots. This adds address arithmetic and memory-expansion cost when spills occur.
Sonatina retains explicit expansion for memory reads when `MSIZE` can observe it;
LLVM uses volatile reads and copies for those cases.

These adapters are still experimental. SIR's upstream compiler rejects recursive
calls and does not expose `MSIZE`. Yul cannot spill recursive functions; recursive
native spilling also remains limited in Sonatina and LLVM. Internal activation frames use the free-memory pointer and
remain allocated so escaping references stay valid. This can cost more memory
and gas than the built-in backend's frame planning.

Yul uses solc's process interface; it does not link C++ libsolc into the CLI.
LLVM and Sonatina construct their native IR with typed builders; text dumps are
for inspection and LLVM worker transport. Sonatina uses a strict, tested parser
for its pinned memory-plan snapshot because upstream keeps structured plan
access private. Unknown formats stop compilation.

Sonatina and SIR use their own native optimization and stack-scheduling pipelines.
SIR uses the same upstream O2 pipeline for gas and size because it has no distinct
size preset. LLVM uses the EVM LLVM target and linker in a child of the same
statically linked CLI. Library hosts using LLVM must call
`backend::llvm::initialize_cli_worker` at startup and return its optional exit
code. The musl release explicitly omits LLVM because its native libraries
need a matching musl C++ toolchain; the other release targets include it.

See [the benchmark guide](../../benches/runtime/README.md) for backend selection
and comparison commands.
