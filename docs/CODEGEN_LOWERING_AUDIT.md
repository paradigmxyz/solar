# Codegen lowering audit

This audit records the lowering rewrite and the remaining boundaries. It
describes observable structure in the current tree, not an intended design.

## Completed slices

* Scalar external arguments stay typed in built MIR. The `lower-abi` pass adds
  the calldata-size and canonical-word checks, carries enum variant counts in
  the ABI shape, then clears the temporary `abi_args=lazy` marker while it
  forms the wrapper. Raw words are loaded for validation so MIR simplification
  cannot assume the check already passed.
* Supported aggregate external arguments stay typed until `lower-abi`. Fixed
  arrays (including nested static arrays), dynamic arrays (including nested
  arrays and narrow scalar elements), byte strings, and scalar/byte/enum tuples
  carry an ABI shape in MIR; the ABI phase remaps physical head words,
  validates scalar fields, and builds either memory objects or calldata slices.
  Scalar dynamic arrays in calldata stay typed slices. Dynamic structs carry
  one trailing source-base word when their fields need calldata slices.
* ABI parameter shapes print and parse with MIR text, so the boundary metadata
  survives phase dumps and round trips.
* Source `abi.encode(...)` emits the typed `abi_encode` MIR operation. The ABI
  pass owns its allocation and tuple layout; HIR only adapts the resulting
  memory slice to a bytes object.
* `keccak256(abi.encode(...))` consumes that typed ABI slice directly instead
  of staging tuple words at an unbumped free-memory pointer.
* Packed ABI encodes allocate a semantic bytes object before writing their
  tight-packed payload, and packed hashes consume that object through
  `keccak256_bytes`.
* ECDSA and literal precompile inputs allocate semantic bytes objects before
  writing their fixed-size payloads; the HIR paths no longer read the free
  memory pointer for those scratch buffers.
* Multi-return values cross control-flow edges through semantic fixed-array
  storage, with only the data base published in the existing return-buffer
  slot.
* Contract-creation bytecode and constructor words use a semantic bytes object
  as their `CREATE`/`CREATE2` input instead of manually advancing the free
  memory pointer.
* Linked-library delegatecall payloads size a semantic bytes object from their
  dynamic tails before writing selector, heads, and tails; struct return space
  shares that allocation.
* Event payloads with representable ABI types use the same typed ABI operation;
  recursive or otherwise unsupported event types stop at the ABI boundary with
  a codegen diagnostic instead of using a HIR scratch buffer.
* Packed storage locations carry their semantic encoding. Signed values are
  sign-extended on load, fixed bytes are aligned at the MIR boundary, and both
  forms share the same read-modify-write path for state variables, fields, and
  fixed arrays.
* Nested calldata arrays use semantic memory-object length and data operations;
  the memory-layout pass selects their physical header offsets.
* Dynamic mapping keys stay as `mapping_slot_memory` or
  `mapping_slot_calldata` until the mapping-slot pass. Target-specific memory
  copies are selected after that semantic operation is lowered.
* Dynamic storage-array data slots stay as `storage_array_data_slot` until the
  same pass. Indexing, push/pop, bytes access, and aggregate copies share that
  typed operation instead of staging the slot in HIR scratch memory.
* Modifier placeholders expand the modifier chain in source order. Return
  values pass through the suffix, and constructor base calls stay in the
  constructor prelude.
* The lowering module exposes only `lower_contract` and
  `lower_contract_with_bytecodes` publicly. Context, loop, and storage
  implementation types are private to lowering and its child modules.

## Current shape

`crates/codegen/src/lower/mod.rs` owns HIR traversal, function construction,
ABI layout calculation, calldata and constructor decoding, local-frame
allocation, inline-call management, helper synthesis, and return encoding.
The sibling modules are extensions of that one mutable context rather than
independent lowering stages. A code search shows direct `mload`, `mstore`,
`sload`, `sstore`, calldata, and returndata operations spread through
`expr.rs`, `stmt.rs`, `call.rs`, `bytes.rs`, `index.rs`, `storage.rs`, and
`abi_encode.rs`.

## Concrete shortcomings

* **The ABI boundary is still split for complex aggregates.**
  `Lowerer::lower_function` still decodes recursive or unsupported aggregate
  shapes, and the ABI phase's supported decoder does not yet share every
  malformed-calldata guard with the legacy path. Supported fixed arrays,
  dynamic arrays, byte strings, and scalar/byte/enum structs now defer to
  `lower-abi`, but the pass still has a second representation for unsupported
  aggregate shapes.
* **Raw memory is used as a language-level value model.** Mutable locals are
  assigned fixed offsets beginning at `EvmMemoryLayout::HEAP_START`, and many
  lowering paths manually pair pointer and length words. This couples HIR
  lowering to the physical EVM memory policy and makes alias reasoning depend
  on undocumented scratch-space conventions.
* **Helper generation is ad hoc.** `Lowerer` has separate option fields for
  `Error(string)` and storage-bytes helpers, a recursion guard, and helper
  builders embedded in the main lowering context. Other nontrivial operations
  (checked exponentiation, ABI copies, and repeated cleanup/validation) are
  emitted inline or have their own special-case builders. The helper API does
  not describe an operation, its inputs, or deduplication key, and the
  outlining path used to force helpers to remain out of later inlining passes.
* **Pattern-specific lowering is duplicated.** Struct, fixed-array, dynamic
  array, bytes, and calldata-slice handling each have separate branches in
  parameter setup, expression indexing, assignment, return gathering, and
  storage copying. Several branches repeat pointer/length arithmetic and
  bounds checks instead of sharing a type-directed aggregate abstraction.
* **Storage semantics are still split between incompatible paths.** Static
  state variables, direct struct fields, and fixed arrays now share
  `StorageLocation` and its encoding for packed reads and read-modify-write
  stores. Runtime storage references, nested aggregate copies, and dynamic
  array and bytes element strides still mix that layout with independent slot
  arithmetic. The replacement should make one type-directed location query the
  only source of storage addresses.
* **Context state is difficult to reason about.** One `Lowerer` carries
  function-local maps, contract-wide storage maps, inline-return state,
  constructor state, ABI state, helper state, and error-checking state. Calls
  to `ensure_internal_mir_function` save and restore a large subset of these
  fields manually. This permits state from one HIR function or inline call to
  affect another path when a new field is not added to every save/restore
  sequence.
* **Modifier lowering is now explicit, but still shares the function frame.**
  Function-root discovery excludes modifier declarations. The lowering stage
  expands modifier chains at `StmtKind::Placeholder`, carries return values
  through suffix code, and keeps constructor base calls in the constructor
  prelude. Modifier parameters and locals still use the enclosing function's
  frame, so frame isolation remains a follow-up concern.
* **Legacy compatibility surface is broad.** The top-level module exposes many
  `pub(crate)` and `pub(super)` methods because sibling files reach through the
  context. Their callers are not grouped by phase, so removing one helper
  requires searching the whole directory and often changes unrelated lowering
  logic. The rewrite must retain only methods with verified callers and keep
  new interfaces at the smallest useful visibility.

The verified entry-point surface is small. `lower_contract_with_bytecodes` has
production callers in `crates/codegen/src/contract.rs` and `benches/src/lib.rs`.
`lower_contract` is used by codegen MIR tests. No caller needs `Lowerer`,
`LoopContext`, or storage implementation types outside the lowering module.

## Replacement constraints

The replacement will translate HIR expressions and statements into typed MIR
values and semantic memory/storage objects. ABI decoding, wrappers, and
external termination will be created by the ABI phase. Allocation and object
access will remain semantic until the memory/backend boundary. Storage layout
will be represented by one type-directed location abstraction that handles
slots, byte offsets, packed reads, and packed writes. Outlined helpers will be
named operations keyed by their semantic signature, generated lazily, and
allowed to be inlined by later passes.
