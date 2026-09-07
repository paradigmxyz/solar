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

```rust
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

### Key Types

- `ValueId`, `InstId`, `BlockId`, `FunctionId`: Index types for SSA values
- `MirType`: Types used in MIR (UInt, Address, MemPtr, StoragePtr)
- `InstKind`: Instruction variants (Add, Sub, SLoad, SStore, Call, etc.)
- `Terminator`: Block terminators (Jump, Branch, Return, Revert)
