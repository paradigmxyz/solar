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
of that boundary for semantic contract creation.

## Replacement shape

The replacement is split into stateful, private components:

* `FunctionLowerer` owns one function's HIR context, typed value environment,
  loop targets, return bindings, and `FunctionBuilder`.
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

## Verified replacement slice

The current slice compiles the workspace and has been exercised against the
existing scalar and packed-storage MIR fixtures. It supports:

* scalar literals, local bindings, returns, arithmetic, comparisons, shifts,
  logical and bitwise operations, assignments, compound assignments, and
  pre/post increment of scalar l-values;
* typed external ABI metadata for scalar, enum, byte, array, and tuple shapes;
* state-variable reads and writes through the shared storage-location object;
* packed unsigned, signed, address, enum, and fixed-bytes storage fields;
* nested structs, mappings, dynamic arrays, and short and long storage bytes;
* explicit state-variable initializers, including a synthetic constructor when
  the contract has no explicit constructor;
* loop-carried scalar and storage-reference environments, including `break` and
  `continue` exits;
* split Solidity and Yul builtin lowering with shared positional-argument,
  arity, named-argument, and unsupported-builtin diagnostics;
* Yul arithmetic, environment, memory, calldata, storage, logging, call,
  creation, termination, and hashing builtins through direct MIR operations;
* calldata slice parameters and `.offset`/`.length` access and assignment in
  both ordinary Solidity expressions and inline assembly;
* constructor and fallback/function attributes needed by the backend;
* inherited public and internal function discovery with selector de-duplication;
* most-derived virtual resolution for internal calls, `super` calls, and
  overridden modifiers, while retaining shadowed public bodies as internal MIR
  targets;
* explicit and synthetic base-constructor lowering in linearized order, including
  constructor argument binding and storage-to-memory aggregate copies;
* block, transaction, message, `blockhash`, `blobhash`, and enum/integer
  `type(...).min`/`type(...).max` builtins through typed MIR operations;
* compile-time ERC-165 interface IDs from the sema interface-function set;
* positional multi-value declarations and assignments, including evaluation of
  discarded tuple values for their side effects;
* low-level and typed external calls, including returndata capture and EVM-version
  checks;
* payable address `send` and `transfer`, including the EVM value stipend and
  forwarding of transfer failures;
* contract creation with compiled child deployment bytecode, semantic
  constructor ABI encoding, `value`/`salt` options, and forwarding of failed
  creation returndata;
* `abi.decode` scalar, tuple, dynamic-array, and calldata-slice paths through
  semantic memory slices and object copies; struct targets still fail closed;
* `ecrecover`, `sha256`, and `ripemd160` through version-aware precompile
  calls and semantic memory objects;
* `string.concat` and `bytes.concat` through one variadic packed-memory path,
  including empty, literal, dynamic, and fixed-bytes pieces;
* lazy, deduplicated ABI cleanup helpers and outlined revert helpers.

The generated MIR for `tests/ui/codegen/lowering/compound_assign.sol` contains
the expected `sload`, arithmetic, and `sstore` sequence, and does not contain a
free-memory-pointer allocation. `cargo check --workspace` and `cargo fmt --all`
pass for this slice. The storage-array runtime fixture also matches Solc 0.8.35
for direct, push, lvalue, nested-lvalue, bytes, and mapping cases.

## Remaining work

The rewrite is not full codegen. The following are explicit next stages, each
to be backed by Solc comparisons and existing UI or runtime infrastructure:

1. Complete the remaining aggregate ABI decoding and encoding, including
   structs, nested aggregates, and aggregate returns.
2. Finish base-constructor argument forwarding for indirect and unresolved
   constructor arguments, and cover the remaining constructor modifier edge
   cases with Solc-backed runtime tests.
3. Add the remaining call and language features: events, Yul statements,
   immutables, and function-pointer dispatch.
4. Finish storage-reference CFG merging for every aggregate shape and audit
   checked arithmetic and allocation guards against the corresponding Solc
   behavior.
5. Add and run differential, UI, and runtime tests for every new semantic
   slice, then run the complete existing test and Solc suites before declaring
   the rewrite complete.

Unsupported HIR emits a diagnostic and leaves an `invalid` MIR terminator in the
rejected function. This is a deliberate fail-closed boundary; it must not be
replaced with a stop, zero values, or silent miscompilation.
