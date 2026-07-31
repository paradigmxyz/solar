# Codegen special-case audit

HIR-to-MIR lowering should preserve Solidity semantics and choose a MIR
representation. Optimization belongs in MIR or EVM IR passes. Backend lowering
should only handle target representation, stack scheduling, and assembly.

This audit covers `crates/codegen/src/lower`, the MIR pass pipeline, and the EVM
backend. It records the remaining exceptions so they do not become accidental
architecture.

## Removed

| Lowering behavior | Owner |
| --- | --- |
| Profitability-based inlining of ordinary internal and unlinked-library calls | MIR `inline` |
| A second statement interpreter for straight-line calldata-slice helpers | Normal statement lowering, pending removal after slice returns enter MIR |
| Re-reading the literal length in `new T[](literal)` | `MemoryObjectLen`, forwarded by memory DSE |
| Scanning a named-return body to skip memory-struct initialization | Emit the semantic default; memory DSE removes overwritten initialization stores |
| A duplicate literal-string `keccak256` branch | The shared dynamic-bytes hash lowering |
| Separate tuple declaration and tuple assignment result extraction | One multi-value snapshot path |
| Name-based event signature construction and scalar-only event data stores | Sema event selectors and the shared ABI encoder |
| Separate call-option handling in low-level, high-level, and try calls | One external-call option and opcode emitter |
| Separate external-function-pointer opcode selection and failure forwarding | The shared mutability-aware external-call path |
| Per-builtin arity checks and argument collection through flattening iterators | Shared borrowed slice/array extractors that reject named arguments |
| Source-order iteration of named arguments in internal calls, libraries, structs, events, errors, and base constructors | Sema parameter sources plus one declaration-order mapper |
| Conservative internal-call frame sizes based on a function's total MIR value count | Deferred constants resolved from exact post-emission spill sizes |
| Target-dependent expansion of compiler-generated memory copies during HIR lowering | Semantic `mcopy` plus the required `lower-mcopy` legalization pass |
| Separate EVM-word evaluators in instruction simplification, SCCP, and bounded pure evaluation | One shared MIR constant evaluator |

The HIR inliner previously erased the callee return target for void calls.
`return;` in an exact-base call could therefore terminate the enclosing public
function. Ordinary calls now remain calls until the MIR inliner, where returns
are explicit CFG edges.

## Retained local folding

Lowering keeps a small set of constant fast paths that are encapsulated in the
helper which emits the corresponding semantic operation:

| Helper | Fold |
| --- | --- |
| Array index bounds checking | Skip a proven in-range check or emit the known panic without first building a comparison |
| ABI offset pointer construction | Emit the offset directly when the base is the immediate value zero |
| Checked exponentiation | Use the bounded constant-base algorithm when the base is already a MIR immediate |
| Literal byte/string consumers | Materialize known payload words directly and hash literal contents without first building a temporary memory object |

These folds avoid constructing short-lived instructions, memory objects, and
control flow. They do not scan surrounding statements, duplicate a general
evaluator, or make cross-function profitability decisions. Checked
exponentiation should eventually select its algorithm after SCCP; literal
payload handling can move behind a constant memory/object evaluator without
changing the source-level fast path.

Static event data also uses the shared ABI encoder at the current free memory
pointer without advancing it. `LOG` consumes the data immediately, so reserving
that memory would only add work for later passes. Dynamic event data keeps the
normal allocation path.

## Representation work that still lives in lowering

These paths are not profitability optimizations. Removing them now would leave
MIR that a required lowering pass or the backend cannot represent.

| Path | Why it remains | Exit condition |
| --- | --- | --- |
| Base-constructor body composition | Base initialization is still built into one derived-constructor body | Give constructors an explicit MIR composition/call representation |
| Calldata-slice return inlining | `lower-slices` expands parameters and arguments, but not return signatures | Expand slice return signatures and internal-call results |
| Desugared `for`-loop update recovery | HIR represents a `for` loop as a synthetic loop/conditional/block shape, so codegen must recover the update block to make `continue` execute it | Preserve the update expression and continue target explicitly through HIR or a named CFG-building pass |
| Internal-function-pointer dispatchers | MIR has a function value but no indirect call; lowering discovers address-taken HIR targets and synthesizes signature-specific switch dispatchers | Add an indirect-call operation, specialize constant targets, then lower remaining indirect calls in a required pass |
| Multi-return scratch buffer | MIR instructions expose one result, so additional external and internal call results travel through a published memory buffer | Add first-class multi-result values and call instructions |
| External ABI entry decoding and return encoding | `lower-abi` does not yet cover every dynamic or aggregate shape | Complete `lower-abi` and semantic return encoding |
| Storage packing, storage `bytes`, and memory-object construction | These define source-language representation | Keep as required named lowering passes |
| ABI word validation and bounds checks | These are observable Solidity semantics | Keep, but represent them as semantic MIR operations where useful |

The calldata-slice workaround still owns `InlineReturnCtx`, pending multi-return
values, a recursion walk, and some local-context save/restore. Once slice returns
cross calls, remove those slice-specific pieces rather than extending them.
Constructor composition still needs its separate inline stack and context
save/restore until constructors gain an explicit MIR representation.

## Correctness gaps

These are current miscompile or target-legality risks, not cleanup preferences.

### External boundaries

1. High-level external return data is treated as a fixed list of words.
   Dynamic arrays, dynamic bytes, nested aggregates, short returndata, and ABI
   word validation need a typed returndata decoder. Try calls and linked-library
   delegatecalls must use the same decoder.
2. Contract creation still appends constructor arguments with one `mstore` per
   expression. Dynamic and aggregate constructor arguments need the semantic ABI
   encoder. Creation failure now forwards revert data, but argument encoding
   remains incomplete.
3. Try/catch shares call construction and typed ABI return decoding with
   ordinary calls. Malformed `Error(string)` payloads currently revert during
   decoding instead of falling through to a lower-level catch clause.
4. Linked-library calls have a restricted, independent ABI encoder and duplicate
   return handling. Storage-slot values need to become supported inputs to the
   semantic ABI encoder.
5. Indexed event arrays and structs require Solidity's indexed-event in-place
   encoding before hashing. Dynamic bytes/string and ordinary value topics are
   handled; aggregate topics still need a dedicated semantic operation.
6. Applied modifiers are collected as dependencies but are not composed around
   the function body; modifier placeholders lower to no-ops. This can omit
   access-control, validation, and reentrancy logic entirely.
### Target and storage representation

1. Target-specific MIR is not exhaustively legalized. Pre-Constantinople
   shifts, pre-Byzantium try/catch and explicit Yul returndata/static-call
   operations, and later opcodes such as `CREATE2`, `EXTCODEHASH`, transient
   storage, blob operations, and `CLZ` can still reach a target where their
   opcode is unavailable. `lower-mcopy` is the model: keep the semantic
   operation until one required legalization pass either rewrites or rejects
   every unsupported instruction.
2. `StorageField` classifies some dynamic fields as a word, then
   `lower-aggregates` applies raw `sload`/`sstore`. Storage bytes/string and
   dynamic arrays need distinct storage-field shapes or must be rejected before
   semantic aggregate lowering.
3. Fixed-bytes canonicalization is split across literal typing, packed encoding,
   memory bytes operations, and ABI encoding. Value-magnitude guesses must be
   replaced by one canonical producer representation.

## Pass candidates

### Required MIR lowering

- Add semantic external-call results and a `lower-abi-returndata` pass. It should
  check returndata size, validate words, rebuild aggregates, and produce memory
  objects. Ordinary calls, try calls, and linked libraries should only choose
  failure policy and consume the typed result.
- Add first-class multi-result instructions. Remove the shared scratch pointer
  and ephemeral return buffer once tuple destructuring and calls can consume
  multiple SSA results directly.
- Extend `lower-slices` across function returns and call results. This deletes
  all remaining slice-return call-body inlining.
- Add an indirect-call operation and required lowering pass for internal
  function pointers. Run constant-target specialization first, then replace the
  remaining indirect calls with signature-specific dispatch.
- Add semantic contract creation with typed constructor arguments. A required
  pass should ABI-encode the tail, concatenate it with a constant creation-code
  object, emit `CREATE`/`CREATE2`, and forward failure.
- Represent applied modifiers and placeholder continuations explicitly. Compose
  them in declaration order before ordinary body lowering rather than teaching
  statement lowering to splice modifier HIR into the function.
- Complete target legalization after representation lowering. `lower-mcopy`
  now owns `MCOPY`; shifts, `STATICCALL`, returndata opcodes, `PUSH0`, and new
  opcodes should likewise be selected in one place from the configured EVM
  version.
- Make event data and indexed-event hashing semantic MIR operations. Reuse
  `AbiLayout` for ordinary data and add the indexed aggregate rules explicitly.
- Centralize the backend-readiness check. `lower-evm-shaped`, MIR validation, and
  the backend currently maintain different unsupported-type lists. One
  exhaustive predicate should gate the phase transition.

### Optimization

- Represent checked exponentiation as a typed operation, then choose the
  constant-base and general algorithms after SCCP and range analysis.
- Represent `abi.encodePacked` as a typed packed-write plan. Allocation elision,
  adjacent static-run coalescing, and `keccak256` fusion should run on MIR rather
  than inspect adjacent HIR statements.
- Keep dynamic mapping keys semantic through optimization, then lower memory and
  calldata sources in one pass after CSE.
- Generalize cold helper outlining. Constant `Error(string)` payloads, dynamic
  bytes returns, and storage-bytes materialization are currently synthesized
  during HIR lowering and marked `no_inline`.
- Add constant memory/object evaluation for hashes and literal payloads.
  Literal spelling should not decide whether a hash folds.
- Represent storage-bytes push/pop and whole-value copies as semantic mutations,
  then lower their packed short/long layout in one pass.

### EVM IR

- Emit structured `JumpI` terminators from MIR-to-EVM lowering. Raw
  push/`JUMPI` sequences force CFG passes to rediscover edges and make revert
  sharing special-case instruction streams.
- Build one immutable stack-scheduling plan before emission. Global argument
  layouts, stack phis, internal-call argument retention, block traversal, and
  spill placement currently run as separate heuristic scans over the same
  function and constrain one another indirectly.
- Separate scheduler traversal from physical block layout. Cold analysis,
  stack scheduling order, and final EVM block placement currently overlap.
- Let EVM IR own branch inversion, fallthrough selection, revert-tail sharing,
  and final layout exactly once.
- Represent the deployment prefix, appended runtime object, and constructor
  argument suffix in one deferred assembly object. Deployment currently
  reassembles the prefix in an outer loop capped at eight iterations to
  stabilize two offsets; the assembler should own the complete least fixed
  point and have no magic convergence bound.

## Large and repetitive code

| Area | Problem | Split |
| --- | --- | --- |
| `lower::Lowerer::lower_function` | ABI entry handling, local layout, constructor work, body lowering, and return lowering in one function | Entry convention, local initialization, body, epilogue |
| `lower::expr::lower_value_expr_unchecked` | Source expression dispatch mixed with storage, memory, and ABI representation | Syntax dispatch calling representation-specific helpers |
| `lower::call::lower_member_call_with_opts` | Builtins, arrays, libraries, low-level calls, and high-level calls share one dispatcher | Resolve call kind, then use one builder per semantic kind |
| Builtin value-call dispatch | Solidity and Yul builtin matches repeat arity extraction, operand lowering, result typing, and nearly identical opcode recipes | Declarative builtin schemas plus small semantic exceptions |
| Contract creation lowering | Bytecode staging, call-option evaluation, scalar-only argument encoding, opcode selection, and failure policy are coupled | Semantic creation instruction plus typed argument legalization |
| Linked-library call lowering | Reimplements a restricted ABI head/tail encoder and return-area policy beside the semantic ABI encoder | Add storage-slot ABI values, then share external-call encoding and typed return decoding |
| Try/catch lowering | Call construction, return decoding, selector dispatch, clause binding, and rethrow policy remain in one path | Semantic call results plus small success/failure policy blocks |
| Internal-function-pointer dispatcher synthesis | Address-taken discovery, fixed-point function lowering, signature grouping, switch construction, and call lowering are coupled | Keep indirect calls semantic through target specialization, then lower them in one required pass |
| Deployment artifact/prefix generation | Runtime generation, immutable patch planning, constructor emission, object concatenation, and offset convergence are interleaved | Build a deployment object graph and assemble its deferred layout once |
| `backend::evm::codegen::generate_function_body` | Liveness, phi elimination, stack-phi planning, block layout, spill lifetime, and terminator emission are interleaved | Prepare a scheduled function plan before physical block emission |
| Backend stack-layout analyses | `GlobalStackPlan`, stack-phi planning, stack-argument masks, and per-call retention each rescan CFG/liveness state with separate profitability gates | One scheduler-owned plan with explicit costs and invariants |
| `backend::evm::codegen::generate_inst` | Opcode selection, stack effects, memory operations, and call conventions are interleaved | Instruction families plus shared operand scheduling |
| `backend::evm::codegen::emit_value_fresh` | Repeats a large subset of opcode selection from `generate_inst` for rematerialization | Give rematerializable MIR instructions one shared target recipe |
| Backend binary/unary operand emitters | Repeat top-of-stack, liveness, rematerialization, spill, swap, and result bookkeeping cases around a generic operand planner | Make the planner authoritative and leave opcode emission mechanical |
| Dynamic and static internal-call emitters | Duplicate argument retention, frame stores, return setup, and result recovery around two frame-address policies | One call plan parameterized by dynamic or deferred-static addressing |
| `backend::evm::codegen::generate_terminator` | MIR termination semantics, fallthrough choice, stack preservation, and raw EVM jump layout are coupled | Emit structured EVM IR terminators, then choose layout in EVM IR |
| `backend::evm::codegen::resolve_static_frames` | Call-graph depth, frame overlay, static allocation acceptance, spill ranking, and heap-floor resolution are one fixed-point/layout routine | Compute an immutable frame-layout plan, then resolve deferred addresses |
| MIR inliner | Recomputes call counts, reachability, and recursion and mirrors every instruction in a large clone match | Reuse call-graph analysis and operand visitors |
| MIR validator | Phase checks and per-instruction validation are combined | Shared phase/readiness predicates plus local instruction checks |

The remaining size is not itself a reason to move code. Each split should remove
an ownership overlap, a duplicated semantic rule, or a source-shape dependency.
