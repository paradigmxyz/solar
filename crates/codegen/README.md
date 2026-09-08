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
    result = Word,
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
including the gate that advances MIR to `evm-shaped`. The scheduler consumes the
schema's rematerialization trait; constant folding uses the shared opcode selector.

Equivalent local rewrites use `Instruction::replace_kind` or
`Instruction::rewrite_operands`. These preserve result identity, provenance, and
semantic obligations while invalidating memory regions, storage aliases, and
effect overrides. The caller still proves equivalence; the helpers do not prove
an optimization correct or permit changing the result representation.

`isle/prelude.isle` declares the generated operation view, and
`isle/extractors.isle` adds value-definition extractors for MIR simplification.
The opcode table in `src/backend/evm/op.rs` generates the constants in
`isle/evm_prelude.isle`. `isle/select.isle` uses the two vocabularies to select
single EVM opcodes and scheduling shapes, without access to value definitions.
The emitter and target cost model call that same selector.

`build.rs` compiles the rule sets to Rust with `cranelift-isle`. Local identities
in `isle/egraph.isle` run inside the existing Rust e-graph algorithm; EVM IR window
patterns live in `isle/peephole.isle`. Global analysis, profitability, stack
scheduling, complex lowering, and assembly remain in Rust. The schema snapshot
tests check the generated vocabularies, and the selector snapshot checks its
opcode mappings and stack contracts against both operation tables.

### Optimization search and costs

The offline rule tool can mine bounded pure trees from real MIR artifacts,
rank them by occurrence-weighted target cost, search for cheaper equivalents,
and verify the emitted ISLE. See [the discovery and proof guide](../../scripts/evm_rules/README.md).
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
These proofs cover the modeled rules and explicit trusted contracts, not global
memory transformations, the complete backend, or whole-program correctness.

### Key Types

- `ValueId`, `InstId`, `BlockId`, `FunctionId`: Index types for SSA values
- `MirType`: Types used in MIR (UInt, Address, MemPtr, StoragePtr)
- `InstKind`: Instruction variants (Add, Sub, SLoad, SStore, Call, etc.)
- `Terminator`: Block terminators (Jump, Branch, Return, Revert)
