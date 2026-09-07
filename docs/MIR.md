# MIR architecture

MIR keeps typed values and function calls through optimization. The backend
receives scalar values after progressive lowering, then schedules them onto the
EVM stack. These boundaries serve different purposes: maintaining SSA is a
correctness requirement for every transform; choosing a physical representation
is a late lowering decision.

## Phase model

The two representation phases and checked backend boundary are implemented.
Shared instruction effects, checked arithmetic, conditional checks, revert payloads,
precompiles, and concatenation
now use the phase boundary. The remaining builtin families are described below.
The CFG and aggregate contracts below also describe implemented behavior.

Use two stable MIR representations, `semantic` and `lowered`, followed by the
existing EVM IR. Keep one set of MIR data structures. Optimization history is
pipeline state, not an IR phase: running SCCP does not change which instructions
or types a module may contain.

| Representation | Contract | Main work |
| --- | --- | --- |
| Semantic MIR | Typed SSA, structs, slices, object references, semantic builtins, ordinary function calls; ABI and storage layouts remain explicit data. | Inline and specialize small functions, propagate constants, promote frame slots, simplify aggregates, remove redundant checks and memory/storage work. |
| Lowered MIR | Word-valued SSA, explicit routing and ABI code, physical memory accesses, lowered call signatures, backend-supported operations. No semantic builtin or unresolved layout remains. | Simplify exposed scalar code, remove redundant loads/stores, optimize generated loops where profitable, prepare scheduling. |
| EVM IR | Scheduled blocks with physical stack operations and explicit control transfers. | Target peepholes, sharing, outlining, layout, then assembly. |

`lowered` does not mean scheduled: SSA values, phis, functions, and calls survive
until the scheduler. Label addresses, immutable references, and proven static
allocation addresses may remain as a small, explicitly verified set of backend
placeholders. A remaining allocation must not hide an unlowered runtime check,
initialization, or free-memory-pointer update.

The pipeline should have named groups with visible entry and exit contracts:

```text
HIR -> semantic MIR
    -> semantic optimization
    -> expand builtins, ABI, and dispatch
    -> bounded cleanup of the exposed code
    -> lower aggregates, frame slots, storage addresses, and memory layouts
    -> place/coalesce allocations, then expand allocation and copy operations
    -> verify and enter lowered MIR
    -> scalar and memory optimization
    -> scheduler -> EVM IR optimization -> assembly
```

These are responsibilities and dependency constraints, not a benchmarked new
pass ordering. The conversion may contain several named passes and local
cleanup steps without introducing another stable phase. In particular, ABI
expansion can create object operations and aggregate results; flatten structs
before erasing object types, and keep allocation identity until placement has
finished. Any newly introduced helper must pass through the remaining required
lowerings too. Expansion must not leave a high-level operation behind merely
because it was created after that operation's lowering pass ran.

During conversion, mixed operations remain subject to the general SSA and type
verifier. The phase stays `semantic` until full conversion succeeds; this phase
permits primitive operations as well, including those from inline assembly.
Passes inside the conversion use explicit local preconditions. Add a third
stable MIR phase only if an independent consumer needs a verified intermediate
representation. A pass name or useful dump point alone does not justify one.

### Preserve builtin semantics through frontend lowering

HIR lowering should evaluate operands, resolve types and layouts, and emit a
semantic operation for each runtime builtin. Builtins can stay opaque to
passes that do not understand their internals while still exposing signatures,
effects, and constant-folding rules. Use typed intrinsic identities or existing
`InstKind` variants; do not encode them as unknown `ICall` targets or strings.
Keep source functions and compiler intrinsics distinct, and retain callee-derived
return signatures for ordinary calls.

ABI encoding/decoding, aggregate copies, memory-object accesses, abstract
allocations, checked arithmetic, packed encoding, concatenation, and precompiles follow this
approach. `lower-arithmetic` expands checked word operations and exponentiation
loops. `lower-builtins` expands precompile buffers/calls, concatenation copies, and packed
encoding. Packed arguments retain scalar widths and owned array layouts; length
reads and packing loops run after argument evaluation. Both bytes results and
scratch hashes share the encoder. Solidity `addmod` and `mulmod` retain their
zero-modulus panic until conversion; their Yul counterparts keep native zero
semantics. Runtime `erc7201` retains the namespace object, while a literal
namespace folds directly to its constant slot. Storage bytes loads retain
header validation, allocation and copying as one operation. Header reads keep
`sload` visible to storage and loop passes; a separate validation operation
retains its encoding check when the word is unused.
Storage promotion uses shared read/write effects to reject opaque accesses
it cannot redirect through promoted values.
`lower-checks` expands typed panic and revert checks into branches and shared
payloads, preserving source origins and the selected debug revert strings.
Require keeps evaluated payload arguments in MIR and encodes them only on failure.
Payload reads retain prior memory stores, including stores to nested child objects.
Range-based check elimination learns facts from these operations before expansion;
constant checks fold only when they pass. Revert outlining runs after check and
builtin expansion, before checked arithmetic expands.
Jump threading collapses a phi-only branch when a single unconditional predecessor
remains and the phi has no outside uses, exposing nested short-circuit checks
without another pipeline iteration.
The final scalar/check cleanup group runs after these conversions; the semantic
operation still counts as control flow when inlining estimates its expansion.
Array push/pop and Solidity-level call preparation remain to migrate.
Yul word operations already express their complete semantics and need no extra
opaque wrapper. Type-only builtins can disappear, and genuine constant results
can fold without constructing a runtime implementation.

For example, keep the following operations visible together until a MIR combine
can choose whether to allocate or use scratch memory:

```text
bytes = abi_encode(layout, values)
hash = keccak_bytes(bytes)
```

The frontend currently recognizes some ABI/hash combinations from HIR syntax.
Moving that combine to MIR lets it see through calls and value substitutions.
Eliminating the allocation still requires proof that its identity, contents,
free-memory-pointer movement, and memory expansion are not observed elsewhere.

Preserve evaluation order and failure semantics explicitly. `require(condition,
message())` evaluates `message()` even on success; only failure-path encoding
may be deferred. Checked arithmetic and ABI validation can revert even when
their result is unused. Keep those checks as semantic effects until proved
redundant. Low-level calls and precompiles retain their gas, returndata, and
failure behavior. Solidity's checked division and Yul's division by zero must
not share a folding rule that changes their distinct semantics.

Aim for one typed body per source function, with external entries represented
by interface declarations. ABI lowering owns decoding, validation, encoding,
and dispatch wrappers. It may fuse or specialize a wrapper/body pair when that
saves runtime work; the representation must not require an extra runtime call.
Move existing frontend exceptions for external arguments into explicit decode
or validation operations, preserving validation required even for unused
arguments. Constructor, fallback, receive, and internal entry behavior need
separate regression coverage during that migration.

A phase verifier cannot infer whether raw arithmetic came from Yul or an eagerly
expanded Solidity builtin. Enforce the frontend policy through its construction
APIs and HIR-to-MIR fixtures as well as the phase checks.

### Make phase transitions checked boundaries

Required conversions use `MirPass::try_run_pass` to report failure separately
from their changed flag. The pass manager and custom pipelines stop on errors.
`abi_wrapper` marks an explicit entry ABI; it is verified independently of the
module phase. The final phase uses a shared legality check, and runtime codegen
requires an immutable `LoweredModule` view.

Define one legality implementation for instructions, types, function signatures,
terminators, and module entries. Use it both when completing conversion and when
accepting lowered input for codegen. Match instruction variants exhaustively so
adding an operation requires a legality decision. Check semantic attributes and
backend placeholders as well as opcodes; inspect all retained definitions, not
only the ones that happen to have consumers.

Make the phase field private to parsing and checked transitions. Advance it
only after verifying the destination contract, with monotonicity enforced in
all builds. Parsing an `@phase` header declares a contract to verify; it does
not prove that contract. Required conversion returns a diagnostic result,
separate from its changed flag, and stops the pipeline on failure. Preflight
unsupported cases before editing where practical. Otherwise discard the failed
compilation's module; do not publish a partially lowered module as successful
or clone every module just to provide rollback.

Run a cheap representation check at each stable boundary in all builds. Keep
full SSA, dominance, and type verification after each changed pass in debug
builds and with `-Zvalidate-ir`; validate untrusted textual MIR fully at ingress.
The backend should receive a verified immutable view after the last MIR pass,
so a phase label cannot bypass checking and later mutation cannot silently
invalidate the checked view. Keep full validation costs out of every release
pass invocation.

Optimization passes preserve the current representation. Required lowering
passes declare their input requirements and fail clearly when a custom pipeline
violates them. Optional optimization being disabled must never suppress a
required conversion. Individual lowering passes remain available for UI tests
and dumps; only the complete checked conversion promises `lowered` output.

### Separate effects from permission to transform

`EffectKind` is a coarse single category. The codebase also has precise ModRef
queries, bounded interprocedural footprints, allocation/capture facts, and
pass-specific rules for memory expansion and execution guarantees. Retain that
precision and expose a shared semantic interface; a larger single effect enum
would still fail to describe operations that both read and write several resources.

| Query | Required facts |
| --- | --- |
| What can this operation access? | Read/write footprints for memory, persistent/transient storage, immutables, and mutable environment or returndata state; widen unknown accesses conservatively. |
| Can it disappear if unused? | Observable writes, failure/termination, allocation observations, and other required behavior. An unused result is insufficient. |
| Can an earlier result replace it? | Equal operands, stable read dependencies, and compatible identity and observable behavior. |
| Can it execute earlier or on another path? | Dependency ordering, guaranteed execution or a proof of safe speculation, failure behavior, and gas profitability. |
| Can it be duplicated or rematerialized? | The preceding facts plus allocation identity and repeated execution cost. |

Use small derived properties for context-free behavior, backed by the existing
alias and call-summary analyses for footprints. Before layout lowering, use
object identity and field/element accesses where proven; raw pointer or assembly
accesses must conservatively alias unless a disjointness proof exists. Do not
attach heap-allocated effect records to every instruction. Unknown calls remain
conservative; known intrinsics expose their summaries without expanding their
implementation.
DCE, CSE, and LICM use shared derived deletion, commoning, and speculation
properties. Internal-call summaries include failure, divergence, and external
termination. DCE removes unused calls only when these summaries prove normal
termination and no observable effects. Recursive calls and possible CFG cycles
remain conservative. DCE and ADCE preserve memory expansion when `msize` in the
function or a callee can observe it.

Track changing observations such as `gasleft`, returndata, balances, `msize`,
and the free-memory pointer separately from stable inputs such as calldata.
An EVM memory read can expand memory. A mathematical operation can be too costly
to hoist onto a path that never executed it. Keep semantic safety and gas/size
profitability as separate decisions, including the existing guards against
speculating expensive loads out of zero-trip loops. Do not import LLVM's
undefined-behavior assumptions into Solidity checks or raw EVM operations.

Correctness properties belong in operation semantics or verified analysis
results. They must not depend on retaining optional source/debug metadata.
`validate_abi` keeps a source ABI validation obligation explicit in the operand
graph, including unused calldata struct fields. ABI lowering discharges it only
when entry decoding or a typed internal body supplies the validation contract.
This obligation round-trips through MIR text and cannot cross the lowered boundary. Keep genuine layout proofs and allocation
semantics explicit; do not discard them as mere optimization hints.

### LLVM and MLIR lessons

LLVM recommends intrinsics for call-like extensions and gives them memory
properties and folding rules. This supports keeping a builtin compact without
making every pass understand its expansion. See
[Extending LLVM](https://llvm.org/docs/ExtendingLLVM.html).

MLIR's full dialect conversion succeeds only when every operation satisfies the
conversion target, including dynamic legality constraints. Use that rule for
our stable boundary without adopting a dialect registry, generic rewrite
engine, or rollback framework. Its effects model also separates resource
accesses from speculation; byte ranges and alias proofs remain separate
analysis concerns. See [dialect conversion](https://mlir.llvm.org/docs/DialectConversion/)
and [effects and speculation](https://mlir.llvm.org/docs/Rationale/SideEffectsAndSpeculation/).

LLVM's SelectionDAG pipeline combines operations before legalization and again
after type and operation legalization. Follow that placement principle: lowering
exposes new optimization work. It does not imply rerunning the entire expensive
MIR pipeline after every expansion. Reuse analysis caches across pipeline
groups when their dependencies survive, and invalidate them when expansion
changes calls, memory effects, or control flow. See the
[code generator](https://llvm.org/docs/CodeGenerator.html#selectiondag-instruction-selection-process).

### Migration and acceptance

Implement the changes in independently reviewable steps:

1. Share legality checks and separate conversion failure from pass changes.
   Group the existing pipeline without reordering it; keep bytecode identical.
2. Consolidate effect queries and migrate DCE, CSE, and LICM consumers before
   introducing additional opaque operations. Preserve existing conservative rules.
3. Move builtin families from frontend expansion to semantic MIR one at a time,
   with folding, effects, expansion, and runtime tests in the same change.
4. Consolidate external interface/ABI ownership, then replace the historical
   phase labels with the two representation contracts and migrate MIR fixtures.
5. Tune the optimization groups on both forms, including checks and loops newly
   exposed by expansion. Change one pass group at a time.

Keep the existing indexed IR, aggregate representation, local CFG mutation
helpers, and scheduler/EVM IR/assembler split. A generic dialect system,
`Module<Phase>` types throughout all passes, effect-token SSA, full MemorySSA,
a general SSA repair service, and a pass dependency solver are not prerequisites.
Revisit them only when a concrete transform or profile demonstrates the need.

The expected gains are smaller input to expensive early passes, semantic
combines that survive inlining, fewer frontend/backend special cases, and
codegen rejection at the boundary that failed. Runtime gains need measurements:
hiding operations without teaching analyses their effects can make code worse.
Preserve the current bounded call footprints and late aggregate flattening.
Unused return-field elimination can then build on those contracts rather than
return-buffer conventions.

For each migration compare the same successful UI and runtime corpus IDs,
serialized bytecode, hot-call gas, and gas/size-mode sizes. Record instructions,
blocks, generated helpers, and per-pass time before and after expansion; measure
compiler time and peak memory on the same inputs. Retain checks for malformed
ABI inputs, revert payloads, side-effecting arguments, reentrancy, recursive
calls, memory observations, and debug-bytecode neutrality. Structural migrations
should preserve output; optimization changes must explain per-case regressions
and earn their place on runtime gas and output size before compile time.

## CFG edits own phi maintenance

A control-flow edge appears in three places: the source terminator, the target's
stored predecessor list, and each phi's incoming list. Phis use one value per
predecessor block, even when a switch has several cases targeting that block.
Changing only a terminator leaves those other representations stale.

Each transform maintains these structures at its edit sites. There is no
whole-function phi repair pass. A repair scan could remove stale inputs but
could not supply values for newly added edges or fix dominance violations.
The parser's deferred inference of aggregate call and phi types is separate:
it resolves forward references once, without changing control-flow edges.

Use operations with narrow, explicit contracts:

- `replace_terminator` updates only affected successors. It requires phi inputs
  for a new edge before changing the terminator, retains inputs on kept edges,
  and removes them from dropped edges. It preserves debug metadata.
- `fold_terminator_to_jump` keeps an existing successor, removes the other
  successors' phi inputs, and collapses duplicate kept edges without losing
  their value. It preserves the terminator's debug context.
- `invalidate_unreachable_block` clears a block after the caller proves it
  cannot execute. It removes outgoing edges and their phi inputs. Incoming
  backlinks stay until their source terminators change, so dead cycles can be
  processed in either order.
- `split_edge` inserts one block on a logical edge and rekeys the target's phis.
  All duplicate occurrences of that edge go through the same new block.
- A redirect that introduces a predecessor must explicitly supply or construct
  each new phi value. Neither a generic setter nor a repair scan can infer it.

SCCP, check elimination, DCE, ADCE, CFG simplification, and jump threading use
these operations. Loop preheader creation prepares its phis before redirecting
edges. PRE uses `split_edge`; ABI continuation splitting transfers successor
links and phi inputs with the terminator. Replacing a body with a helper call
also removes the original body's outgoing edges. Frame promotion preserves
control flow, and pure evaluation reconstructs the whole body directly.

The stored CFG and cached analyses are distinct. Updating phis and predecessor
lists does not update dominance, reachability, alias analysis, or loop facts.
The pass machinery invalidates analyses after relevant changes; a pass that
continues querying them after an edit must recompute or update them first.
The verifier checks IR at pass boundaries in debug builds and with
`-Zvalidate-ir`. It should report broken invariants rather than repair them.

### LLVM comparison

LLVM's predecessor iterator follows basic-block uses in terminator operands,
so it does not need our separate predecessor-vector rebuild. Phi incoming
operands still need maintenance: `BasicBlock::removePredecessor` updates phis,
and utilities such as `SplitEdge` handle particular CFG rewrites. Changing a
successor operand alone is not a complete SSA update. `SSAUpdater` helps when
moving or duplicating definitions requires new merges; it is not an automatic
cleanup after every pass. See LLVM's [CFG iterator](https://llvm.org/doxygen/IR_2CFG_8h_source.html),
[BasicBlock implementation](https://llvm.org/doxygen/IR_2BasicBlock_8cpp_source.html),
[CFG utilities](https://llvm.org/doxygen/BasicBlockUtils_8h_source.html), and
[SSAUpdater](https://llvm.org/doxygen/SSAUpdater_8cpp_source.html).

For this index-based IR, local mutation helpers avoid adding an intrusive
use-list system solely to maintain CFG backlinks. More precise analysis
preservation and a general SSA updater can be added when measurements and
transforms justify their cost.

## Aggregate values are not memory objects

`insert_value` and `extract_value` construct and project fixed SSA structs.
They do not allocate storage, copy bytes, or imply an address. A slice field
carries its pointer and length; a memory-object field carries a typed reference,
not a copy of the referenced object.

A raw `u256` field can carry all bits of a nominal object reference. Keep that
loss of type information explicit: `word_cast` preserves the bits and yields a
raw word; `memory_object_from_ptr` gives a word an object type without proving
validity or ownership. Neither operation allocates or copies memory. Aggregate
lowering inserts `word_cast` when a raw field contains a nominal reference;
memory-object lowering erases both conversions. Alias analysis follows their
unchanged addresses.

The verifier checks nominal object kinds against semantic accesses, while
retaining compatibility with raw pointer carriers during lowering. It also
checks ordinary return counts and void signatures. `ret` returns to a MIR
caller, including for void functions; `stop` ends EVM execution even inside
a helper. ABI lowering converts empty external returns into `stop`. A shared
returnability analysis follows tail-call chains, including cycles, so forwarding a call
cannot hide an incompatible return signature. Proven nonreturning chains stay
exempt. Call-to-tail-call conversion uses the same returnability facts, so it
cannot discard a continuation after a returning tail-call chain. Slices as well
as structs must be gone at the EVM-shaped boundary.

LLVM calls the matching aggregate operations `insertvalue` and `extractvalue`.
Its `insertelement` and `extractelement` operate on vectors, can take dynamic
indices, and have different target-lowering rules. In the SelectionDAG path,
LLVM flattens aggregate values into component DAG values: insertion substitutes
the selected components and extraction selects them. Vector operations instead
use vector-element DAG nodes, which undergo target legalization. See the
[language reference](https://llvm.org/docs/LangRef.html#aggregate-operations) and
[SelectionDAG builder](https://llvm.org/doxygen/SelectionDAGBuilder_8cpp_source.html).

We already have a dedicated `lower-structs` pass. It keeps field order, flattens
nested structs and slice fields, and rewrites aggregate parameters, results,
phis, selects, and calls. It reserves scalar placeholders before rewriting so
loop-carried aggregates do not depend on block traversal order. For example:

```text
s0 = insert_value {u256, u256}, undef, 0, a
s1 = insert_value {u256, u256}, s0, 1, b
x = extract_value {u256, u256}, s1, 0
```

becomes the value substitution `x = a`, with no load or store. An aggregate
`phi [left: {a, b}], [right: {c, d}]` becomes two scalar phis. Stack scheduling
later decides whether their values need stack moves or spills.

The pass runs after ABI and dispatch construction, before frame and memory
lowering. Internal calls remain typed and aggregate-valued until this boundary.
Here, their results adopt the backend convention: the first scalar result plus
reads of the remaining result words. Those reads occur immediately after the
call so a later call cannot overwrite them. Only this calling convention, real
memory objects, and eventual spills require memory traffic; insertion and
extraction alone do not.

## Output quality and remaining work

Keep aggregates through inlining and high-level optimization, then lower once
at the calling-convention boundary. This keeps return values explicit for
analysis without exposing the shared return buffer too early. Keep physical
stack moves in the MIR-to-EVM scheduler and byte offsets in the assembler.
Adding another insert/extract lowering pass would duplicate `lower-structs`.

Instruction simplification forwards projections through insertion chains before
lowering. Its bounded walk handles overwritten fields and nested aggregates
without hanging on malformed unreachable cycles. It preserves nominal type
changes for the explicit conversion at the lowering boundary. Both struct and
memory-object lowering prune unreachable definitions before resolving value
substitutions.

Further output optimizations can remove unused fields and return components
where all callers allow it, or simplify scalar phis exposed by lowering. Those
are separate transforms with their own profitability constraints.
Early flattening can expose scalar optimizations but also enlarge IR and extend
live ranges; materializing aggregates in memory adds aliasing and memory costs.
Neither should be chosen without checking generated code.

Compare runtime gas first, then gas- and size-mode bytecode, with compile time
as a tie-breaker. Check the same successful corpus cases and serialized bytecode,
not only total size. CFG maintenance changes must retain behavior; fewer repair
scans alone are not evidence of better generated code. Debug metadata must stay
bytecode-neutral throughout these rewrites.
