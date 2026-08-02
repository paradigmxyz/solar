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
  validates scalar fields and calldata ranges, and builds either memory objects
  or calldata slices.
  Scalar dynamic arrays in calldata stay typed slices. Dynamic structs carry
  one trailing source-base word when their fields need calldata slices.
* ABI parameter shapes print and parse with MIR text, so the boundary metadata
  survives phase dumps and round trips.
* ABI shape construction tracks recursive structs and fails closed with a
  codegen diagnostic instead of recursing through HIR lowering.
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
* Custom-error reverts use the typed ABI encoder with the error selector, so
  empty and aggregate payloads share the same layout and memory boundary as
  source-level ABI encodes.
* Packed storage locations carry their semantic encoding. Signed values are
  sign-extended on load, fixed bytes are aligned at the MIR boundary, and both
  forms share the same read-modify-write path for state variables, fields, and
  fixed arrays.
* Nested calldata arrays use semantic memory-object length and data operations;
  the memory-layout pass selects their physical header offsets.
* Calldata fixed arrays and aggregate field copies allocate typed memory
  objects and write through their element or field operations. Byte-object
  literal materialization and zero-padding use word-chunk object stores.
* ABI decoding stores dynamic-array element pointers and validates copied words
  through typed object accesses. Fixed-array literals use the same element
  store path, leaving only bulk payload copies as raw memory operations.
* Literal low-level call payloads use a bytes memory object for their
  length-prefixed staging, then expose only its data slice to the call.
* Literal mapping keys and embedded creation bytecode use typed bytes-object
  stores for their word chunks before hashing or returning the object.
* Dynamic mapping keys stay as `mapping_slot_memory` or
  `mapping_slot_calldata` until the mapping-slot pass. Target-specific memory
  copies are selected after that semantic operation is lowered.
* Dynamic storage-array data slots stay as `storage_array_data_slot` until the
  same pass. Indexing, push/pop, bytes access, and aggregate copies share that
  typed operation and the shared element-stride helper instead of staging the
  slot in HIR scratch memory.
* Dynamic storage-array element slots now stay as
  `storage_array_element_slot` with their type-directed stride. The mapping
  pass alone expands the hash and offset arithmetic, including long bytes
  element access.
* Storage bytes ABI copies keep their loop index separate from the physical
  slot. Each load, store, and stale-slot clear requests a typed
  `storage_array_element_slot`, so long-form byte encoding and decoding defer
  storage hashing to the mapping-slot pass.
* Mutable locals and storage-reference values use typed `frame_load` and
  `frame_store` operations. `lower-frame-slots` selects the external scratch
  region or the internal-call frame and lowers those operations to physical
  memory after ABI and dispatch lowering. Slice slots stay typed as
  pointer/length values until that pass.
* Memory-object field and element reads and writes stay typed through HIR
  aggregate lowering. `lower-memory-objects` alone selects their physical
  offsets and emits the final word loads and stores.
* Storage-to-memory and memory-to-storage aggregate copies use the same typed
  field and element operations. Packed and nested copies no longer expose a
  destination address to HIR lowering.
* The ABI encoder reads tuple fields and fixed-array elements through typed
  memory-object loads. Address formation and the final `mload` happen only in
  `lower-memory-objects`.
* Panic, short-error, and storage-bytes helpers use one lazy registry keyed by
  semantic operation. Repeated uses share one helper, while synthesis guards
  keep recursive helper construction finite.
* Modifier placeholders expand the modifier chain in source order. Return
  values pass through the suffix, and constructor base calls stay in the
  constructor prelude.
* Function, inline-call, and modifier lowering use scoped state overlays.
  Bindings, loop targets, error flags, storage-reference markers, and return
  continuations are restored at each boundary; inline frame allocation keeps
  only its high-water mark in the enclosing function.
* Recursive calldata-array materialization and storage-bytes copy loops keep
  their counters in typed temporary frame slots. Nested elements are written
  through semantic memory-object stores instead of pointer scratch buffers.
* The lowering module exposes only `lower_contract` and
  `lower_contract_with_bytecodes` publicly. Context, loop, and storage
  implementation types are private to lowering and its child modules.

## Current shape

`crates/codegen/src/lower/mod.rs` owns HIR traversal, function construction,
ABI layout calculation, calldata and constructor decoding, logical frame-slot
allocation, inline-call management, helper requests, and return encoding.
The sibling modules are extensions of that one mutable context rather than
independent lowering stages. A code search shows direct `mload`, `mstore`,
`sload`, `sstore`, calldata, and returndata operations spread through
`expr.rs`, `stmt.rs`, `call.rs`, `bytes.rs`, `index.rs`, `storage.rs`, and
`abi_encode.rs`.

## Concrete shortcomings

* **The ABI boundary is still split for constructor aggregates.**
  External functions with supported fixed arrays, dynamic arrays, byte
  strings, and scalar/byte/enum structs defer to `lower-abi`, including its
  range and overflow checks. Constructors still decode their argument blob in
  HIR lowering because the ABI phase currently handles runtime calldata only.
* **Physical memory still leaks through a few aggregate helpers.** Mutable
  locals, direct object accesses, and storage aggregate copies now stay typed,
  but memory-object allocation, byte/ABI bulk copies, and some loop scratch
  state still emit raw memory operations before the semantic memory passes.
  The remaining work is to keep those policies in typed MIR until the
  memory-layout boundary.
* **Helper coverage is incomplete.** Panic, short-error, and storage-bytes
  helpers now share a keyed lazy registry. Checked exponentiation, ABI copies,
  and repeated cleanup or validation still have inline or pass-specific
  builders, so the registry does not yet cover every reusable operation.
* **Pattern-specific lowering is duplicated.** Struct, fixed-array, dynamic
  array, bytes, and calldata-slice handling each have separate branches in
  parameter setup, expression indexing, assignment, return gathering, and
  storage copying. Several branches repeat pointer/length arithmetic and
  bounds checks instead of sharing a type-directed aggregate abstraction.
* **Storage semantics are still split between incompatible paths.** Static
  state variables, direct struct fields, and fixed arrays now share
  `StorageLocation` and its encoding for packed reads and read-modify-write
  stores. Runtime storage references and some nested aggregate paths still mix
  that layout with independent slot arithmetic. The replacement should make
  one type-directed location query the only source of storage addresses.
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
values, frame slots, and semantic memory/storage objects. ABI decoding,
wrappers, and external termination will be created by the ABI phase. Frame
slots, allocation, and object access will remain semantic until the
memory/backend boundary. Storage layout will be represented by one type-directed
location abstraction that handles slots, byte offsets, packed reads, and
packed writes. Outlined helpers will be named operations keyed by their
semantic signature, generated lazily, and allowed to be inlined by later
passes.
