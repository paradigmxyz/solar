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

### Key Types

- `ValueId`, `InstId`, `BlockId`, `FunctionId`: Index types for SSA values
- `MirType`: Types used in MIR (UInt, Address, MemPtr, StoragePtr)
- `InstKind`: Instruction variants (Add, Sub, SLoad, SStore, Call, etc.)
- `Terminator`: Block terminators (Jump, Branch, Return, Revert)

## Memory ownership

Compiler frames, spill slots, and temporary return buffers require memory that
source assembly cannot address. An unannotated assembly block that can access
memory outside the constant scratch range `[0, 64)`, change the free-memory
pointer, or change a memory binding marks its function `unrestricted_memory`.
Reading the free-memory pointer alone does not require this restriction.
`assembly ("memory-safe")` and the legacy `/// @solidity memory-safe-assembly`
annotation supply the source contract instead.

The backend propagates this restriction through each internal call context.
It keeps compiler state on the stack, including arguments, return tuples, and
phi values, and rejects code generation if a required memory fallback remains.
Separate external entry points and creation code have separate memory lifetimes.
Recursive Yul tuple components and their callees also require stack-owned state,
so suspended calls cannot reuse a frame. The backend tracks this requirement
separately from the source assembly annotation.
MIR `compiler_memory` metadata distinguishes private accesses from source
operations; it is independent of alias and debug metadata. The emitter checks
private operations after operand scheduling, so a source memory instruction
cannot grant permission to a scheduler spill.

A closed stack-recovery sequence may temporarily use zeroed memory above
`msize()` to reach a deep value or arrange a wide control-flow edge. It restores
the saved words and clears that scratch before any source operation runs. This
exception requires memory extent to be unobservable in the call context; clearing memory cannot undo its expansion. See
[CODEGEN-008](../../docs/SOLC_DIVERGENCE.md#codegen-008-deep-forwarding-stacks-with-observable-memory-size).

Constructor immutable assignments can remain in SSA until a normal constructor
exit carries their values to the deployment postlude. Staging then happens after
source execution ends. This lowering requires each assignment to dominate its
reads and normal exits; it inlines nonrecursive readers when needed. Shapes that
cannot establish this ownership retain the guarded memory lowering.
