# Proof boundaries

An operation having semantics does not mean every occurrence of that operation
can be verified. A positive result covers the admitted inputs, observations and
assumptions in its audit record. It does not prove the Rust compiler, complete
pass or complete contract correct.

## Import correspondence

[Correspondence.lean](SolarTV/Correspondence.lean) proves that successful CFG
expansion preserves execution for every initial state. Instruction effects may
change state or terminate execution; missing nodes and exhausted expansion fuel
are unsupported, not successful outcomes. The theorem is generic in the state
and observation types. Its application determines which effects it covers.

For acyclic EVM regions, the Rust helper reparses the compiler-validated source
and exports reachable instruction identities and control edges. It does not
consume the Python importer's execution tree. A generated Lean certificate
checks that graph's expansion against the separate tree and applies
`expansion_correct` and `EVM.Region.toTree_correct`. Changing both sides of a
pair to the same incorrect tree therefore cannot turn a corrupt import into a
proof. The projected stack, memory, transfer stack growth and terminal effects
must agree with the source graph.
An entry theorem then links the actual function used by the pair proof to that
graph, with the requested entry label, incoming stack height and input types.
Checking an unused code declaration is insufficient; corruption tests also
replace the execution function while leaving its checked code tree intact.

Pure cyclic EVM regions have a narrower route: kernel-checked equality of the
independent source graphs implies equal executions for every bound and initial
state. This proves no termination assumption and never equates successful
execution with bounded exhaustion. It currently handles graph-preserving cleanup,
including removal of unreachable blocks; loop rewriting that needs a simulation
or invariant is unsupported. Memory/state operations in this route are rejected.

Pure acyclic scalar MIR imports are checked against the direct typed SSA
interpreter in Lean using the independent Rust schema export. Arguments and the
single result retain their integer widths from i1 through i256. Certificates
cover predecessor-sensitive phis, switches, branches, casts and checked
arithmetic, including exact failure payloads. The independent signature must
match the imported signature; retagging an i1 input as i256 is rejected.

The generated obligation quantifies every typed argument and links the actual
imported function to successful interpreter execution. Unsupported execution and
fuel exhaustion cannot satisfy it. Corruption regressions invert a branch,
change phi selection, omit an overflow check and exhaust the source interpreter.
Internal calls, memory effects, ABI entrypoints and symbolic checked powers
remain outside this certificate. Those modes retain their explicit importer
boundary; absent `import_correspondence` evidence never means a certificate.

The real `phi`, `switch`, `typed-branch`, `casts` and `checked` example pairs
now require both import certificates: ten imports previously outside the
straight-line i256 certificate. Their broken variants still require matching
Lean and native counterexamples. This expands certified import coverage; the
separate real-contract sampling denominator below is unchanged.

The compiler parser, Rust source normalizer/schema exporter, opcode mapping and
backend placement remain trusted. Independent replay checks concrete witnesses
against the original interpreters. It is additional evidence, not an all-input
proof of those implementations. The permitted proof axioms remain `propext`,
`Classical.choice` and `Quot.sound`; generated positive proofs reject others.

Deletion-only MIR function tables have a separate structural certificate in
`MIR/Transforms/FunctionDCE.lean`. Each surviving declaration is identical,
including its interface, attributes and instructions. The kernel checks that
every surviving named reference is closed over the retained table and every
external root remains. An induction proves equality of execution for every step
bound, with arbitrary primitive steps, state, saved caller continuations and
observations. Calls, tail calls, returns, cycles and recursion are included;
there is no termination assumption or fixed unrolling depth.

This is a named-call-machine theorem, not an instruction interpreter import
certificate. The compiler parser/display, extraction of named references and
binding to that call machine are trusted. The theorem permits entry only into
the retained closed set: a removed function is never validated as an entry.
Numeric function-pointer dispatchers and backend-memory records are rejected.
Physical gas, code placement and numeric function identities remain outside the
claim. Reports distinguish this module-level structural certificate from local
instruction proofs and retain `whole_pass: false`. Changed surviving bodies,
global declarations or phases, deleted external roots and missing call targets
are unsupported. A changed live result has an independent semantic
counterexample in `examples/mir/function-dce/`.

Deletion-only SSA changes inside cyclic MIR have a contextual simulation in
`MIR/Transforms/DeadSSA.lean`. Independent typed source exports retain every
block, phi group, transfer and ordered effect. A local-ID correspondence must be
injective and preserve every retained operand and definition. The kernel checks
the generated action programs against exact erasure of the original program,
then checks that no retained read refers to an erased definition, including phi
backedges and terminal observations. Only total scalar operations and typed
slice construction or projection can be classified as silent. The
`MIR/Transforms/DeadSlice.lean` lemmas use the direct interpreter to show these
slice operations only update a local. Construction takes defined pointer and
length words, and projection checks the field index and type with its address
space. The checker retains loads, storage reads, checked arithmetic and
allocations. The live-projection regression also checks that a changed storage
write yields the same counterexample in Lean and Rust.

The theorem preserves live registers and arbitrary shared state after every
block. It quantifies all effect and transfer handlers, initial values and
block-transfer bounds. Failures propagate immediately; cycles require neither
fixed unrolling nor a termination hypothesis. Equality at one exhausted budget
does not establish verification. All-budget equality follows from the kernel's
block simulation. Logical fuel counts blocks, so this is a stuttering theorem,
not equality of instruction work or physical gas consumption.

Source/schema export, injective local-ID normalization, total-pure classification
and binding to the action machine remain trusted. This is not direct typed SSA
interpreter correspondence. Opaque supplied callees must be unchanged and may
not re-enter the changed function. Code/GAS observers, external messages,
placement queries, backend records and undefined operands are excluded. SSTORE
remains a potentially failing effect; scalar pricing and its effect on the
EIP-2200 sentry are not certified. A corrupt generated program, changed phi
backedge, live operand, effect or supplied callee is rejected. The broken-store
and checked-underflow regressions have independent runtime counterexamples.

The calldata scan certificate takes the next step beyond deletion-only loops.
It matches the complete exported `stopAt` CFGs against typed templates in
`MIR/Transforms/CalldataInduction.lean`, including the unchanged ABI and revert
blocks, both phi predecessors, the early exit and the scratch-memory return.
Its logical loop projection relates the cursor to `base + (index << 5)` in
256-bit arithmetic. A successful ABI length guard implies a length below
`2^251`, so word offsets stay injective even if the cursor wraps around the
base. Induction preserves the guard, calldata read and recovered index on every
visit. A separate termination theorem bounds visits by the input length plus
one. The typed CFG projection and the parser/schema export remain trusted.
The real pass capture has direct Lean and Rust execution checks for empty,
matching, exhausted and malformed inputs. A doubled stride produces matching
counterexamples from both interpreters.

The stateful `ContinuedCounter.fill` certificate executes both captured
functions via `MIR.State.runStores`. Its kernel theorem projects the traced
execution to the ordinary MIR interpreter. Each run completes within 128
work units and produces the same eight ordered MSTOREs, final memory,
journal and ABI return. This covers arbitrary aligned memory up to 64 KiB,
including expansion, and any aligned heap floor at least 128 with 224
bytes of room in that model. The kernel derives the first pointer loop's
exit displacement of 64 bytes and its recovered index of 2, then executes
the receiving phi and both later loops. The changed handoff recovers 1 and
overwrites the second array element. Independent Rust replay checks that
counterexample. The compiler parser, schema export and constructor renderer
remain trusted. The theorem prices no MIR instructions.

The runtime-bounded `WrappingBounds.fillWords` certificate binds both full
typed CFGs, including checked allocation, the fast and slow loops, and
ABI publication. Its proof derives the pointer sentinel bound from the
allocation and fast-edge guards. The affine invariant preserves the two
loop forms for arbitrary shared effects, including live header reads,
ordered stores and the first failure. A rank proves that a valid logical
execution finishes within the runtime bound of `min(length, count) + 1`
header visits. The compiler parser, typed SSA export and CFG projection
remain trusted. Direct Lean and Rust replay checks a corrupted stride
against the real IR.

`MIR/Transforms/LoopSimulation.lean` also supports user-proved cutpoint
invariants over two typed MIR machines. Positive segments connect each
logical step to primitive transitions. This permits different numbers of
primitive steps while preserving ordered effect prefixes, terminal
observations and divergence. The typed example
`examples/mir/loop-simulation/Proof.lean` verifies a memory-writing loop
with a jump on one backedge. New CFG shapes need a checked invariant and
either a direct segment proof or a reported projection boundary.

Complete unused named-type table erasure has a separate resolved-data theorem
in `MIR/Transforms/TypeDCE.lean`. Each source module is independently parsed and
validated. All functions are exported, including unreachable bodies, with
structural types resolved in parameters, results, operands and operations. The
kernel checks these complete typed graphs and the full retained canonical text
for equality. Functions, numeric identities, order, phase, linked declarations,
attributes and executable lines must remain unchanged. Partial table rewrites
and simultaneous function removal are rejected.

The theorem covers every initial state, work bound and interpreter of the
resolved data. It does not execute unsupported operations, invent GAS results,
assume that loops terminate or accept two matching model errors. The separate
module export is explicitly structural: ordinary source/replay requests keep
their GAS execution restrictions. Compiler parsing, schema export and typed
constructor rendering remain trusted; primitive instruction implementations,
physical gas and backend layout remain outside this structural certificate.
Reports identify `resolved-type-table-erasure`, retain module scope with
`whole_pass: false`, list every retained function and erased type, and hash the
independent resolved graphs. Tests reject corrupted resolved data despite equal
retained text. The real pass pair and a wrong-store kernel/native witness are
in `examples/mir/type-pruning/`.

Allocation coalescing has a separate conditional prefix certificate in
`MIR/Transforms/CoalesceAlloc.lean`. Kernel-checked normalization separates the
ordered data writes from reservation updates. It proves equality of complete
byte-memory functions, active sizes and final cursors for arbitrary initial
contents, then applies an arbitrary identical continuation. Reachable callee
bodies and all ordered call arguments must match; recursion back into the
changed constructor is rejected. Unchanged loops are not unrolled, summarized
as successful calls or treated as matching model errors.

The claim covers only the selected constructor. Changed bodies outside its
transitive call closure are recorded and excluded, not certified. Shared
declarations, function interfaces and numeric table order must agree, and
function-pointer dispatchers remain unsupported. Named-reference extraction
and its binding to the call graph remain trusted.

The producer contract explicitly requires an aligned FMP of at least 128,
matching the word at `0x40`, a nonwrapping total reservation below `2^64`, and
aligned active memory including the FMP slot. This contract is not inferred
from arbitrary caller state. Establishing it at every call site remains an
obligation. The kernel checks source reservation extents and ordered writes;
the compiler parser/schema export, SSA projection, cached-cursor correspondence
and continuation binding remain trusted. Functional byte writes agree with the
shared byte-list operation within active memory. FMP overwrite and disjointness
laws are also kernel-proved. This remains an unpriced prefix certificate, not
direct SSA interpreter correspondence or a physical gas/layout theorem. An
aliasing FMP at 64 is a kernel counterexample when its safety contract is
removed. The reduced real pass and a wrong initializer with independent replay
are in `examples/mir/coalesce-allocs/`.

`MIR/Transforms/LowerAllocPrefix.lean` also proves that a first reservation's
lowered FMP load, constant word addition and FMP store implement the same
byte-memory update under this producer contract. The active size includes slot
`0x40`, and the cursor bound makes word addition agree with natural addition.
The projector recognizes only the uninterrupted initial sequence, including its
pointer cast and exact SSA operands. Later reservations on the lowered side and
all intermediate observations are rejected. The generated pair theorem applies
this bridge before the allocation-prefix theorem; it does not assume the
lowered operations are an abstract allocation without that kernel step. A
wrapping cursor is a kernel counterexample outside the contract. The reduced
real `lower-alloc` pair, wrong FMP-store counterexample with independent replay,
and malformed-sequence tests are in `examples/mir/allocation-lowering/` and
`tests/test_allocation_lowering.py`.

Fresh heap-floor reservations have a different, direct-state certificate in
`MIR/Transforms/FreshHeap.lean`. The named placement binding and its equality to
the current FMP are explicit producer obligations, not facts inferred for
arbitrary caller memory. Initial memory is aligned and includes bytes
`0x40..0x5f`; the aligned floor is at least 128 and the source reservation fits
the direct interpreter's 64 KiB work domain. These conditions imply a defined,
nonwrapping infallible reservation. Its complete byte-memory update is
kernel-proved. An explicit entry FMP write establishes the cursor condition,
and a non-vacuity theorem constructs valid states for every admitted extent.
The checker rejects an extent too large to admit any such state.

The generated pair theorem executes both complete independently exported typed
SSA bodies with arbitrary journal, byte contents, returndata and context under
that condition. It requires `MIR.State.equivalent`, so either model error or
exhaustion prevents a proof. There is no suffix projection, implicit placement
or successful-continuation assumption. The target opcode availability and
internal-entry mode match the direct importer; other context stays arbitrary.
Reports distinguish `producer-valid-fresh-heap-ssa` and retain the source
extent, constructor-renderer boundary and generated-proof fingerprint. Parser,
schema export and constructor rendering remain trusted. This conditional
function theorem does not establish that actual callers or backend placement
satisfy the producer obligation, or price physical gas. Real `lower-alloc`
output and a changed-store kernel/native witness are in
`examples/mir/fresh-heap/`; tests also demonstrate the invalidity of replacing
an arbitrary live FMP with an unrelated floor.

Unknown-height block windows also admit a narrower contextual prefix proof.
An independent Rust export includes every instruction and the terminal. Only a
fixed-price scalar/stack prefix may differ, and the complete remaining suffix
and physical successor must agree. A Lean theorem quantifies over every stack
and arbitrary live world, including all stack-limit cases. Equal-price prefixes
preserve all gas budgets; cheaper prefixes prove resource refinement and require
sufficient gas and a gas-insensitive continuation. Neither calls nor code-image
observers are admitted. The source exporter and opcode mapping remain trusted;
this route does not claim the CFG-expansion import certificate described above.
A failed universal proof is unsupported, since its hypothetical stack may be
unreachable. It never becomes an unreplayed wrong-code claim.

Unknown-height complete terminal regions now have a second route. The checked
CFG expansion and actual entry obligation quantify every incoming stack and
projected memory state, including underflow, overflow and a transfer's temporary
label push. No height, incoming constant or zero-initialized heap is assumed.
Caller addresses and calldata bytes are also arbitrary. `CALLDATALOAD` uses the
same zero-extending byte reader as direct frame execution, and `CALLER` widens
the frame's 160-bit address. Frame construction is covered by kernel-checked
correspondence lemmas. Size words are independently quantified, a conservative
superset of consistent frame contexts. Fixed scalar-window inputs still reject
these reads rather than defaulting their missing inputs to zero.
An iterative analysis of the independent source graph and reflected stack
effects guides finite underflow/overflow proof splits. These are tactic hints,
not assumptions: the theorem still quantifies every stack and live memory.
The importer still checks constant access ranges; dynamic targets, unknown
addresses, cycles and physical gas remain outside this route. Counterexamples
on hypothetical stacks are not claimed to be reachable compiler failures.

Scalar rewrites can also follow an identical memory prologue. The independent
block export must agree on that entire prologue and on the remaining suffix,
terminal and physical successor. `MemoryPrefix.charged` executes the shared
operations, including dynamic and expansion charges, stores and early faults.
Kernel-checked append/scalar lemmas bind the staged proof to one sequential
instruction execution. The concrete scalar operations must prove their costs
are fixed; an unverified static price cannot hide a dynamic EXP charge.

Equal-cost scalar rewrites preserve all gas budgets. Cheaper rewrites require
sufficient scalar gas after each successful prologue execution and an unchanged,
gas-insensitive continuation. A prologue fault is never assumed away. Symbolic
memory addresses, changed stores, changed continuations and code-image/call
observers remain unsupported. A scalar window missing caller/calldata inputs may
try this universal route; its requested height is recorded separately from the
theorem's arbitrary incoming stack. Actual counterexamples are never replaced.

Large call-free scalar MIR bodies have a compositional effect-tree route. The
independent Rust export supplies typed instructions and control edges. A trusted
renderer expands the acyclic CFG, resolves phis simultaneously and evaluates
scalar operations using the existing MIR word definitions. All other admitted
opcodes remain requests with their identity, ordered operands and result shape.
The proof quantifies every input and every request's result word. The theorem
`MIR.Effects.contextual` lifts tree equality to equality under every stateful
handler and terminal interpretation, including handlers that stop on a fault.
No storage-key separation, empty memory or successful effect is assumed.

This is an `independent-scalar-effect-tree` import and a
`compositional-effect-tree` proof, not a `universal-caller-state` certificate.
Unlike the pure scalar import certificate, it does not prove correspondence to
the direct SSA interpreter: the schema exporter and constructor renderer remain
trusted. Supplied acyclic internal calls expand with private arguments/locals
and shared request state. Normal returns resume the caller; frame-ending
outcomes do not. `MIR.Effects.interpret_resume` proves this continuation rule for
every handler, including handlers that fail. Allocation requests preserve size,
zeroing and checked-failure policy. Their replies are arbitrary word carriers,
without a successful allocation, freshness, validity or implicit FMP assumption.
Pointer offsets retain wrapping word arithmetic, not a provenance proof.

External calls, tail calls, layout queries, code-image reads, GAS sampling and
undefined operands are rejected, including unused instructions in callees.
Expansion is bounded to call depth 32, renderer nesting 128, 4,096 work units
and 256 KiB of generated text; exceeding a bound is unsupported. SSA phis remain
simultaneous. The supplied source model and this continuation renderer remain
trusted; there is no direct-interpreter or backend return-placement certificate.
Call imports record `independent-call-effect-tree`, their supplied function
count and allocation sites. Scalar steps are unpriced. SSTORE remains an
arbitrary request that may fail; changes to physical gas before its sentry are
not certified.
Negative regressions change effect values/order, omit a store and change a
terminal. A broken WETH width check also has a concrete kernel/native witness.
WETH's `DOMAIN_SEPARATOR` SCCP pair retains its allocating helper in
`examples/mir/domain-separator/`. The wrong helper length has a concrete
kernel/native witness; its memory fixture does not establish the universal
result.

An effectful EVM stack-prefix route retains memory/hash/log traffic without a
bounded memory projection. The independent complete block export and reflected
stack signatures supply its instructions. Scalar operations use the existing
checked stack semantics and fork cost table. Each effect requests its opcode,
consumed words and result shape. Handlers are arbitrary gas-independent state
transitions, with arbitrary replies, failures and complete dynamic charges.

`EVM.Effects.expansion_correct` relates that tree to instruction-by-instruction
charging. `EVM.Effects.resource` proves that lower scalar charges between the
same requests preserve every original success, including the complete stack,
live world and no less remaining gas. The generated entry theorem applies both
lemmas to the actual imported lists. All stack heights and words are quantified;
underflow and the 1024-word limit are retained, never sampled away.

The certificate is `universal-effectful-stack-prefix`. The remaining suffix,
terminal and physical successor must match. It requires a gas-insensitive
continuation and does not prove physical label placement or full-state handler
correspondence. The parser, schema mapping and concrete handler binding remain
trusted. Only memory/hash/log effects are admitted; calls, unpaired or dynamic
interior transfers, code/GAS observers and storage writes are excluded. Exact
gas is not silently replaced by refinement. Changed log arguments and increased scalar charges fail
validation. The reduced real Morpho pair is in `examples/evmir/log-stack/`; a
concrete broken-topic entry has matching Lean and native counterexamples.
Amsterdam's two-resource gas is rejected rather than projected onto one budget.

Adjacent static label PUSH/JUMPI pairs additionally return either the remaining
prefix's result or a taken label with its complete live stack. `guardCharged`
retains separate PUSH and JUMPI debits and the temporary label stack slot.
`guardCharged_eq` and the extended `expansion_correct` bind the interaction tree
to those sequential steps. All guard conditions are symbolic; neither their
outcomes nor incoming stack heights are sampled. The generated theorem compares
the exit as well as the world, stack and remaining gas.

The `universal-branching-effect-prefix` certificate quantifies a shared per-label
pair of physical PUSH/JUMPI charges. Logical label numbers are not byte offsets
and do not determine PUSH width or exclude PUSH0. The shared physical price and
relocation binding, unchanged target continuations and their gas insensitivity
remain explicit trusted boundaries. Unpaired transfers and missing labels are
rejected. `examples/evmir/decoder-branches/` reduces Morpho's real `block-cse`
pair; the tests also check a wrong store offset independently in Lean and Rust.

An additional `universal-post-precompile-suffix` certificate retains an exact
common prefix that contains GAS and recovery-precompile calls. The independent
block exporter supplies the entire prefix, including immediate operands; the
audit binds its hash and instruction count. An arbitrary shared executor
receives that same sequence on both sides. Its result carries either an
arbitrary halt outcome or the updated context, stack, gas and complete world.
No shared instruction is discarded or replaced by an entry-state assumption.

`EVM.Suffix.stopped` preserves every early prefix halt exactly, including any
failure and its observable state. `EVM.Suffix.resource` composes the successful
prefix with the checked suffix interaction refinement. Every original suffix
success retains its edge, full stack/world and no less gas. Stack faults and
temporary label slots remain in the suffix execution model. The scalar
predicate lemmas cover both equality outcomes over complete 256-bit words.

Admission permits only canonical `push 1; gas; staticcall` prefix calls. The
target-position and always-active recovery-precompile lemmas are kernel-checked.
Code-image observers, relocations and arbitrary external calls are rejected;
suffix handlers and the unchanged continuation must be gas-insensitive. The
shared prefix-executor binding and source renderer remain trusted, as do the
shared logical label prices. There is no full-state interpreter, physical
placement or whole-contract certificate. Exact gas and Amsterdam's two-resource
gas are rejected. Prefixes are limited to 256 operations and suffixes to the
existing 64-operation/32-word proof budget.

The real WETH pair binds all 103 common prefix instructions. The reduced pair
in `examples/evmir/post-precompile/` includes a concrete broken-predicate
counterexample independently checked by Lean and Rust. Negative regressions
change the prefix, call target or predicate; early prefix OOG also has a direct
exact-gas regression.

## Real-contract evaluation

Run `solar-tv/evaluate --limit 8 --timeout 10 --seed 0 --output DIR` with a
fresh directory. [evaluation.json](evaluation.json) pins source archives by
SHA-256. The contracts are Solady WETH, OpenZeppelin ERC20Mock and Morpho Blue.
The runner compiles their real import closures for Cancun, via IR, with the
optimizer enabled at 200 runs and metadata trailers disabled.

Each case has a normal compilation and an unchecked capture compilation. Their
ABI, creation bytecode and runtime bytecode outputs must match exactly. Capture
stores every changed region, including ones the hook already knows it cannot
verify. The deterministic sampler alternates layers and passes before taking a
second region from a group. Seeded hash ordering selects within each group;
successful checks receive no preference.

The prefix-only baseline, using the same selected IDs as the original 1/24 run, produced:

| Contract | Captured checks | Selected | Verified | Unsupported |
| --- | ---: | ---: | ---: | ---: |
| Solady WETH | 329 | 8 | 2 | 6 |
| OpenZeppelin ERC20Mock | 372 | 8 | 1 | 7 |
| Morpho Blue | 1,515 | 8 | 0 | 8 |

All three output comparisons were identical. There were no counterexamples,
timeouts, inconclusive checks or errors among the 24 selected checks. The other
2,192 captured checks were not selected and have no verification claim. The
verified checks are an EVM IR tail-merge region in WETH, its `block-cse`
constant-push reuse before `LOG3`, and ERC20Mock's `peephole-early` stack cleanup
before a dynamic return. The first prefix is exact-gas; the second requires the
recorded gas-insensitive continuation. All three output fingerprints also match
the previous baseline. This small sample is a reproducible local coverage
measurement, not a whole-program soundness result or a broad
comparison with other validators. CI uses four checks per contract and retains
its own fingerprints, denominators, timings, reports and full input pairs.

## Automatic typed source execution

The scalar expression projection has a separate automatic fallback: direct
execution of the Rust-exported typed SSA, including void/multiple returns,
aggregate arguments/results and acyclic helper calls. The checker reconstructs
the source model before checking; a changed cached body or signature cannot
certify itself. There is no reduced scalar tree to compare in this route. The
compiler parser/schema exporter and constructor renderer remain trusted.

The generated theorem quantifies every typed argument and arbitrary initial
journal, byte-memory contents, return data, caller environment and block context.
Initial memory is aligned and at most 64 KiB. `State.equivalent` requires two
successful interpreter results and compares all live boundary observations.
It is false for equal model errors. Work is bounded structurally over the
acyclic CFG/call graph; exhaustion cannot satisfy the theorem.

The real storage, memory, multiple-return, aggregate and helper-call `egraph`
pairs in `examples/mir/universal-state/` pass this route. Their broken storage,
memory and return-value versions have kernel-checked, independently replayed
counterexamples. Concrete scenarios never establish a universal positive result.
Unproved pointer validity and erased-layout witnesses are not counterexamples.

The current sample, with the exact same 24 selected IDs, uses a 30-second budget:

| Contract | Captured checks | Selected | Verified | Unsupported |
| --- | ---: | ---: | ---: | ---: |
| Solady WETH | 329 | 8 | 7 | 1 |
| OpenZeppelin ERC20Mock | 372 | 8 | 6 | 2 |
| Morpho Blue | 1,515 | 8 | 4 | 4 |

All ABI and creation/runtime bytecode fingerprints remain identical to the
earlier run, and capture remains bytecode-neutral. There are no counterexamples,
timeouts, inconclusive checks or errors. Five additional EVM checks use universal
terminal-region proofs: WETH block layout, ERC20Mock tail merge, loop layout and
block layout, and Morpho tail merge. All retain independent source-CFG
certificates. The ERC20Mock layout pair's custom-error path reads the caller and
consumes incoming stack words. Real layout and calldata CSE examples
are in `examples/evmir/caller-tail/` and `examples/evmir/calldata-prefix/`.
Broken caller and calldata reads are rejected by universal proofs and have
concrete counterexamples checked by both Lean and the independent Rust replay.
Morpho's `reorder-expressions` pair now verifies after a charged shared memory
store. The reduced real-pass example and an independently replayed wrong-offset
counterexample are in `examples/evmir/memory-prologue/`.

ERC20Mock's complete `burn` / `_update` `spell-width-tests` pair now also closes
with the same Lean heartbeat limit, over arbitrary caller state. Source tables
are shared, observations/finalization wait for completed execution, and generic
width predicates use the executor's canonical zero representation. Branch
splitting requires the proposition to be closed, rather than the continuation's
branches. No helper is replaced by an assumed effect summary. The reduced real
pair is in `examples/mir/erc20-width-guard/`; a smaller stateful pair and a broken
padding guard with kernel/native replay are in `examples/mir/width-guard/`.

WETH's complete `transferFrom` width-guard pair now verifies in about 1.2 seconds
through the compositional route described above. Its unchanged storage/hash/log
body is retained as exact effect requests instead of repeatedly evaluating that
body. The reduced real pair and independently replayed wrong-width fixture are
in `examples/mir/weth-width-guard/`.

Morpho's `peephole-early` swap cleanup around `LOG4` now verifies through the
effectful stack-prefix route above. Its decoder's `block-cse` pair also verifies
through all four early guards. WETH's `DOMAIN_SEPARATOR` SCCP pair verifies
through its allocating helper. Its post-precompile predicate peephole verifies
through the shared-prefix/suffix route. WETH's removal of its two unreferenced
token-transfer hooks now verifies through rooted function-table correspondence.
The allocation-coalescing constructor now discharges the conditional prefix
obligation above, including its explicit allocation-producer contract. This
does not establish that every caller satisfies the contract. The same sample
therefore verifies 17 of 24 recorded obligations; 7 remain unsupported, and
check identities and bytecode hashes are unchanged.

The ten-second control run verifies 16 checks, with 7 unsupported and the larger
MIR proof still timing out. It takes about 25 seconds at the larger budget. The
budget difference is explicit; this is not a seventeen-of-24 result at ten seconds.

A `universal-exact-scalar-island` certificate handles a scalar rewrite inside
an otherwise unchanged stateful block. It retains the independently exported
common prefix, complete suffix, terminal and physical successor. The kernel
checks their equality, the middle's stack behavior for every input and its exact
cost. `EVM.Suffix.scalar_contextual` lifts that proof through arbitrary shared
context executors, including prefix exits and gas-sensitive suffixes. Prefix
storage updates, logs and static guards are not discarded or replaced by
successful-access assumptions. The contexts' source/executor bindings remain
trusted; this is not a direct full-state importer certificate. Code-image
observers, external calls and Amsterdam two-resource gas remain excluded.
The real reduced `block-cse` example, malformed contexts and exact-gas boundary
tests are in `examples/evmir/interior-cse/` and `tests/test_interior.py`. Its
small wrong-store-address counterexample has kernel and independent Rust
evidence; native execution also checks the larger storage/log context and its
early failures. A hypothetical bad stack at an unknown entry still cannot be
reported as a reachable compiler counterexample.

The separate `universal-source-graph-identity` route covers exactly identical
independently exported reachable EVM graphs, including previously unmodeled
stateful operations. Kernel-checked data equality retains entry/node identities,
all operands and terminals, control edges and transfer stack-growth checks.
`EVM.SourceGraph.identity` proves equality for every initial state and every
execution bound under shared node handlers. Handlers may mutate state or fail;
stateful cycles do not acquire a termination or fixed-unrolling assumption.
No context read is replaced by zero. The Rust parser/CFG normalizer and raw
constructor binding remain trusted, and this is a structural certificate, not
an opcode-implementation, physical-gas or backend-layout proof. Code-image
observers and calls remain excluded. The fallback only handles unsupported
instructions; it cannot replace failed import certificates or counterexamples.
The real layout pair and a changed-store kernel/native witness are in
`examples/evmir/graph-identity/`. Tests also reject an unsafe jump-growth change
and demonstrate its possible overflow under an abstract handler, without
claiming that arbitrary handler state is reachable in the source module.

Reordered graphs have a separate `kernel-checked-schema-stack-layout` route.
`EVM.StackGraph.contextual` proves simulation under an explicit node mapping
and per-source-node height bounds, checked by the kernel with `decide +kernel`.
The entry bound is 1024, not a sampled frame height. Opcode inputs/outputs come
from the shared schema; the interpreter checks underflow and overflow and fixes
the resulting height. Handlers keep an arbitrary payload and may mutate it or
fail. Different zero/one-slot jump checks match only when the certificate proves
the source height at most 1023. Tests reject forged predecessor bounds and show
the overflowing abstract execution; they do not claim that execution is a
reachable whole-module counterexample.

This theorem holds for every initial height, live payload and execution bound,
including cyclic paths and early failures. It retains the source graphs rather
than deleting transfer checks. Parser/CFG normalization, schema data binding and
the concrete opcode-handler/height embedding remain trusted. Transfers are
unpriced: physical gas equality is not claimed. A raw dynamic `JUMP` is an
observed full-payload exit under a shared logical continuation; target resolution
and physical return-address correspondence are not established. Reports retain
these exit nodes. The real layout pair, changed-store kernel/native witness,
unsafe-growth and bytecode-neutrality tests are in
`examples/evmir/stack-layout/` and `tests/test_stack_layout.py`.

## Remaining obligations

After rebasing onto `main`, the new frozen sample has a different selection of
24 obligations: its baseline verifies 15, with 9 unsupported and no timeouts or
counterexamples. This is a new compiler/capture fingerprint, not a regression
comparison with the earlier 17-of-24 sample above.

Rechecking those same frozen pairs at 30 seconds verifies **24 of 24**, with
no unsupported results, counterexamples, timeouts or errors. Morpho's `lower-alloc`
constructor first raised the count to 16; the OpenZeppelin ERC20Mock
`lower-alloc` constructor then raised it to 17, and WETH's `block-cse` window
raised it to 18. WETH's `block-layout` region then raised it to 19 through the
separately reported source-graph identity certificate. ERC20Mock's
`block-layout` region then raised it to 20 through the stack-layout simulation.
WETH's `lower-alloc` body raises it to 21 under the explicitly reported
fresh-heap placement/FMP producer condition. Morpho's unused type-table erasure
raises it to 22 through resolved module-data identity. The two
`lower-builtins` certificates described below close the sample.
The selected check identities and input hashes are identical. Fresh compilation
of the three inputs reproduces the saved ABI and creation/runtime bytecode.
The original capture compiler fingerprint is
`8a897a40c88678e68e53f3b08b0e86099d5190f94478681ceda9c68e3ef3ae37`
and independent replay is
`b3e69bf9df7c153571cb67cecc99249abcb8a75c4600d0f51bfcdcd4183e182d`.
The current validation compiler is
`3b234fd5cb924dc426bbc8048f0093a3f90cf9635090fff7b1b890a92216dc6e`
and the helper with structural module export is
`122a13e1e1891d513c3b8c351176888332e623b8e6465007755ee1da407f6f57`.
The original captures and check IDs are reused; plain ABI and bytecode outputs
from the new compiler match every saved contract fingerprint. Live audit and
capture bytecode neutrality also have end-to-end tests.

Morpho's `lower-alloc` constructor pair now verifies through the compositional
effect-tree route. The FMP request is defined as `MLOAD(64)` and its primitive
correspondence is kernel-proved in `MIR/Transforms/LowerAlloc.lean`, including
memory expansion and failures. Constructor base/end requests retain their
discriminator and arbitrary results; no layout, cursor, fresh memory or
successful access is supplied. Whole SSA projection and call expansion remain
trusted. This does not establish physical gas or backend-layout correspondence.
The reduced real pair, changed-slot counterexample with independent replay,
and constructor-range distinction tests are in `examples/mir/fmp-lowering/`
and `tests/test_fmp_lowering.py`.

ERC20Mock's constructor uses the conditional allocation-prefix bridge described
above. Its cyclic storage helpers and ordered call arguments remain identical.
The same pass also changes `load_storage_bytes`, which is outside the
constructor's reachable call graph and explicitly excluded from this
function-local claim. Reports retain the unchanged callee set and list that
unrelated changed body. Neither loops nor allocation preconditions are silently
assumed away.

WETH's unknown-height `block-cse` window uses the exact scalar-island
certificate. Its repeated nonzero constant push becomes `dup 1` before a log.
The complete shared prefix retains its storage updates, memory
work and static guard, including all early exits. Its gas-sensitive storage
sentries cannot be bypassed with a cheaper-prefix assumption. Shared context
executor bindings and logical relocation context remain explicit trust
boundaries; no physical code-placement or whole-pass claim follows.

WETH's `block-layout` region retains exactly the same independently exported
reachable graph, including its stateful reads/writes, context reads, log and
branch edges. The structural identity certificate preserves these nodes under
shared handlers without supplying a stack height or zero-valued context. This
does not establish opcode implementations or physical transfer prices; those
remain separate from source-graph equality.

ERC20Mock's `block-layout` region keeps every source operation and edge but
reorders node IDs and changes three temporary transfer checks. Their checked
source-height bounds are each 1021, so neither version can overflow at those
transfers. The kernel verifies the node mapping, local bound obligations and
simulation for every execution bound. Its dynamic return is an explicit
full-payload exit under a shared logical continuation. This does not resolve
that target, prove physical return addresses or price the changed jumps.

WETH's `name.body` now verifies through complete direct typed SSA execution,
including its initialized bytes, final FMP and returned pointer. The supplied
named heap floor must equal the live FMP, with an aligned, active cursor word
and a source reservation inside the 64 KiB work domain. The kernel proves the
reservation, explicit initialization and non-vacuity of admitted source
extents. A concrete disagreeing floor demonstrates why the condition cannot
be omitted. Proving that callers and actual backend placements establish this
condition remains separate work; this is not unrestricted caller-state or
physical-gas equivalence.

Morpho's `lower-structs` pair removes only `struct0` through `struct3` from the
presentation table. All 47 functions, including unreachable bodies, are
independently exported with resolved types. The kernel checks the complete
typed tables and retained text for equality. This is the structural certificate
described above, not a claim that every primitive operation or physical layout
has a proved interpreter embedding.

The final two frozen pairs are from `lower-builtins`. The validator imports
the actual typed MIR on both sides, checks the complete expansion and relevant
callee bodies, and applies the Lean proofs to the matched SSA.

- OpenZeppelin's `store_storage_bytes` verifies the whole write and its
  introduced clear helper. `MIR.LowerStorageBytes.store` composes header
  decoding, the checked extent and the two storage loops. The loop checks
  retain the actual phi inputs, wrapping slots, full-word order and masked
  tail. The certificate covers arbitrary headers and live object memory in
  the 64 KiB and 2,048-word work domain, with enough call depth for the
  clear helper. Malformed headers return `Panic(0x22)`, and length rounding
  overflow returns `Panic(0x41)`. Static writes fail and scratch effects on
  successful paths remain visible.
- Morpho's `safeTransferFrom` verifies CALL and its returndata allocation.
  `MIR.LowerReturndata.memoryObjectCall` preserves the message request, child
  outcome, returndata bytes, memory and return pointer. The typed check
  binds the GAS producer and retains the entire shared prefix, continuation
  and called helper. The frozen check uses the exact emitted CALL budget
  described below. The standalone MIR checker can also check this shape using
  a supplied budget when the final image is unavailable.

The frozen certificates cover these exact shapes. General loop invariants,
dynamic placement and whole-pipeline physical gas correspondence remain
outside the sample.

### Exact emitted CALL gas

The `call-budget` check captures the actual `lower-builtins` pair, scheduled
EVM IR and final runtime in one compilation of the pinned Morpho input. The
final bytecode is decoded from the emitted bytes, with each PUSH width and
resolved branch destination retained. The selected prefix must match the
scheduled helper's operands and ordered ABI writes. The byte decoder,
scheduled function binding and projection into priced events are trusted.
The kernel checks the imported trace against the independent price law.

Fixed work through GAS costs 142 in the scheduled IR and 148 in the emitted
bytecode. The final optimizer forms the selector with a short PUSH and SHL.
In both forms EXTCODESIZE costs 100 or 2,600, warms the token and selects the
no-code edge on an empty account. On the CALL edge, dynamic memory expansion
is the difference between the initial memory price and the price of the
highest touched word. The final footprint covers the entire 100-byte
payload. CALL adds a warm charge of 100 with zero value and zero output
bytes. If the starting gas cannot pay these charges, the prefix runs out of gas.

The kernel checks that the normalized guard and encoder instructions imported
from MIR are exactly those executed by `runMirPrefix`. The shared helper-entry
state fixes the typed arguments, environment, journal, active byte memory and
returndata. The guard warms the target on both sides. The scalar MIR encoder and
the projected bytecode stores write the same words in the same order, including
the overlapping selector and first argument. The resulting full memories agree,
including untouched bytes outside the object. This derives the warm journal,
memory extent, and 100-byte length word needed to price the call.

The imported write sequence also has a symbolic child-input theorem for
separate bounded MIR and machine arenas. Each arena may start with a
different free pointer and dirty contents. The selector and argument stores
cover every byte in the CALL payload, so both layouts pass the same bytes
for arbitrary sender, receiver and amount. This relates the child input.
The full ABI proof still needs a relation for the remaining live state and
the physical active-memory extent.

The earlier MIR GAS sample reaches the same EIP-150 cap as the emitted sample.
Both sides execute the same child code with that gas, environment, input
and journal. SUCCESS commits the child journal. REVERT restores the checkpoint
and preserves the returned bytes and unused gas. Exceptional halts clear the
buffer. The retained caller gas and unused child gas form the continuation's
budget.

The final continuation starts with the real CALL result on the decoded stack.
Its stack projection checks both operands of each resolved branch, the FMP
bump, the length store and the exact-length copy. The proof matches that
collector to the semantic and lowered MIR, including dirty padding, and
matches the physical signed length and boolean tests to the direct MIR
decoder. The common error handler produces the exact owning-frame revert
bytes. The cost ledger charges the selected path, copy words and memory
expansion. These are physical charges applied to the matched observation
because MIR instructions carry no opcode gas.

The certificate uses Cancun, a valid internal return address, seven extra
stack slots, a live FMP slot and word-aligned byte memory within the 64 KiB
model. It admits up to 30 million entry gas and requires known child code and
bounded returndata allocation. The prefix price model also checks no-code
transfer and insufficient gas before CALL.

The optimized EVM IR is matched against the runtime's opcode bytes and resolved
labels. The named dispatcher entries bind the five caller sites, including
the `supply` fallthrough and the two shared return destinations. The decoder
checks which stack words reach the ABI stores. The MIR importer checks the
void return or both inserts into the typed product in each caller, in both
snapshots. Three callers return the transferred amount as one word. Its
identity is checked on the MIR and bytecode call operands. The other word is
related at the shared live-in boundary.

After a helper success, the proof matches the caller's STOP or two ABI words
in order, charges the physical return block, and keeps the committed world.
A helper revert skips publication and uses the owning invocation's entry
journal with the exact revert bytes. Out-of-gas also rolls back. The theorem
starts at helper entry with related caller live-in values and the owning
checkpoint.

With `--check-caller-entry`, each emitted route from PC 0 to its named ABI
decoder is proved for arbitrary calldata with the matching selector and
length in the word domain, zero call value, arbitrary journal, and enough
gas to reach that decoder. The proof checks each actual opcode, PUSH
immediate and taken jump destination against the emitted image, and its
charge against the Cancun opcode table. Every step carries the world through
unchanged. From there a second symbolic segment checks the emitted length
and address guards, tuple stores and bytes-slice decoding for the canonical
ABI head offset. It derives the physical stack, active memory, and residual
gas for arbitrary valid head words and callback bytes. The same checked
debits prove that every smaller gas allowance stops at the first unpaid
instruction, with empty returndata and the entry journal restored.
Aligned word-store lemmas relate the physical memory to the MIR entry frame.
The MIR ABI model separately relates the typed arguments to the canonical
encoding.

For Supply, Repay, SupplyCollateral and Liquidate the symbolic physical
prefix continues through the first market storage read. It checks the
`MarketParams` hash, both mapping scratch stores, the derived storage
key, and the loaded word for an arbitrary journal. The corresponding
typed MIR prefixes run from canonical ABI decoding through the actual
load on both sides of the rewrite and agree on the value, memory and
journal. The physical proof checks the Cancun warm or cold charge and
the slot-warming update. An underpaid read halts without warming the
slot. Rust replay varies the market input, storage value and warmth and
checks the exact gas boundary.

The market guard certificate continues from that read through the emitted
mask, stack shuffles and branch, with the exact stack and gas at either
successor. Both MIR versions continue from their checked SSA frame and
branch on the same low 128 bits. The metered prefix composes with dispatch,
decoding and the price of the first storage read. This proves rollback for
every budget below the guard successor, even if the slot had already been
warmed. For a missing market, the proof continues through the shared error
encoder and proves the exact `Error("market not created")` bytes, remaining gas,
and rollback. Independent replay checks both gas outcomes with a cold and warm
slot, and tests a nonzero high half with a zero low half.

FlashLoan's zero-assets branch is checked from ABI entry through its REVERT.
The certificate includes the memory expansion for the error payload and
proves the funded receipt or out-of-gas rollback for any incoming balance.
Independent replay checks the threshold and surplus gas.

SupplyCollateral carries this symbolic certificate through the first
position update for canonical calldata, a created market, a nonzero
amount and owner, and a collateral sum that fits in 128 bits. The emitted
path checks both nested mapping hashes, the original and current slot
values used for pricing, the SSTORE gas sentry, the signed refund, and the
exact post-store stack and memory. Both captured MIR bodies execute the
narrowing call and checked packed update and agree with the physical
memory and journal aside from the EVM's metering refund. Rust replay
checks cold and warm reads, fresh and nonzero slots, overflow, and a
one-gas-short store. It also checks the actual callback's forwarded gas
and that a reverted callback restores the position and discards the log.
For empty callback data, the symbolic certificate continues through LOG4,
the callback branch and the final stack setup to the transferFrom entry.
Both captured MIR bodies reach the same helper call with the same arguments,
memory, and journal aside from the EVM metering refund. The linked proof
derives the exact entry gas from the fork prices, incoming journal, and
emitted schedule for arbitrary admitted ABI words. Independent replay
reaches the helper at zero remaining gas and fails one unit below that
threshold, restoring the position and discarding the log.

`MorphoCallContext.lean` checks the full captured MIR tables in one proof
for all five callers. Each unchanged body is equal and every replacement
retains its call signature and return-buffer identity. The shared
interpreter runs to the first replacement or a terminal result, carrying
the complete machine and residual work fuel. The composition theorem
has an explicit obligation at that cut. Checked local segments can
resume each side at its own residual work fuel. The capture identifies
`safeTransferFrom`, `safeTransfer`, and `setAuthorizationWithSig` as the
replacement bodies. The `safeTransfer` CALL and returndata expansion has
a separate local certificate with a shared incoming budget.

The `setAuthorizationWithSig` certificate checks the two expansions in the
actual MIR pair. Its packed object proof preserves the length word, the
overlapping payload stores, dirty tail bytes, and the free-memory pointer
update for arbitrary domain and digest words. The STATICCALL proof keeps the
cleared output word, request, child result, returndata, journal, and remaining
gas. It requires a paid GAS sample that covers the incoming message budget.
The checker compares the remaining MIR, including the success and revert
paths and the immutable read, after contracting the two checked expansions.
Independent replay compares the original operations with the lowered form at
a concrete sampled budget. It catches a shifted payload store, a corrupt
scratch clear, and a request too small to run the recovery precompile.

The authorization ABI entry certificate starts at PC zero and reaches the
deadline check for a complete, canonical head. It checks selector dispatch,
both address guards, and the boolean guard. The emitted arena agrees with
both captured MIR entry configurations, including the authorization object
and the signature calldata span. Independent replay reaches the cut with
the exact gas charged by dispatch and decoding, exhausts gas one unit below
it, and rejects noncanonical address and boolean words. The next proof
links the emitted timestamp check to both typed MIR branches, including the
expiry branch, and checks the charge on either edge. The expired path
continues through the emitted error encoder to REVERT. The kernel proves the
exact `Error("signature expired")` payload, remaining gas, and rollback for
a funded invocation, and the out-of-gas result for every smaller balance.
For an unexpired authorization with a nonwrapping nonce increment, the
prefix continues through the emitted mapping hash, warm or cold SLOAD, and
checked-add guard
to the nonce SSTORE. The proof derives the canonical key, old nonce,
incremented nonce, residual gas, and warmed journal from ABI entry. It
composes with the typed store continuation, including the write's gas sentry.
Independent replay checks the store boundary with cold and warm slots and
different old values.

The nonce-to-exit theorem starts at that exact frame, executes the typed
SSTORE, and resumes either recursive evaluator. Both continuations produce
the same outcome, returned bytes, gas, work and journal as the emitted
runtime. Where the nonce matches and the write and encoding are funded, the
kernel derives the six-word signing record, its Keccak hash and the active
memory required by the packing proof. This slice costs 217 gas after the
write. Independent replay checks the record and charge, then checks whole
receipts for a valid signature, a mismatched nonce, a bad signature, and the
SSTORE gas sentry.

The separate emitted recovery certificate reads the actual GAS and STATICCALL
bytes from the captured runtime. GAS either exhausts the frame or deducts
two gas and pushes its remaining balance directly into STATICCALL. Given the
matched scratch memory, calldata slice, stack arguments and journal at that
cut, the kernel derives the MIR message budget from the frame. It compares
both memory debits, the EIP-150 split, the child result, unused gas and work.
The recursive evaluator substitutes both captured MIR expansions in the same
callback. The emitted packing proof starts with the domain word read from
the captured runtime, a symbolic digest, and a live memory arena that already
covers the entire allocation. It checks the 32 emitted instructions through
the next hash boundary, including the free-memory pointer, the five stores,
the exact stack and memory, and the 96-gas charge. The recovery proof checks
the child message with the remaining physical budget. The evaluator also
uses the typed MIR store at both emitted SSTORE sites, preserving the gas
sentry, slot warming, and refunds from transaction-original and current
values. The certified regions resume the same frame at smaller work budgets,
so the nonce write, packing, recovery, and authorization write compose in
one invocation.
Independent replay runs a signed authorization reentered by the token during
transferFrom. It checks committed writes and logs, the clear refund and outer
rollback. The changed-domain counterexample agrees with Rust running the
altered domain immediate in the runtime. Changing the recovery GAS or the
authorization word also changes the committed result. Other authorization
instructions still execute as bytecode, including dispatch, the checks and
hashes around these regions, and low-gas exits inside the packing sequence.

The reentry certificate executes Morpho's emitted dispatcher and substitutes
the actual captured `owner()` MIR at the checked getter PC. Both typed bodies
read arbitrary slot-zero data and publish the same address as the emitted
instructions, including the discarded high bits, slot warming, memory charge,
remaining gas and work. The recursive driver proves that this substitution
preserves the entire EVM result across nested calls and continuations.
Independent replay covers a token that reenters the getter, retains its
result on success and loses its write on the outer revert. Other selectors
without certified regions execute with the bytecode interpreter. Whole-body
typed correspondence for reentrant writes remains open.

The ABI gas certificate starts at PC zero for Supply, Repay,
SupplyCollateral, Liquidate and FlashLoan. It accepts arbitrary modeled
calldata, transaction state, block context and gas for the five selectors.
It derives the residual gas at the helper, or the terminal result if
execution halts first. The kernel proves that the signed debits balance and
that resuming the reached frame is equivalent to uninterrupted execution.
The prepared Cancun message proofs also bind both memory debits, the account
charge and the EIP-150 split to the emitted CALLs and Liquidate STATICCALL.
For a forwarded call, the ledger records the unused gas returned by its
actual child. Independent replay checks the ABI-to-helper frame in each
supplied world and on all five nonempty callback routes. It also checks
nested reentry, callback reverts and exceptions, and out-of-gas before the
helper. The FlashLoan test samples gas in the callback itself.

The CALL debit lemma relates the EVM input and output memory steps to
MIR's cumulative debit. It covers both unpaid legs, including an input
that fits the budget and an output that does not. The existing
`safeTransferFrom` proof instantiates it using the active memory derived
from the emitted setup. Other incoming budgets and the continuation
obligations at reachable replacements remain to be discharged.

The concrete ABI-to-exit certificates compose both segments for the
supplied input, resume with the exact residual gas, and then
prove reachability in the emitted runtime and both captured MIR bodies.
Valid alternate bytes offsets use the concrete trace from the dispatcher
exit. The certificates check the stack, ABI words and FMP at the helper cut.
Each execution resumes with its own reached memory and journal, so the proof
carries the actual writes
from the prefix into the suffix. On success it compares the ABI publication,
committed accounts and logs. On revert it compares the payload and rollback
to the owning checkpoint. The bytecode prefix supplies gas at the cut and
the prefix's message budgets. The MIR suffix derives its GAS sample and
CALL budget from the checked Cancun schedule. The kernel compares those
values with the emitted suffix trace. MIR uses its own token, warmth, code
presence and FMP together with the physical entry gas and active memory.
The empty token-code path also prices the emitted error handler and its
scratch memory expansion. The kernel checks predicted early exhaustion
against the emitted trace and its sample and call ledgers. On a paid CALL, the MIR executable child
result supplies returned gas, return data and the active parent memory.
The six checked helper continuation prices and the caller's published
return cost jointly determine whether the parent exhausts its budget and
how much gas remains on an affordable exit. The kernel checks that result
against the emitted trace through the exact ABI exit. The complete ABI proof
compares exact exit gas, published bytes, committed effects and rollback.
The independent replay checks the physical refund.

`MorphoAbiSuite.lean` assembles separate certificates for the five callers.
Each certificate composes short transitions over complete states. The
instruction-aligned image chunks and concrete Keccak digests have
correspondence theorems to the list decoder and original hash. Rust replay
independently checks the physical frames at the ABI decoder exit and helper
PC, and the complete ABI exit, including exact gas and refund.
`--caller-worlds` changes the concrete inputs used to derive the certificates.
An all-input prefix simulation, general private-memory correspondence and physical MIR
instruction pricing remain open. The low-gas Supply fixture reaches the
helper with 1,343 gas and exhausts it at the cold code-size check. The
independent Rust replay and the kernel agree on out-of-gas and rollback. At
36,420 root gas Supply reaches CALL with zero gas after taking the sample.
With 36,520 gas it pays for CALL, resumes from child failure with zero
remaining gas, and exhausts the budget in the continuation. Replays also
cover a paid CALL that returns child failure with enough gas for an
ordinary parent revert, and a successful child one gas below and at the
ABI publication fee.

The independent replay preserves the emitted prefix bytes and PUSH widths.
A trampoline supplies the helper's entry stack and relocates the no-code
target within the test image. The child returns its GAS word and calldata.
Lean and the Rust executor agree on 95,572 versus 95,569 gas for a cold token
and 98,033 versus 98,029 for a warm token after a paid mutation. The payload
is identical in each pair. A separate wrapper reads the entire active memory at
CALL on a dirty entry and detects a shortened FMP bump. Both replays preserve
the emitted prefix on the successful side. Compiling with and without the
capture reproduces the same ABI and creation and runtime bytecode. The
continuation replay executes relocated emitted instructions and checks all six
paths with Lean and the Rust executor. A child that writes storage and then
reverts leaves the checkpoint unchanged. A successful child that returns false
also loses its write when the enclosing helper reverts. The independent ABI
replays execute the unchanged runtime from each of the five public selectors.
They check committed storage and logs on success and full rollback on both a
token revert and a false result.

| Gap | Current boundary | Evidence needed to close it |
| --- | --- | --- |
| General loop proof support | Cutpoint invariants over typed MIR preserve ordered effects, exits and divergence. Real certificates cover calldata, memory and storage loops, including a runtime-bounded fill. | Apply the framework to more changed CFGs. Automatic recognition currently covers supported shapes. |
| Arbitrary state and aliases | Automatic universal caller-state proofs within the defined 64 KiB domain; explicit fixtures for broader operation families | Universal account/memory relation with aliasing, reentry, rollback and fresh allocation obligations. |
| Whole-function MIR interfaces | Automatic typed products/slices, void/multiple results and acyclic calls | ABI entry, allocation/layout preconditions, recursive call invariants and broader defined memory domains. |
| EVM entry stacks and indirect transfers | Fixed entry heights, universally quantified effect-free prefixes, or complete terminal-region proofs over every stack | Stateful context/window correspondence, call/return stack relations and verified indirect targets; never assume an unknown height or destination. |
| Importer correctness | Certificates for acyclic EVM regions and acyclic typed scalar MIR | Extend independently checked imports to calls, memory, state, ABI entries and symbolic checked powers; audit the remaining Rust and opcode boundaries. |
| Physical code and resource correspondence | Explicit layout assumptions and selected priced execution | Backend layout certificates, complete caller work pricing and gas-sensitive transfers; observable code locations and resource failures must agree. |
| Fork and transaction completeness | Admitted fork-specific execution, explicit delegated account state | Transaction admission/authorization processing and Amsterdam's two-resource state gas; differential boundary cases. |
| Practical evaluation and review | Pinned three-contract sampling plus regression pairs | Wider published corpus, stronger pass coverage, independently reproduced results and external soundness review. |

In the initial sample, the largest blockers were unknown EVM entry heights and
MIR functions outside the single integer-result fragment. Automatic typed source
execution now covers those interfaces; ABI/layout and memory-domain obligations
continue to limit the selected real-contract checks. Removed function
identities, shared declaration/phase changes, internal calls, dynamic return
jumps and local calldata reads also remained unsupported. Lowering a status to
`verified` without addressing its obligation would hide a gap, not close it.
