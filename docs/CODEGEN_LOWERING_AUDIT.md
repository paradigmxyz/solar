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
* `lower_contract_with_bytecodes(Gcx, ContractId, &FxHashMap<ContractId, Bytes>) -> Module`.

The first is used by MIR tests. The second is used by contract compilation and
the benchmark harness. Child bytecodes are deployment bytecode and remain part
of that boundary; creation lowering will consume them after the creation
operation is implemented.

## Replacement shape

The replacement is split into stateful, private components:

* `FunctionLowerer` owns one function's HIR context, typed value environment,
  loop targets, return bindings, and `FunctionBuilder`.
* `TypeLowerer` owns recursive aggregate-shape state and produces MIR types and
  ABI descriptors. Recursive structs fail closed instead of recursing forever.
* `StorageBuilder` computes one base-to-derived layout. `StorageLayout` owns
  packed-field reads and read-modify-write stores, including signed and
  fixed-bytes normalization.
* `contract` only discovers functions, assigns function attributes and
  selectors, and assembles the module.

HIR lowering emits typed scalar MIR and semantic storage operations. External
  functions retain typed parameters, ABI parameter shapes, and ABI return
  layouts in built MIR. The existing `lower-abi` pass remains responsible for
  calldata wrappers, decoding, and external termination. No new lowering code
  reads or updates the free-memory pointer.

## Verified replacement slice

The current slice compiles the workspace and has been exercised against the
existing scalar and packed-storage MIR fixtures. It supports:

* scalar literals, local bindings, returns, arithmetic, comparisons, shifts,
  logical and bitwise operations, assignments, compound assignments, and
  pre/post increment of scalar l-values;
* typed external ABI metadata for scalar, enum, byte, array, and tuple shapes;
* state-variable reads and writes through the shared storage-location object;
* packed unsigned, signed, address, enum, and fixed-bytes storage fields;
* basic conditional and loop CFG construction with scoped environments;
* constructor and fallback/function attributes needed by the backend.

The generated MIR for `tests/ui/codegen/lowering/compound_assign.sol` contains
the expected `sload`, arithmetic, and `sstore` sequence, and does not contain a
free-memory-pointer allocation. `cargo check --workspace` and `cargo fmt --all`
pass for this slice.

## Remaining work

The slice is not full codegen. The following are explicit next stages, each to
be backed by solc comparisons and existing UI or runtime infrastructure:

1. Lower memory objects, slices, arrays, bytes, structs, and aggregate copies
   through semantic MIR operations. Physical memory selection belongs to the
   existing memory-lowering boundary.
2. Add storage aggregate locations for nested structs, arrays, mappings, dynamic
   arrays, and storage bytes, including dynamic slot addressing.
3. Expand modifiers as source-order chains with explicit return continuations;
   lower base-constructor calls and state initializers in constructor order.
4. Add internal and external calls, multi-return values, builtins, events,
   reverts, Yul, contract creation, immutables, and function-pointer dispatch.
5. Add a lazy helper registry keyed by semantic operation. Trivial `u256`
   cleanup stays inline; nontrivial checked conversions and shared error paths
   are named, deduplicated helpers without forced `NO_INLINE`.
6. Add and run differential/UI/runtime tests for every new semantic slice, then
   run the complete existing test and solc suites before declaring the rewrite
   complete.

Unsupported HIR currently emits a diagnostic and omits that function from the
returned module. This is a deliberate fail-closed boundary while the stages
above are implemented; it must not be replaced with zero values or silent
miscompilation.
