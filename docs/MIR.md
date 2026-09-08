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
cleanup steps without introducing another stable phase. The gas pipeline runs
CSE, storage PRE, and range-check elimination after aggregate expansion.
First eliminate repeated dominated loads, then replace join loads with phis and
fold checks that forwarding exposes. Storage PRE leaves memory reads alone to
avoid extending pointer lifetimes. After allocation expansion, gas mode forwards
free-memory-pointer loads within each block, discarding the cached word at other
side effects. ABI expansion can create object operations
and aggregate results; flatten structs before erasing object types, and keep
allocation identity until placement has finished. Any newly introduced helper must pass through the remaining required
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
retains its encoding check when the word is unused. Hashed storage-data clearing
retains its slot and half-open word range as one storage-writing operation;
builtin conversion emits the loop before physical address hashing. Stores from
memory bytes objects retain header validation, tail clearing, and copying as
one storage-writing operation that reads the source object. Literal assignments
to state variables, fields, and indexed entries retain their bytes as an owned payload,
including hex literals, and need no source-memory reads. Both
forms share the clear helper created during builtin conversion; literal
expansion writes known headers and padded words without allocating memory.
Repeated short literals share their validation, cleanup and final write in a
helper. Each caller reads the old header so prior storage writes can still
forward into that read.
Dynamic storage-array loads retain scalar element widths, signed/fixed-bytes
encoding, enum bounds, and bytes-element identity. Conversion allocates and
copies the array, sharing the bytes loader across bytes elements and direct
loads. Nested array and struct layouts still use their existing lowering paths.
Storage promotion uses shared read/write effects to reject opaque accesses
it cannot redirect through promoted values.
`lower-checks` expands typed panic and revert checks into branches and shared
payloads, preserving source origins and the selected debug revert strings.
Require keeps evaluated payload arguments in MIR and encodes them only on failure.
Payload reads retain prior memory stores, including stores to nested child objects.
Range-based check elimination learns facts from these operations before expansion;
constant checks fold only when they pass. Revert outlining runs after check and
builtin expansion, before checked arithmetic expands.
Payable `send` and `transfer` retain their address and amount until builtin
conversion emits the stipend calculation and call. A failed transfer reverts
with returndata; a send returns success. Their external-call effects invalidate
account observations and storage reads even while the call remains opaque.
Low-level address calls retain a bytes input and evaluated gas/value options.
Conversion exposes the input buffer and computes any pre-EIP-150 gas reserve.
A separate returndata capture operation allocates a fresh bytes object at its
original position, before another call can replace the returndata. Static calls
read storage; call and delegatecall may also write it.
Jump threading collapses a phi-only branch when a single unconditional predecessor
remains and the phi has no outside uses, exposing nested short-circuit checks
without another pipeline iteration.
After representation lowering, `branch-simplify` folds repeated SSA conditions
established by a sole incoming edge. It retains intervening instructions and
updates phi inputs through the CFG edit API. This avoids keeping loop bounds
checks and their spill homes after an earlier branch has proved the bound.
Block merging and terminal sharing remain with the backend at this stage.
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

Every pass uses one `MirPass::run_pass` entry point returning `Result<bool>`.
Errors stop the pass manager and custom pipelines; the boolean reports whether
the pass changed the module. Wrappers preserve both outcomes.
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
accesses must conservatively alias unless a disjointness proof exists. Allocation
provenance uses shared CFG cycle facts to distinguish joins from loops. Data
pointers retain a proven lower bound from their object, so writes to allocation
contents do not appear to reset the free-memory pointer. Raw pointer conversions
still need a proof that they avoid reserved memory. An access beyond a proven
fresh allocation can overlap other heap objects while retaining its heap region;
loop allocations lack this guarantee after an explicit pointer reset. Do not
attach heap-allocated effect records to every instruction. Unknown calls remain
conservative; known intrinsics expose their summaries without expanding their
implementation.
DCE, CSE, and LICM use shared derived deletion, commoning, and speculation
properties. Internal-call summaries include failure, divergence, and external
termination. Compute them only for called functions, including tail-call targets;
uncalled bodies need no interprocedural summary. DCE removes unused calls only
when these summaries prove normal termination and no observable effects. Recursive calls and possible CFG cycles
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
CSE, DCE, and allocation placement share module call summaries. Changes drop
them unless the pass proves they remain conservative; CSE preserves them when
removing equivalent reads and computations. Preserving function-local CFG or
alias facts alone never preserves call summaries. Consecutive passes in custom
pipelines use the same cache and invalidation rules as the canonical pipeline.
The verifier checks IR at pass boundaries in debug builds and with
`-Zvalidate-ir`. It should report broken invariants rather than repair them.

Terminal-block and function equivalence compare the full instruction with
normalized SSA operands. Literal contents, element widths, enum bounds, and
other semantic fields participate in equality; instruction names alone do not
establish equivalence.

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

Static allocation keeps shared frames fixed so one entry's local objects cannot
raise another entry's heap floor. Locals go before spills when their PUSH widths
stay unchanged, after spills when they fit below shared frames, or after the
entry's reachable frames. A reserved heap prefix stays between these locals and
the initial free-memory pointer. An allocation marked `preserves_fmp` must keep
its FMP address and bump, even when folding makes its size constant. ABI encoding
uses this requirement when it writes the output before reserving its final size.
The flag round-trips through MIR text.

CSE and load PRE avoid extending a load from an allocation base across blocks
solely to eliminate a cheap reload. They can reuse a value already live across
the edge, and load PRE prefers an equivalent constant or already-live value.
The EVM revert pass removes an existing branch inversion around a cold payload
when success can fall through after layout; it preserves the payload's target.
After code sharing, the EVM peephole keeps a stored word on the stack for an
immediate reload of the same storage or transient slot. This avoids extending
MIR live ranges or changing earlier outlining choices.

Private branch successors can retain their live stack before the backend imposes
an argument-only layout. A condition that remains live keeps the global plan,
which avoids disturbing loop-entry layouts. Cold terminal siblings may keep
unused stack words, but their payloads and required live-ins stay explicit.

Final CFG cleanup exposes acyclic branch triangles as structural conditional
terminators. Layout places the taken arm before its join so assembly can omit
the arm’s jump. Known loops and cold arms keep their existing order, and the
conversion preserves source origins and excludes function activation events.

EVM layout packs small shared terminal traces below the PUSH1 address limit.
It moves the whole fallthrough trace, so moving a shared exit does not insert
jumps between its predecessor blocks. Multi-block traces must end at an exit
with at least four references beyond the low-address range; the stricter limit
avoids moving hot code for weak size gains. Packing reserves space for one-byte
indexed jump tables, since wider entries add shifts and masking to each lookup.

Packed ABI encoding skips allocation rounding when every component occupies
whole words, while retaining overflow checks. Storage-byte pushes place the
short-header path first so packed writes can fall through during growth.

Semantic checks fold a negated condition into their failure polarity. This lets
ordinary dead-code cleanup remove the predicate before conversion and exposes
phi-only short-circuit joins to jump threading. Builtin expansion also runs
copy elision for buffers whose writes become visible at that boundary; their
allocation and failure behavior remain intact.

Revert outlining runs once per optimization mode. Gas mode shares source and
builtin payloads before arithmetic expansion, preserving local overflow edges
for stack scheduling. Size mode includes arithmetic payloads in the shared
helpers to avoid duplicated stack and exit code.

The scheduler emits each private predecessor chain before its continuation,
including blocks appended during conversion. Gas mode keeps the surrounding
order because shared call tails in loops depend on fallthrough placement.
Size mode uses reverse postorder for the remaining chains. Layout and spill
availability share one CFG snapshot.

Stack layout planning covers ordinary joins as well as phi edges. Removing a
function's last phi must not disable carrying live values through its other
branches. Typed catch clauses test their selector and payload only when reached,
so an earlier matching clause does not compute later catch conditions.

A bare catch needs no copied return-data object. It leaves the EVM return-data
buffer available to inline assembly. Gas-mode stack planning keeps constants
used on only one branch off its sibling edge when the sibling can retain an
identity layout; the edge that needs them emits their pushes. Size mode keeps
the shared layout to preserve opportunities for merging tails.

Final EVM peepholes move a word store immediately followed by a return of that
word to scratch memory. This reduces pushes and memory expansion after tail
sharing has settled. A different return range or an intervening instruction
keeps the original address, including an `MSIZE` that observes the store.

Final store cleanup consumes a stack word directly when a duplicate is stored
and its original is discarded immediately afterward. It preserves the order of
the remaining stack and does not cross a function event or glued boundary.

Two-word branch layouts place one reloaded join value above the resident word.
Preparing the condition then needs one swap. Wider layouts retain their existing
order because downstream joins can outweigh that local saving.

Gas cleanup can copy an eight-byte word-return body into a stub shared by
multiple empty stubs. This removes an extra jump while retaining distinct
return labels. Size mode keeps the shared body, and function-entry blocks or
activation events on the replaced jump prevent the copy.

When both branch arms terminate normally, gas layout places the false arm first.
The true arm then uses the existing condition directly, avoiding an inversion
and exposing its return path to later sharing and placement.

Tail merging reuses an existing whole-body terminal suffix when there are no nested shared tails or function-entry events. Other return labels remain distinct jump stubs, so sharing avoids an extra block without changing address identity or nested fallthrough paths.

The lowered pipeline folds constant results before branch cleanup and stack scheduling. It keeps other value identities and instruction choices intact to avoid lengthening live ranges after representation lowering.

The scheduler carries known loop membership into EVM IR, and block merging
preserves it. Gas-mode outlining keeps loop computations and large pushes
inline. Tail merging can reuse an existing non-loop tail from a loop, but loop
blocks do not seed new sharing groups.
