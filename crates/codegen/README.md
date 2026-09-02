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
The declaration generates operand traversal, the `Op` view that rewrite rules
match on, and `isle/prelude.isle`, the ISLE vocabulary for that view. Rule
sets such as `isle/inst_simplify.isle` are compiled to Rust by `build.rs`
with `cranelift-isle`; the extractors and constructors they call are
implemented next to the pass that runs them.

### Key Types

- `ValueId`, `InstId`, `BlockId`, `FunctionId`: Index types for SSA values
- `MirType`: Types used in MIR (UInt, Address, MemPtr, StoragePtr)
- `InstKind`: Instruction variants (Add, Sub, SLoad, SStore, Call, etc.)
- `Terminator`: Block terminators (Jump, Branch, Return, Revert)
