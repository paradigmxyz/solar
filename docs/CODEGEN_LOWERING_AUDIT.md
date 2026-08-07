# Codegen lowering audit

This document is the design record for the lowering rewrite. It describes the
tree after the legacy implementation was removed and the replacement work that
has been verified so far.

## What the old lowering got wrong

The deleted implementation put HIR traversal, function construction, ABI
encoding and decoding, storage layout, memory allocation, helper synthesis,
inline calls, modifiers, and backend-specific operations in one mutable
`Lowerer`. The result had several concrete problems:

* ABI work happened while walking HIR, so the same calldata and return rules
  were repeated for external functions, constructors, calls, errors, and
  events.
* Raw `mload`, `mstore`, calldata, returndata, `sload`, and `sstore` operations
  were emitted from many pattern-specific branches. The layer could not state
  which representation was valid at each MIR phase.
* Memory allocation depended on ad hoc free-memory-pointer reads and manual
  pointer arithmetic. This mixed allocation policy with expression lowering.
* Storage offsets, packed reads, and packed writes were calculated in several
  unrelated paths. Nested structs, arrays, bytes, and references did not share
  one location query.
* Helpers were inserted by whichever branch happened to need one. There was
  no semantic key for deduplication or a clear rule for when a trivial cleanup
  should remain inline.
* Modifiers, constructors, inherited functions, and synthetic entry points
  were handled as special cases in the same context rather than as explicit
  lowering stages.
* The context exposed a broad sibling-facing method surface. Removing one
  helper required searching unrelated lowering files, which made stale paths
  easy to keep alive.

The rewrite does not restore those modules or copy their control flow. The
legacy files were removed in commit `d43240e92` after checking their only
production callers.

## Verified public boundary

`crates/codegen/src/lower/mod.rs` exposes only:

* `lower_contract(Gcx, ContractId) -> Module`;
* `lower_contract_with_bytecodes(Gcx, ContractId, &FxHashMap<ContractId, Bytes>) -> Module`;
* `lower_contract_with_bytecodes_and_runtime` adds child runtime artifacts for
  `type(...).runtimeCode`.

The first is used by MIR tests. The second is used by contract compilation and
the benchmark harness. A companion entry point also carries child runtime
bytecodes for `type(...).runtimeCode`; deployment bytecodes remain part of the
boundary for semantic contract creation.

## Replacement shape

The replacement is split into stateful, private components:

* `FunctionLowerer` owns one function's HIR context, typed value environment,
  loop targets, return bindings, and `FunctionBuilder`.
  ABI value and packed encoding helpers live in the child
  `function/abi_values` module, leaving the main walker with only the
  dispatch points and shared argument materialization it needs.
  ABI call-argument typing and calldata materialization live in the sibling
  `function/abi_calls` module.
  Solidity, Yul, and address builtin dispatch live in `function/builtins`,
  with raw op emission limited to the explicit inline-assembly boundary.
  Modifier and base-constructor expansion live in `function/modifiers`, with
  continuation blocks carrying the placeholder and return semantics.
  Checked arithmetic, allocation overflow, and index bounds checks live in the
  shared `function/checks` module instead of being repeated by each lowering
  path.
  Memory-backed arrays, tuples, literals, and zero-initialized aggregate
  defaults live in `function/memory_values`.
  Storage-reference access, packed indexing, array push/pop, and aggregate
  storage copies live in the child `function/storage_values` module instead of
  in the main HIR expression walker.
* `TypeLowerer` owns recursive aggregate-shape state and produces MIR types and
  ABI descriptors. Recursive structs fail closed instead of recursing forever.
* `StorageBuilder` computes one base-to-derived layout through a stateful
  `StorageCursor`. `StorageLayout` owns packed-field reads and read-modify-write
  stores, including signed and fixed-bytes normalization.
* `LowerAbiCx` owns ABI wrapper construction and a lazy cleanup-helper registry.
  `OutlineRevertsCx` owns the corresponding registry for shared revert paths.
  Both registries key helpers by their semantic shape and leave trivial `u256`
  operations inline.
* `contract` only discovers functions, assigns function attributes and
  selectors, and assembles the module.

HIR lowering emits typed scalar MIR and semantic storage operations. External
functions retain typed parameters, ABI parameter shapes, and ABI return
layouts in built MIR. The existing `lower-abi` pass remains responsible for
calldata wrappers, decoding, and external termination. No new lowering code
reads or updates the free-memory pointer. Contract creation receives compiled
child deployment bytecode through the public lowering boundary and appends
semantic ABI-encoded constructor arguments before emitting `create` or
`create2`.
Multi-return buffers use the same semantic fixed-array objects and frame-slot
operations in HIR lowering, ABI lowering, and generated function-pointer
dispatchers. Their consumers carry the complete logical slice length instead
of rebuilding a raw address for each word.
External-call, linked-library, function-pointer, `try` return, and
`catch Error(string)` payload decoding now emits `AbiDecode` in HIR. The ABI
pass expands each instruction through the shared checked decoder and publishes
additional tuple values through the multi-return frame slot.
Typed memory and calldata-slice copies, byte indexing, ABI scalar words, and
selector dispatch use semantic slice loads; direct memory opcodes remain only
for inline assembly, explicit revert payload construction, and operations
materialized by the ABI/backend boundary. High-level returndata capture uses a
semantic `returndata_size` value until `lower-abi` rewrites it to the EVM query.
Failed external calls use a `revert_returndata` MIR terminator until `lower-abi`
selects the EVM-version behavior and emits the returndata copy and revert.

## Verified replacement slice

The current slice compiles the workspace and has been exercised against the
existing scalar and packed-storage MIR fixtures. It supports:

* scalar literals, local bindings, returns, arithmetic, comparisons, shifts,
  logical and bitwise operations with short-circuit control flow, assignments,
  compound assignments, and pre/post increment of scalar l-values;
* checked scalar add, sub, mul, div, mod, negation, and exponentiation with
  Solidity panic payloads, explicit unchecked-block state, and narrow-type
  wrapping;
* typed external ABI metadata for scalar, enum, byte, array, and tuple shapes;
* nested ABI parameter locations, fixed-array constructor word decoding, and
  memory-shaped dynamic calldata returns;
* state-variable reads and writes through the shared storage-location object;
* packed unsigned, signed, address, enum, and fixed-bytes storage fields;
* left-aligned fixed-bytes values from literals, storage, state initializers,
  and `msg.sig`, with canonical narrowing and explicit integer and address
  conversions;
* nested structs, mappings, dynamic arrays, and short and long storage bytes;
* canonical short-storage bytes writes with unspecified memory padding masked
  before the length tag is persisted;
* storage `bytes` assignments that clear stale long-storage words when a value
  shrinks, and dynamic-array assignments that clear truncated element slots;
* storage `delete` for dynamic and fixed arrays, packed elements, structs, and
  nested storage objects through one recursive location-aware path;
* memory `delete` for dynamic and fixed arrays, bytes, and structs by rebinding
  only the selected reference, preserving aliases to the previous object;
* explicit state-variable initializers, including a synthetic constructor when
  the contract has no explicit constructor;
* modifier parameters that copy calldata arrays into independent memory objects
  before modifier-local mutation;
* loop-carried scalar and storage-reference environments, including `break` and
  `continue` exits through Solidity and Yul `for` updates, with conservative
  spill-based fallback for nested storage-reference phis;
* storage-reference branch, ternary, and loop merges that preserve slot and
  packed-offset state across differing exits;
* storage aggregate references copied through memory returns and internal or
  external calls, including fixed and dynamic storage arrays of words, bytes,
  and structs with nested dynamic members;
* multi-value returns that materialize storage-backed dynamic bytes into memory
  ABI objects;
* constructor-assigned immutable declarations and reads, including inherited
  immutables, typed deployment patching, and narrow immutable widths;
* split Solidity and Yul builtin lowering with shared positional-argument,
  arity, named-argument, and unsupported-builtin diagnostics;
* Yul arithmetic, environment, memory, calldata, storage, logging, call,
  creation, termination, and hashing builtins through direct MIR operations,
  including the Prague `extcall`, `extdelegatecall`, and `extstaticcall`
  instructions;
* Yul `switch` statements with default and fall-through-to-merge paths,
  left-aligned string and hex case words, and branch-local value merging;
* calldata slice parameters and `.offset`/`.length` access and assignment in
  both ordinary Solidity expressions and inline assembly, including indexed
  bytes and array slices, internal slice returns, and external `this` calls
  that ABI-encode memory aggregates;
* `fallback(bytes calldata) returns (bytes memory)` through an explicit
  argument-free wrapper, full-calldata slice, and raw returndata body;
* constructor and fallback/function attributes needed by the backend;
* inherited public and internal function discovery with selector de-duplication;
* reachable internal library calls, including `using for` receiver binding;
* linked public library calls through ABI-encoded `DELEGATECALL`, including
  storage-reference slot forwarding, nested library hops, aggregate arguments,
  aggregate returns, and revert-data bubbling;
* most-derived virtual resolution for internal calls, `super` calls, and
  overridden modifiers, while retaining shadowed public bodies as internal MIR
  targets;
* explicit and synthetic base-constructor lowering in linearized order, including
  constructor argument binding and storage-to-memory aggregate copies;
* block, transaction, message, `blockhash`, `blobhash`, and enum/integer
  `type(...).min`/`type(...).max` builtins through typed MIR operations;
* enum member constants and checked integer-to-enum conversions with Solidity's
  `Panic(0x21)` range payload;
* compile-time ERC-165 interface IDs from the sema interface-function set;
* positional multi-value declarations and assignments, including full
  right-hand-side evaluation before tuple stores and evaluation of discarded
  tuple values for their side effects;
* low-level and typed external calls, including contract-typed receiver dispatch,
  source-order receiver, low-level call-option, and argument evaluation, explicit
  `gas`/`value` options, contract-address conversions, returndata capture, and
  EVM-version checks, including modern-EVM returndata length and scalar
  canonicality validation;
* positional and named struct constructors with declaration-order field layout;
* one-time l-value resolution for compound assignments and increments, plus
  argument-before-length evaluation for storage-array and storage-bytes pushes;
* external function-pointer calls with aggregate return decoding through the
  shared ABI path, explicit `gas`/`value` options, and `.address` extraction;
* event emission with overload and named-argument resolution, selector and indexed
  topics, dynamic-topic hashing, static and word-array aggregate-topic hashing,
  nested dynamic array, struct, string, and bytes topic hashing, in-place
  aggregate function members, dynamic aggregate diagnostics, indexed external
  function pointers, and MIR ABI data;
* `revert` and `require` payloads for `Error(string)` and custom errors through
  semantic ABI encoding, including named arguments and exact argument checks;
* payable address `send` and `transfer`, including the EVM value stipend and
  forwarding of transfer failures;
* contract creation with compiled child deployment bytecode, semantic
  constructor ABI encoding, `value`/`salt` options, source-order option and
  argument evaluation, and forwarding of failed creation returndata;
* `abi.encodePacked` arrays of static word elements and external function
  values, including nested fixed arrays and dynamic arrays of fixed arrays,
  indexed and struct-field array values, storage-backed short and long bytes,
  address `code` and `codehash` builtins, `type(...).creationCode` and
  `runtimeCode`, and fixed-bytes to address conversions;
* `try`/`catch` lowering for resolved external calls with scalar and aggregate
  return bindings, ordered selector dispatch for multiple catches, bare
  catches, `catch (bytes memory)` returndata objects, and
  `catch Error(string memory)`/`catch Panic(uint256)` selector and payload
  checks, including source-order receiver, `gas`/`value` options, and argument
  evaluation, plus constructor creation with `CREATE`/`CREATE2` failure
  dispatch;
* internal function-pointer values and shape-specific dispatchers for exact,
  virtual, and `super` targets, including storage-backed values, higher-order
  returns, memory arrays, and multi-return calls;
* multi-return values published through semantic frame slots and fixed-array
  objects across external calls, linked-library calls, ABI decoding, and
  internal function-pointer dispatch;
* `abi.decode` scalar, tuple, struct, fixed- and dynamic-array, and
  calldata-slice paths through semantic memory slices and object copies;
* dynamic calldata arrays of multiword static elements, including storage
  copies and ABI encoding of nested fixed arrays;
* calldata fixed-array indexing of structs without eagerly validating unused
  dynamic fields;
* dynamic and fixed memory arrays of structs with nested bytes and dynamic
  arrays, including lazy zero-initialization of aggregate elements from `new`
  allocations and their nested dynamic objects;
* calldata structs and dynamic arrays with `bytes` members copied to memory,
  including checked length and exact payload bounds; direct `bytes` and
  `string` calldata slices copied to memory, indexed, converted to fixed bytes,
  or passed through ABI encoding, packed encoding, hashing, calls, and events
  validate their exact source range;
* `abi.encodeCall` arguments typed from the resolved callee with scalar
  coercions, including dynamic structs, string literals, and fixed bytes;
* string literals coerced to fixed-bytes values in comparisons and byte-array
  element stores, with canonical alignment at each boundary;
* Solc-compatible dynamic ABI offset bounds, including valid zero offsets
  and overflow/range rejection;
* `ecrecover`, `sha256`, and `ripemd160` through version-aware precompile
  calls and semantic memory objects;
* ERC-7201 namespace hashing for literal, memory, calldata, and storage-string
  arguments through semantic bytes objects, with Solc-backed runtime checks;
* `string.concat` and `bytes.concat` through one variadic packed-memory path,
  including empty, literal, dynamic, fixed-bytes, and storage-backed pieces;
* lazy, deduplicated ABI cleanup helpers and outlined revert helpers.

The generated MIR for `tests/ui/codegen/lowering/compound_assign.sol` contains
the expected `sload`, arithmetic, and `sstore` sequence, and does not contain a
free-memory-pointer allocation. `cargo check --workspace` and `cargo fmt --all`
pass for this slice. The storage-array runtime fixture also matches Solc 0.8.35
for direct, push, lvalue, nested-lvalue, bytes, and mapping cases. The
Foundry projects `abi-encoding`, `stress-arrays`, `stress-inheritance`, and
`stress-modifiers` now run in the default differential suite. Their Solar tests
match Solc, including multi-value ABI decoding, dynamic-array clearing, and
fixed-array deletion.

## Remaining work

The rewrite is not full codegen. The following are explicit next stages, each
to be backed by Solc comparisons and existing UI or runtime infrastructure:

1. Differentially exercise the aggregate ABI paths against Solc with
   independent nested, mixed-tuple, and malformed vectors. The current runtime
   corpus covers valid flat, mixed-tuple, typed and decoded
   dynamic-struct-array, fixed-dynamic-array, and nested dynamic-array-plus-
   bytes vectors; malformed short and invalid-offset inputs; and
   Solc-compatible zero-offset round trips. Storage-backed ABI encoding now
   also covers dynamic arrays of
   multiword fixed elements and fixed nested arrays. The calldata materializer
   uses each element's ABI head width, so static elements wider than one word do
   not overlap.
   `abi_calldata_nested_reencode.sol` adds Solc-checked hashes for a three-level
   dynamic array and an outer dynamic array of fixed pairs of dynamic arrays.
   `abi_calldata_overlapping.sol` adds the Solc aliasing case where two nested
   dynamic-array heads share a tail, alongside its malformed-tail rejection.
   `abi_calldata_nested_validation.sol` now checks Solc's distinction between
   shallow validation for an unused nested array and deep validation when an
   inner element is read.
   `abi_calldata_nested_static_middle.sol` covers the same distinction when a
   dynamic array contains a fixed array whose dynamic child is still absent:
   indexing the fixed array checks only its head, while indexing the child
   validates the nested tail. The fixture also checks materialization at memory
   assignment and internal-call boundaries.
   `abi_calldata_scalar_validation.sol` adds Solc's invalid narrow-word vectors:
   calldata `uint8[]` values are checked when indexed and when re-encoded.
   `abi_calldata_stride_validation.sol` checks the element stride for full-word
   calldata arrays when they are read or re-encoded.
   `abi_calldata_array_size_validation.sol` checks that a dynamic calldata array
   rejects a length whose element payload exceeds the available calldata.
   `abi_packed_calldata_scalar_validation.sol` applies the same check to
   `abi.encodePacked`, which must clean each array element instead of copying
   dirty calldata words.
   `abi_calldata_static_tuple_lazy_validation.sol` covers static calldata
   structs: field reads validate only the selected scalar, unused dirty fields
   remain lazy, and full-word tuples materialize before ABI encoding.
   `abi_calldata_static_struct_validation.sol` checks canonical int16, uint8,
   and bytes2 words in a static memory-struct parameter.
   `abi_calldata_fixed_middle_short.sol` covers a truncated nested dynamic
   array whose fixed-array head is wider than one word; indexed access rejects
   the short payload while the valid Solc vectors still decode.
   `abi_calldata_fixed_middle_short_reencode.sol` forwards the same canonical
   long and Solc-short payloads through an external self-call and rejects a
   missing fixed-array head word.
   `abi_calldata_fixed_dynamic.sol` covers a top-level fixed array of dynamic
   bytes, including exact unpadded tails and a missing nested tail.
   `abi_calldata_unused_aggregate_validation.sol` covers the other lazy
   boundary: unused dynamic bytes and structs validate their immediate heads,
   while nested dynamic offsets remain lazy like Solc.
   `abi_calldata_lazy_struct_array.sol` checks that indexing a calldata struct
   array leaves an unused sibling dynamic field lazy while selected dynamic
   fields still enforce their payload bounds.
   `abi_calldata_struct_dynamic.sol` checks ABI re-encoding of one and two
   dynamic structs with word-array members.
   `calldata_struct_dynamic_memory.sol` checks copying the same aggregate
   shape from calldata into memory before reading nested values.
   `calldata_nested_element_memory.sol` covers nested dynamic and fixed-array
   elements selected from calldata and copied into memory before indexing.
   `abi_encode_memory_calldata_v1.sol` checks the mixed memory-array and
   calldata-bytes encoder through Solc's v1 ABI mode, and
   `abi_encode_rational.sol` checks signed rational literal encoding.
   `abi_encode_empty_string.sol` checks that empty string literals retain a
   dynamic `bytes` ABI shape in both full and packed encoding.
   `abi.decode`, external return decoding, and `catch Error(string)` payload
   decoding are represented by semantic MIR operations and lowered by the ABI
   pass; multi-return materialization uses the shared frame/object path. The
   indexed event path now flattens nested dynamic arrays, structs, and padded
   string or bytes members for topic hashing. Tuples and other unsupported
   aggregate shapes still fail closed until their in-place encoding rules are
   defined.
   remaining work is broader independent differential coverage. Lazy wrapper
   validation still does not reject every Solc-invalid aggregate vector; eager
   validation for shapes beyond the covered nested-array heads needs a separate
   design so it does not force materialization.
2. Extend base-constructor argument forwarding coverage to the remaining
   unresolved and constructor-modifier edge cases. Inherited constructor
   arguments that call direct or virtual functions now have Solc-backed
   run-call coverage. Virtual constructor modifiers and library modifiers with
   storage parameters now have differential runtime coverage as well. The
   `modifier_return_postlude.sol` vector checks that a return still runs the
   modifier postlude. `constructor_dynamic_array_forwarding.sol` ports the
   dynamic `address[]` path through a derived constructor, a base constructor,
   and storage before reading it back. The
   `constructor_nested_aggregate_forwarding.sol` vector extends this to a
   nested `struct[]` with dynamic `bytes` fields and checks the copied storage
   values after construction. `constructor_modifier_creation_context.sol`
   covers virtual modifier dispatch and public calls made while the base
   constructor is still running. `constructor_inheritance_init_order.sol` and
   `constructor_state_variable_order.sol` cover base state initialization,
   derived initializers, implicit bases, and constructor-body reads in
   linearized order. `constructor_diamond_forwarding.sol` covers argument
   forwarding through a diamond with a shared base. `modifier_return_reference.sol`
   covers modifier arguments that assign the function's named return variables
   before the body runs. `modifier_named_arguments.sol` covers named modifier
   argument binding. `constructor_fixed_array_forwarding.sol` covers
   fixed-array constructor decoding and the memory-to-storage copy.
   `constructor_external_arguments.sol` covers direct deployment decoding of
   packed `bytes3` and boolean constructor parameters.
   `constructor_bytes_forwarding.sol` covers dynamic bytes forwarded through an
   internal base-constructor helper, while `constructor_internal_arguments.sol`
   covers fixed-byte literals and booleans through child creation. The
   `constructor_base_fixed_bytes.sol` vector covers the same fixed-byte literal
   conversion through an inherited base constructor, and
   `constructor_function_call_fixed_bytes.sol` covers a direct internal call
   from a constructor.
3. Extend function-pointer ABI coverage to the remaining edge cases. External
   pointers now have runtime coverage for aggregate arguments and returns,
   pointer arguments and pointer returns, including aggregate `try` return
   bindings. Custom error catch clauses are rejected by the tracked Solidity
   type checker and are not a valid lowering target. Creation and runtime code
   literals now use compiled child artifacts. External pointers also have
   runtime coverage inside memory structs and arrays, storage structs and
   arrays, and memory-signature pointers targeting calldata implementations.
   `external_function_pointer_memory_type.sol` checks the direct assignment of
   a calldata function to a memory-typed external pointer before the call.
   `function_selector_ternary.sol` checks selector extraction after choosing
   between two external pointers at runtime.
   `function_selector_side_effect.sol` checks that a call used to obtain a
   statically resolved selector still runs before the selector value is read.
   `internal_function_pointer_calldata.sol` checks calldata slices through
   internal pointer dispatch, and `mapping_internal_function_pointer.sol`
   checks mapping-backed internal pointer state transitions.
   `internal_function_pointer_multislot.sol` covers internal dispatch with
   function-pointer parameters occupying multiple argument slots.
   `internal_function_pointer_library.sol` covers a library reducer receiving
   an internal callback and a memory-array argument.
   `constructor_function_pointer_dispatch.sol` covers internal callback
   dispatch during construction, including a fixed-byte return value.
   `external_function_pointer_calldata_array.sol` adds a Solc-checked calldata
   array decode and re-encoding vector. `abi_function_pointer_validation.sol`
   ports Solc's canonical 24-byte pointer checks: unused calldata structs stay
   lazy, while memory structs and accessed calldata fields reject high-byte
   garbage. `abi_function_pointer_array_validation.sol` covers the same lazy
   versus eager distinction for dynamic function-pointer arrays and verifies
   that memory arrays re-encode canonical pointer words.
   `constructor_function_pointer.sol` covers an external function pointer passed
   through contract creation and called from the child constructor.
   `abi_packed_function_pointer_array.sol` covers canonical pointer cleanup when
   an external function array is re-encoded with `abi.encodePacked`.
   `abi_forward_function_pointer_array.sol` covers cleanup and validation when a
   lazy calldata pointer array crosses an external-call boundary.
   `external_function_pointer_storage_array.sol` covers memory-to-storage and
   storage-to-storage copies of external function pointer arrays.
   `external_function_pointer_storage_struct.sol` covers packed external
   function pointers embedded between scalar fields in storage structs.
   `external_function_pointer_options.sol` checks source-order evaluation and
   forwarding of `gas`/`value` options on external pointer calls.
   `function_pointer_inline_array_options.sol` covers an external pointer
   selected from an inline array before forwarding a `value` option, and
   `function_pointer_dirty_bits.sol` covers Yul `.address`/`.selector`
   assignments that must clean high bits before pointer comparison.
   `function_pointer_delete.sol` and `external_function_pointer_delete.sol`
   cover clearing internal and external storage pointers, including Solc's
   zero-internal-pointer panic.
   `internal_function_pointer_storage_copy.sol` covers copying a fixed array of
   internal function pointers from memory into storage and the zero-pointer
   panic on a subsequent storage dispatch.
   `internal_function_pointer_dynamic_storage_copy.sol` extends that coverage
   to dynamic storage arrays allocated with `new` and copied between storage
   variables before dispatch.
   `internal_function_pointer_storage_struct.sol` covers packed internal
   function pointers embedded between scalar fields in storage structs, and
   `external_function_pointer_parameter.sol` covers direct and internal calls
   that pass external pointers as parameters.
   The Unifap creation fixture now passes the differential Foundry
   suite; the companion fixture compiles with no Solar-only regressions, but
   retains seven pre-existing failures under both compilers because its
   hard-coded CREATE2 init-code hash does not match the current pair bytecode.
4. Storage-reference CFG tests now cover packed struct fields, mapping-pointer
   rebinding, and Yul `.slot`/`.offset` access. Direct dynamic bytes and array
   overflow cases now have exact `Panic(0x41)` run-call checks, as do nested
   dynamic arrays and arrays of dynamic structs. Malformed short and long
   storage-byte headers now reject with Solc's exact `Panic(0x22)` payload.
   Extend this coverage to the remaining aggregate allocation shapes against
   Solc; dynamic and fixed memory arrays of nested structs now have independent
   run-call coverage,
   dynamic storage arrays of words, bytes, and static structs now have typed
   external-call coverage, dynamic storage arrays of structs with nested bytes
   and arrays now have typed external-call coverage, and storage copies of
   nested fixed arrays have an ABI-encoding fixture. `storage_nested_struct_calldata.sol`
   adds a nested calldata struct-array copy with dynamic and fixed members.
   `storage_nested_struct_memory.sol` covers the same three nested struct-array
   shapes through memory-to-storage assignment. `storage_array_to_mapping.sol`
   covers nested dynamic arrays copied from calldata, storage, and memory into
   mapping elements. `storage_nested_dynamic_calldata_to_storage.sol` covers
   nested dynamic calldata arrays assigned to storage, including the second-level
   array shape. `storage_struct_two_bytes.sol` covers a packed struct with two
   dynamic byte fields whose lengths cross the short and long storage boundary.
   `storage_struct_dynamic_words_calldata.sol` adds a dynamic struct-array
   copy whose nested arrays use full-width words. The
   `storage_nested_dynamic_words_calldata.sol` vector covers dynamic and fixed
   nested word arrays without a struct wrapper. External function pointers use
   Solc's 24-byte packed storage representation, and internal pointers use its
   8-byte representation; following fields and fixed pointer arrays share the
   remaining bytes in each word.
   `nested_array_memory_storage.sol` covers memory-to-storage copies where
   fixed nested arrays widen into dynamic or larger fixed storage arrays, and
   `nested_array_element_memory_storage.sol` covers selected nested fixed-array
   elements copied into storage. `array_copy_memory_storage.sol` covers scalar
   fixed and dynamic memory arrays copied into storage, while
   `nested_storage_memory_copy.sol` and
   `nested_storage_memory_pointer_copy.sol` cover packed nested fixed arrays
   copied from direct and reference-based storage values into memory.
   `array_struct_memory_storage.sol` covers memory-to-storage copies of
   structs with packed fixed arrays and nested dynamic arrays, while
   `storage_memory_nested_struct.sol` covers the reverse copy through a
   packed struct array. `storage_storage_array_conversion.sol` covers
   storage-to-storage copies across scalar widths and nested packed fixed-array
   bases. `storage_array_different_packing.sol` covers a `bytes8[]` to
   `bytes10[]` storage conversion with elements spanning different packing
   widths. `array_copy_clear_storage.sol` and
   `array_copy_clear_storage_packed.sol` port Solc's stale-tail cleanup checks
   for full-word and packed dynamic arrays. `array_copy_cleanup_uint40.sol`
   extends the packed case across multiple storage words.
   `storage_memory_nested_bytes.sol`
   covers short and long storage-byte elements copied through a dynamic storage
   array into memory. The
   `storage_struct_dynamic_copy.sol` vector covers storage structs and dynamic
   storage-array elements with `bytes` members, including indexed read/write
   isolation after an aggregate copy.
   `storage_struct_bytes_index.sol` covers indexed reads through an explicit
   `bytes(storage)` conversion on a nested storage field.
   `bytes_memory_storage.sol` vector checks memory-to-storage bytes writes with
   dirty unused memory bytes, matching Solc's truncation.
   `storage_delete_packed_array.sol` checks that deleting packed fixed-array
   storage references preserves the runtime element offset while clearing.
   `storage_delete_packed_struct.sol` extends that check through packed struct
   fields in dynamic arrays and mappings, including aggregate assignment.
   `storage_nested_element_copy.sol` covers selected nested storage-array
   elements copied into dynamic and fixed-shaped storage targets, including
   empty nested objects from a zero-initialized memory array.
   `storage_packed_array_copy.sol` checks packed fixed-array copies across
   different element widths, and `storage_boundary_array_assignment.sol`
   checks fixed multi-slot storage references returned from Yul and rebound at
   a negative slot boundary. `storage_boundary_array_copy.sol` extends this to
   two independently returned storage references and an aggregate copy across
   that boundary. `storage_boundary_struct_array_multislot.sol` covers the
   same returned-reference path for structs spanning multiple storage slots.
   `storage_boundary_struct_array_packed.sol` covers packed four-field structs
   across the same boundary, including copying, deletion, and canary
   preservation.
   `storage_boundary_struct_array_mixed.sol` extends that boundary coverage to
   mixed-width fields spanning four storage slots.
   `storage_boundary_array_partial_assignment.sol` covers short array-literal
   assignment into a fixed storage reference.
   `storage_boundary_array_packing_not_overlapping_variable.sol` checks packed
   boundary cleanup without clobbering the following storage slot.
   `storage_boundary_array_overlap.sol` covers the deliberate overlapping
   fixed-array/state-variable boundary and its cleanup behavior.
   `storage_return_pointer_multi.sol` checks tuple assignment from an internal
   call returning multiple storage references before mutating both targets.
   `storage_return_pointer_mixed.sol` and
   `storage_return_pointer_mixed_decl.sol` cover mixed scalar/storage returns
   through tuple assignment and multi-variable declarations.
   `storage_return_pointer_direct.sol` covers direct single-reference returns
   through conditional storage rebinding and member writes.
   `storage_string_bytes_conversion.sol` covers a constructor string copied to
   storage and then converted to `bytes` before reading its length.
5. Keep expanding runtime and differential coverage for aggregate allocation
   shapes and constructor/modifier edges. The UI snapshots are now in sync
   with the rewrite, and the full `cargo tq ui` suite passes. ERC-7201
   literal, memory, calldata, and empty-storage arguments now have runtime
   vectors. `fixed_bytes_storage.sol` checks packed storage, state
   initialization, `msg.sig`, and Solc-compatible numeric returns.
   `fixed_bytes_conversions.sol` checks fixed-byte widening, narrowing, and
   numeric conversions; the remaining work is broader aggregate and
   constructor coverage. `memory_nested_arrays.sol` adds Solc-backed
   three-dimensional dynamic memory allocation and assignment coverage, and
   `memory_aggregate_allocation.sol` covers zero-initialized arrays of fixed
   arrays and structs with nested dynamic byte objects.
   `memory_multiple_dynamic_arrays.sol` checks that separate nested dynamic
   allocations stay independent while their untouched elements remain zeroed.
   `nested_struct_memory_alias.sol` checks that nested memory aggregate
   assignment preserves Solidity's reference aliasing.
   `storage_array_assignment_cleanup.sol` checks the raw storage tail after a
   dynamic-array replacement, matching Solc's cleared slots.
   `nested_dynamic_storage_cleanup.sol` checks recursive cleanup when a
   truncated array contains dynamic-array elements.
   `require_modulo_chain.sol` checks modulo inside chained `require` conditions,
   including the divide-by-zero panic and short-circuit paths; the same cases
   now run in the control-flow Foundry differential project.

The current intentional boundary is dynamic-element calldata array slicing.
Solidity rejects range access for arrays with dynamically encoded base types,
and the MIR slice representation keeps only a pointer and length, so it cannot
recover the original base needed to resolve nested dynamic ABI offsets.

Generated internal function-pointer dispatchers carry an explicit dispatcher
attribute. The inliner keeps a shared dispatcher intact unless a constant
pointer permits specialization, while still allowing its normal single-call
policy. Cleanup and revert helper registries remain deduplicated by semantic
shape without imposing an inline attribute.

Unsupported HIR emits a diagnostic and leaves an `invalid` MIR terminator in the
rejected function. This is a deliberate fail-closed boundary; it must not be
replaced with a stop, zero values, or silent miscompilation.
