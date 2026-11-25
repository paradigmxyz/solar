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
Caller addresses, call values, and calldata bytes are also arbitrary.
`CALLDATALOAD` uses the same zero-extending byte reader as direct frame
execution, `CALLER` widens the frame's 160-bit address, and `CALLVALUE` keeps
all 256 bits. Kernel-checked correspondence lemmas cover frame construction.
Size words are independently quantified, a conservative superset of
consistent frame contexts. A terminal region with a declared stack height
can use this certificate when its fixed-height projection cannot read the
frame context.
An iterative analysis of the independent source graph and reflected stack
effects guides finite underflow/overflow proof splits. These are tactic hints,
not assumptions: the theorem still quantifies every stack and live memory.
The importer still checks constant access ranges; dynamic targets, unknown
addresses, cycles and physical gas remain outside this route. Counterexamples
on hypothetical stacks are not claimed to be reachable compiler failures.

When an initial stack read exceeds the abstract constant-tracking list, the
importer obtains its input demand from the independently exported graph and
opcode schema. It retries tracking with that many unknown words, within the
32-word proof budget. Those words guide import analysis only. The generated
interpreter still receives the actual arbitrary stack, and the kernel checks
all shorter stacks, ordinary executions and stack-limit failures. A real CFG
coalescing regression starts with an incoming `DUP1`, stores its word twice and
returns both copies. Changing one read has matching Lean and native witnesses
from physical entry.

A serial three-second recheck of the full audit's 152 unsupported
`cfg-simplify-early` captures leaves all 152 unsupported. The reports are in
`target/tv-entry-demand-20261124/`. Input-demand tracking closes an importer
restriction, while the selected corpus still needs private-memory, stateful
continuation and resource correspondence. The recorded full-audit counts retain
their original semantics fingerprint.

The full serial regression suite at `19089442e1` passed 769 tests and 1,417
subtests in 6,548.97 seconds. Repository Python checks also passed, including
103 word-rule tests. The explicit two-worker concurrency test was deselected
to retain one proof job at a time. The compiler launcher forced `-j1`, and
both compiler and replay fingerprints remained unchanged. The environment,
command and log hash are retained in `target/tv-end-to-end-environment.json`.
This run used semantics
`5dad878e471c7f3c1215afdbf53d85bd9e28ab582a3c8c0ccb2da767fb7e4a1f`.

`EVMIR/MemoryProjection.lean` relates a chosen byte prefix and active-word
ledger to full byte memory. The prefix length is a parameter, including the
entire 64 KiB frame domain. Reads, big-endian word loads and Keccak agree for
explicitly bounded footprints, including zero-length reads at any offset.
Every successful full-memory expansion preserves the zero-extended reader.
For aligned memory within the 64 KiB model domain, projection commutes with
expansion and gives the same quadratic memory charge. This expansion theorem
also covers accesses beyond the prefix. Equal projected expansions from a
common original heap imply equality of the full expanded heaps, preserving
their unprojected bytes.

The fetched-instruction lemmas bind MLOAD, KECCAK256, RETURN and REVERT to the
frame interpreter's actual opcode byte. Opcode availability, price and arity
remain explicit configuration premises. The equations retain the interpreter's
gas, out-of-gas and model-error branches. A funded MLOAD theorem also equates
the reached stack and projected memory with the local instruction step, while
retaining the physical PC and remaining gas. The byte-order proof uses bitwise
lemmas. Root axiom guards check that these theorems depend only on `propext`
and `Quot.sound`.

`tests/test_memory_projection.py` instantiates the fetched MLOAD theorem on
compiler-emitted bytes at offsets 0, 1, 224, 1,024 and 65,504. Independent raw
`evm2` replay executes concrete prefixes with 1,056 or 65,536 bytes of active
memory, then checks the loaded word, MSIZE and exact remaining gas. Changing
the read address produces the independently computed different bytes and
expansion charges. Replacing MLOAD with POP loses its opcode proof and fails
in native execution. Word reads at offsets 225, 1,025 and 65,505 fail their
chosen footprint proofs.

`EVMIR/MemoryWrites.lean` binds MSTORE, MSTORE8 and MCOPY to fetched opcodes.
The frame model expands memory before writing, while the local model updates
its padded prefix and then the active size. The kernel proves that both orders
give the same projected result after successful frame expansion. MCOPY reads
the original source bytes before making any writes, including when the ranges
overlap. Source and destination may expand the heap in either order. The
separate expansion debits agree with the local charge for their combined
extent. A zero-length copy ignores both offsets and leaves memory unchanged.

The priced instruction equations retain out-of-gas and the exact remaining
gas. The raw instruction equations also retain errors from unsuccessful frame
expansions. Opcode rows, stack operands, alignment, footprint, and the 64 KiB
domain are explicit. Bounded writes preserve every zero-extended read outside
the prefix. Equal projected states and equal tails recover the full aligned
heaps in the model domain. The root axiom guards admit only `propext`,
`Classical.choice` and `Quot.sound` for these results.

The emitted-write regression checks MSTORE and MSTORE8 followed by MCOPY with
overlap in both directions and with zero size at maximum offsets. Independent
raw `evm2` replay checks full memory and the cleared stack at the next PC with
exactly enough gas, then checks out-of-gas one unit below. Replacing MCOPY with
POP loses its opcode proof and changes the concrete stack and memory.
Prefix reachability for arbitrary callers and whole-CFG/backend composition
remain open. Automatic window imports retain their 256-byte bound, and the
historical full-corpus coverage counts remain unchanged.

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

### Exhaustive capture audit

A serial `evaluate --all --timeout 3` run at `6ce76b1bf6` on the pinned
Cancun inputs checked every captured region using compiler build `7828580d`.
The normal and capture builds produced identical ABI, creation bytecode and
runtime bytecode for each contract.

| Contract | Captured | Verified | Unsupported | Timeout |
| --- | ---: | ---: | ---: | ---: |
| Solady WETH | 330 | 113 | 152 | 65 |
| OpenZeppelin ERC20Mock | 372 | 120 | 124 | 128 |
| Morpho Blue | 1,526 | 422 | 875 | 229 |
| Total | 2,228 | 655 | 1,151 | 422 |

There were no counterexamples, inconclusive results or tool errors. The
jump-chain certificates verified 57 EVM IR regions and 25 MIR functions.
EVM IR `cfg-simplify-early` and `compact-pushes` had the most unsupported
regions, with 175 and 103 respectively. MIR `lower-dispatch` and
`cfg-simplify` had the most timeouts, with 73 and 71 respectively. A timeout
reflects the three-second prover budget for this run.

The shared-tail extension verifies four previously unsupported Morpho
`cfg-simplify-early` captures (`bb431`, `bb435`, `bb439`, and `bb443`) in a
targeted recheck of the frozen pairs with a six-second budget. When two
paths reach one physical point in either graph, Lean checks the incoming
edges separately. Each route retains its own temporary stack bound.

### Full corpus audit on 2026-11-24

After the refill and stack-carried return commits, a fresh serial run at
`ade4f7d1c1` checked every capture with `evaluate --all --timeout 3 --seed 0`.
It used the existing `target` directory and the pinned Cancun inputs. The
compiler reports source revision `2eeb3bc839af4b5e4e6506710da234ecc0ac585e`,
built with Rust `1.101.0-nightly (92b5b8c87 2026-10-29)`, debug profile,
`mimalloc,tracing`. Lean is pinned to `leanprover/lean4:v4.34.1`. Independent
replay uses the `evm2` revision pinned in Cargo.lock,
`9b037449c846515624ff6afde739f2b764851446`.

The complete reports, logs, fixtures and capture pairs are retained locally in
`target/tv-full-20261124/`. The source archives are pinned in
[evaluation.json](evaluation.json).

| Contract | Captured | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 330 | 170 | 122 | 38 | 0 |
| OpenZeppelin ERC20Mock | 372 | 200 | 84 | 88 | 0 |
| Morpho Blue | 1,526 | 613 | 723 | 190 | 0 |
| Total | 2,228 | 983 | 929 | 316 | 0 |

Failed counts combine counterexamples and tool errors. Inconclusive counts
are also zero. All three normal/capture comparisons preserve ABI, creation
bytecode and runtime bytecode exactly. Their input and output fingerprints
also match the older 2,228-capture audit. The fresh run verifies 328 more
captures than its 655 verified baseline. The validator semantics fingerprint
stayed fixed throughout the run. The frozen 24/24 sample has its own scope.

| Artifact | SHA-256 |
| --- | --- |
| Compiler binary | `f20bd5bfabaf2b7aa81422b6682d39f1a30a722be5b3424d8f8da8a0a988be0e` |
| Rust replay binary | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Validator semantics | `eb3f337232f4f4fb57f0871648b1888b3395dc68a984e0591d519e814ca74c51` |
| Evaluation manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Complete results.json | `3cc197b9b9d4ba52277906e62e25ce862479d9291ee77f87c3e2d4d245aa7372` |

Input hashes cover canonical standard-JSON inputs, including each import
closure and compiler settings. Output hashes cover the canonical contract
outputs containing ABI and creation/runtime bytecode.

| Contract | Input SHA-256 | Output SHA-256 |
| --- | --- | --- |
| Solady WETH | `b751f0da2642fba80d176248ff576b06d45601fda253d9de32f9a78e3b70a4c0` | `671d642b2c47309ccd86125a4b1e35b281d4d2c367df2b88acacfcffb1c3c978` |
| OpenZeppelin ERC20Mock | `c85c24c264a2ae9a97b32050f93426376c8cb20e15f6e5ad5dc1414ee46ce2c4` | `5e5fb3a0613c6ec21d8b61be398fc1ee325fcce75b144d13107356276d288213` |
| Morpho Blue | `01f4aaf9db4940ea9a62aba808ba1655ef399c671b6ed29875f8a91c3efeba53` | `8ef2219d25d2b256dad76949a880ba655074bee6eb7f53bdf38e14381f43d8a0` |

The following totals retain every capture ID, including invocations that share
a module-level proof report. The three-second limit applies per prover
invocation, with the runner's separate checker wall-time limit. A timed-out
check has no positive verification claim.

| Layer and pass | Captured | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| evmir `block-cse` | 10 | 4 | 5 | 1 | 0 |
| evmir `block-layout` | 368 | 297 | 68 | 3 | 0 |
| evmir `cfg-simplify` | 7 | 5 | 2 | 0 | 0 |
| evmir `cfg-simplify-early` | 291 | 129 | 152 | 10 | 0 |
| evmir `compact-pushes` | 118 | 73 | 44 | 1 | 0 |
| evmir `copy-loop-blocks` | 2 | 2 | 0 | 0 | 0 |
| evmir `dce` | 2 | 0 | 0 | 2 | 0 |
| evmir `inline-returns` | 14 | 7 | 6 | 1 | 0 |
| evmir `late-word` | 19 | 0 | 18 | 1 | 0 |
| evmir `loop-layout` | 29 | 27 | 2 | 0 | 0 |
| evmir `outline` | 7 | 1 | 6 | 0 | 0 |
| evmir `peephole-early` | 35 | 11 | 24 | 0 | 0 |
| evmir `reorder-expressions` | 1 | 1 | 0 | 0 | 0 |
| evmir `reorder-pushes` | 5 | 0 | 5 | 0 | 0 |
| evmir `stack-normalize` | 5 | 0 | 5 | 0 | 0 |
| evmir `stack-reschedule` | 17 | 0 | 17 | 0 | 0 |
| evmir `tail-merge` | 49 | 46 | 3 | 0 | 0 |
| evmir `terminal-dedup` | 124 | 106 | 18 | 0 | 0 |
| mir `byte-run-loads` | 14 | 4 | 8 | 2 | 0 |
| mir `call-cleanup` | 1 | 0 | 0 | 1 | 0 |
| mir `cfg-simplify` | 144 | 118 | 0 | 26 | 0 |
| mir `check-elim` | 7 | 2 | 4 | 1 | 0 |
| mir `coalesce-allocs` | 1 | 1 | 0 | 0 | 0 |
| mir `cse` | 48 | 25 | 21 | 2 | 0 |
| mir `dce` | 86 | 10 | 59 | 17 | 0 |
| mir `dead-arg-elim` | 11 | 0 | 11 | 0 | 0 |
| mir `egraph` | 116 | 29 | 65 | 22 | 0 |
| mir `evm-inst-schedule` | 32 | 0 | 14 | 18 | 0 |
| mir `function-dce` | 3 | 0 | 0 | 3 | 0 |
| mir `indvar-simplify` | 2 | 0 | 2 | 0 | 0 |
| mir `inline` | 58 | 2 | 52 | 4 | 0 |
| mir `integer-cleanup` | 35 | 0 | 19 | 16 | 0 |
| mir `jump-threading` | 52 | 39 | 8 | 5 | 0 |
| mir `licm` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-abi` | 65 | 0 | 65 | 0 | 0 |
| mir `lower-abi-encode` | 16 | 0 | 16 | 0 | 0 |
| mir `lower-alloc` | 19 | 2 | 16 | 1 | 0 |
| mir `lower-arithmetic` | 16 | 0 | 11 | 5 | 0 |
| mir `lower-builtins` | 6 | 3 | 3 | 0 | 0 |
| mir `lower-checks` | 31 | 0 | 24 | 7 | 0 |
| mir `lower-dispatch` | 73 | 0 | 0 | 73 | 0 |
| mir `lower-evm-shaped` | 3 | 2 | 0 | 1 | 0 |
| mir `lower-immutables` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-integers` | 55 | 0 | 33 | 22 | 0 |
| mir `lower-mapping-slots` | 32 | 0 | 17 | 15 | 0 |
| mir `lower-memory-objects` | 27 | 1 | 26 | 0 | 0 |
| mir `lower-memory-zero` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-slices` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-structs` | 1 | 0 | 0 | 1 | 0 |
| mir `memory-dse` | 10 | 0 | 9 | 1 | 0 |
| mir `memory-dse-unread` | 1 | 0 | 1 | 0 | 0 |
| mir `merge-aborts` | 42 | 7 | 20 | 15 | 0 |
| mir `place-reads` | 33 | 0 | 17 | 16 | 0 |
| mir `sccp` | 12 | 7 | 2 | 3 | 0 |
| mir `simplify-phis` | 1 | 0 | 1 | 0 | 0 |
| mir `specialize` | 1 | 0 | 0 | 1 | 0 |
| mir `spell-width-tests` | 38 | 1 | 20 | 17 | 0 |
| mir `storage-dse` | 10 | 9 | 0 | 1 | 0 |
| mir `storage-promotion` | 12 | 12 | 0 | 0 | 0 |
| mir `word-sequence` | 6 | 0 | 5 | 1 | 0 |
| mir `word-sequence-early` | 1 | 0 | 1 | 0 | 0 |

The selected next coverage gap is EVM IR `cfg-simplify-early`, with 152
unsupported captures and ten timeouts. Its unsupported diagnostics comprise
102 entry-stack failures, 25 calldata reads, eight call-value reads, seven
escaping labels, five unpriced GAS observations, two storage reads, two
program-data pushes and one missing branch condition. These are the first
reported blockers, so removing one can expose another obligation.

The WETH `bb78` pair in `captures/evmirFALFVD/` is a representative entry-stack
failure. It starts with `DUP1; MLOAD` on an incoming pointer and continues
through hashing, storage and an external message. The chosen proof scope needs
a relation for reachable incoming stacks and private memory, with the ordered
stateful continuation preserved. The existing all-stack scalar/256-byte memory
projection cannot supply that relation. Supplying a guessed stack height or
memory address would introduce an assumption. MIR `lower-dispatch` separately
accounts for 73 timeouts under this budget.

The kernel checks the admitted region, structural and scenario obligations
named in each report. Independent replay confirms the refill snapshots and the
changed-return regression, including its output and storage difference.
Compiler parsing, source normalization, opcode mapping and backend placement
retain their documented trust boundaries. General ABI-to-bytecode composition,
spilled return addresses, arbitrary state/aliasing, Amsterdam stateful opcode
and nested-frame pricing, and fee/block settlement remain open.

### Targeted audit of early CFG and ABI lowering

The targeted audit uses the same pinned Cancun sources and a budget of 120
seconds per prover invocation. WETH and ERC20Mock completed in the original
run. Morpho's first large graph reached the stack-height stage and timed out.
The Morpho retry checks the same graph obligations in smaller Lean modules.
Each module imports the earlier proofs and the final theorem joins the ranges.

```sh
solar-tv/evaluate --all --pass cfg-simplify-early --pass lower-abi \
  --pass lower-dispatch --timeout 120 --seed 0 --output target/tv-three-pass-audit
solar-tv/evaluate --all --case morpho-blue --pass cfg-simplify-early \
  --pass lower-abi --pass lower-dispatch --timeout 120 --seed 0 \
  --output target/tv-three-pass-morpho
```

| Fingerprint | SHA-256 |
| --- | --- |
| Compiler | `64f563d6e8b2102f849dc69cb5fbea91179ce4a7c765c8205217da7c0d7c4b76` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| WETH and ERC20Mock checker | `ececd366c87b1d27c5030659d1681d7535f07b2a8b8cf72fdbf1bd4da897fb90` |
| Morpho retry checker | `29ceb5df18fc075aa269b85207ed93dd9b47a1899a9599e230411d472a94f8aa` |

| Selected pass | Captures | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| EVM IR `cfg-simplify-early` | 291 | 291 | 0 | 0 | 0 |
| MIR `lower-abi` | 65 | 14 | 51 | 0 | 0 |
| MIR `lower-dispatch` | 73 | 73 | 0 | 0 | 0 |
| Total | 429 | 378 | 51 | 0 | 0 |

The completed cases retain the same capture IDs and input/output fingerprints
as the full audit above. Normal and capture compilation agreed on ABI,
creation bytecode and runtime bytecode for each contract. All 291 early CFG
checks use physical-entry proofs. Three routed dispatch module proofs cover
all 73 dispatch records. The static ABI proof verifies four WETH wrappers,
four ERC20Mock wrappers and six Morpho wrappers.

Of the 51 unsupported ABI captures, 18 reach the direct MIR interpreter's
`GAS` restriction and 18 enter the static ABI prover without closing.
The other 15 need cyclic execution, larger or dynamic ABI layouts, constructor
or receive entry semantics, module-wide reasoning for added functions, or a
stronger typed execution proof. These counts follow the first reported
blocker for each capture.

Early EVM CFG simplification tries the physical-entry certificate before
checking individual edited blocks. The proof starts with the actual empty
operand stack and follows imported control flow, including stack-carried
continuations. The kernel checks action and interaction correspondence and
the temporary stack-growth obligations along every admitted path. The
evaluator credits a captured block through this route when both sides report
that label reachable. This is an unpriced scheduled EVM IR proof with the
existing source-import boundary.

For MIR `lower-abi`, a static selector-entry certificate compares the
original typed function behind an implicit ABI wrapper with the actual
lowered function. The implicit entry checks the head length, unsigned word
canonicality and wrapper-local nonpayable guard, then encodes the typed
results in ABI word order. The checker admits up to four static words at each
boundary. The theorem quantifies calldata with a
word-representable length, call value, block context and initial journal.
It starts from empty private memory and returndata. The kernel checks the
observable result of both direct MIR executions, including revert bytes,
accounts and logs. Compiler-validated ABI declarations and the typed
source renderer remain part of the import boundary. Dispatcher composition,
physical gas, backend placement and emitted bytecode are separate obligations.

The ABI wrapper regression also changes a returned word. The altered pair
loses its universal certificate, and direct execution of the original and
altered lowered MIR produces a counterexample confirmed by independent
Rust replay.

### Follow-up MIR ABI audit

The serial `evaluate --all --pass lower-abi --timeout 120 --seed 0` run
checked all 65 captures under `target/tv-lower-abi-followup/`.
It used checker fingerprint
`daa645951a48066b58b5badb357d032210d3999912d55c98250ba7caa15bf904`.
Compiler, replay, manifest, input, output and selected capture fingerprints
match the targeted audit above. Each contract's normal and capture
compilations agree on ABI, creation bytecode and runtime bytecode.

| Contract | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: |
| Solady WETH | 10 | 7 | 0 | 0 |
| OpenZeppelin ERC20Mock | 4 | 9 | 0 | 0 |
| Morpho Blue | 10 | 25 | 0 | 0 |
| Total | 24 | 41 | 0 | 0 |

The ten new proofs cover WETH's deposit, receive, constructor, name, symbol
and domain separator, and Morpho's market, position, market parameters and
domain separator. The changed-return regression still produces matching
Lean and independent Rust observations for each side of the counterexample.

Of the 41 unsupported captures, 18 hit the direct MIR restriction on
`GAS`, and 14 reach the ABI entry prover without closing. Four require loop
invariants, three module rows need reasoning about newly added helpers,
one constructor needs argument decoding from staged memory, and one
internal body needs a stronger typed execution proof.

The ABI checker also compares argument-free constructor initializers at their
typed return boundary, and argument-free runtime entries returning one
memory bytes object. The bytes certificate starts with a fresh free-memory
pointer at 128. It reads the object's length and contents from the source
execution and checks the result against the actual lowered offset, length,
payload and zero padding. Full-word runtime immutable loads use arbitrary
shared relocation values for unchanged declarations.

Static output supports up to eight words. The kernel checks that narrowing
the high half of a shifted storage word preserves its zero-extended ABI
encoding. Guard-free void runtime wrappers can use an effect-tree proof.
Their implicit ABI adapter maps the typed empty return to empty success.
The resulting trees must agree on ordered requests and exits under every
shared effect handler, with internal caller continuations expanded from
both source inputs.

### Contextual MIR ABI composition

The contextual ABI checker compares the source's implicit static adapter
with the actual lowered wrapper. It checks the ordered length and canonical
word guards, the decoded arguments, and the scratch stores and exit used by
the public return adapter. Calldata and call value are universal inputs.
Storage and other admitted effects remain ordered requests. Lean proves that
the trees agree for every shared handler, including handlers that fail.

`GAS`, external code observations and messages retain their identities and
operands. The proof relates their positions in the request trace. Equality
of physical gas at those positions and correspondence to emitted bytecode
remain separate obligations. The compiler validates both MIR inputs and the
independent typed exporter supplies the rendered graphs.

For internal helpers, the checker expands the implicit memory boolean
decoder and the returndata-revert adapter. It keeps unchanged calls as
requests only when the kernel checks equality of their closed callee
tables. The compiler regression changes a `CALL` gas operand and requires
the contextual certificate to fail.

The constructor adapter checks the call-value guard, reads one staged ABI
word through the supplied argument-range and memory handlers, and compares
the initializer through its typed exit. Constant-size ABI scratch encodings
and immutable stores retain their ordered requests. The argument-free
variant also handles unchanged cyclic storage helpers. For a single memory
bytes result, the checker compares the pointer arriving at the implicit
encoder with the pointer used by the lowered scratch encode and its two
return projections. The encoder and exit are shared continuations.

The added-helper module row composes entry certificates only when every
constructor, selector, receive and fallback root is checked. The kernel
checks the source and target root tables and coverage of the declared roots.
The audit driver links entry reports by capture hashes, target fork and
checker fingerprint, and records a hash of each linked report. The composed
certificate carries the premises of its entry proofs. The regression also
checks that omitting an entry certificate leaves the module unsupported.

The strict serial `evaluate --all --require-verified --pass lower-abi
--timeout 120 --seed 0` run checked the same 65 capture IDs under
`target/tv-lower-abi-composed-roots/`.

| Contract | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: |
| Solady WETH | 17 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 13 | 0 | 0 | 0 |
| Morpho Blue | 20 | 15 | 0 | 0 |
| Total | 50 | 15 | 0 | 0 |

The checker fingerprint was
`4a014f71e67dce434dba5d6344e2a541b40810e0e409f1d2c9d2f70d6baf9471`.
Compiler, independent replay, input, output and selected capture hashes
match the earlier targeted audit. ABI, creation bytecode and runtime
bytecode match for normal and capture compilations of each contract.
The gate exited with status 1 because 15 captures remained unsupported.
The direct checker first reports the `GAS` boundary for 13 of these.
The other two are a cyclic word-array entry and the Morpho module row.

### Aggregate MIR ABI composition

The runtime adapter checks flat memory tuples, static calldata tuples and
dynamic calldata bytes. It compares the ordered tuple allocation and
stores, the dynamic offset and length checks, and the resulting typed
arguments. Scratch encodings retain their layout and operands, including
the pointer and length projections of dynamic encodings. The projection
proofs also relate defined MIR product fields to the static return words.

For a larger wrapper, the kernel checks unchanged closed callee tables
and proves rewritten callees independently before reusing their call
observations. The entry certificate compares the order and arguments of
those calls without expanding their bodies into the caller's proof. The
composed certificates carry the contextual state and gas premises.

For the word-array entry, the kernel compares the actual decoder with the
implicit bounds and slice adapter. The logical CFG simulation then checks
the loop and its live values at every work bound. Both sides use the same
word-array return encoder. Changing the array bounds in the regression
causes the entry to lose its certificate.

The strict serial run completed with exit status 0 under
`target/tv-lower-abi-complete/`. It used `--all --require-verified --pass
lower-abi --timeout 120 --seed 0` and checked the same 65 capture IDs.

| Contract | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: |
| Solady WETH | 17 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 13 | 0 | 0 | 0 |
| Morpho Blue | 35 | 0 | 0 | 0 |
| Total | 65 | 0 | 0 | 0 |

Every verified result carries kernel evidence. The three module rows
compose 16, 12 and 29 entry certificates respectively. The audit checked
the recorded hashes of the captures and linked proofs. Normal and
capture-enabled compiler outputs agree on ABI, creation bytecode and
runtime bytecode for each contract. The input and output fingerprints
match the full corpus reports.

| Artifact | SHA-256 |
| --- | --- |
| Compiler | `64f563d6e8b2102f849dc69cb5fbea91179ce4a7c765c8205217da7c0d7c4b76` |
| Rust replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Checker on Lean 4.34.1 | `97ef03f3821ac6a1a96e78236bc4e91ee2eebafc48825f540bd0579cb64253e8` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Solady WETH input | `b751f0da2642fba80d176248ff576b06d45601fda253d9de32f9a78e3b70a4c0` |
| OpenZeppelin ERC20Mock input | `c85c24c264a2ae9a97b32050f93426376c8cb20e15f6e5ad5dc1414ee46ce2c4` |
| Morpho Blue input | `01f4aaf9db4940ea9a62aba808ba1655ef399c671b6ed29875f8a91c3efeba53` |

Independent Rust replay agrees with the changed-return regression's
counterexample. The proof boundary still includes the typed MIR importer
and the entry-specific state and gas premises. Exact physical gas and
correspondence from these MIR proofs to emitted bytecode remain open.

### Full corpus after ABI entry coverage

A serial `evaluate --all --timeout 3 --seed 0` run at `b6218f55e5`
checked every capture under `target/tv-full-post-abi/`. All 2,228 capture
IDs and the input and output fingerprints match the earlier full audit.
ABI, creation bytecode and runtime bytecode also match between the normal
and capture compilations. The three-second budget is the same as in that
full audit. The targeted ABI and dispatch gates use a larger budget.

| Artifact | SHA-256 |
| --- | --- |
| Compiler | `64f563d6e8b2102f849dc69cb5fbea91179ce4a7c765c8205217da7c0d7c4b76` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Checker on Lean 4.34.1 | `97ef03f3821ac6a1a96e78236bc4e91ee2eebafc48825f540bd0579cb64253e8` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Complete results.json | `965ca9cbd071c8cd71e4891fcbfd43b4421fa61cce93d9dfacc7d829027fcf0e` |

| Contract | Captured | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 330 | 186 | 99 | 44 | 1 |
| OpenZeppelin ERC20Mock | 372 | 213 | 71 | 88 | 0 |
| Morpho Blue | 1,526 | 660 | 674 | 192 | 0 |
| Total | 2,228 | 1,059 | 844 | 324 | 1 |

| Layer and pass | Captured | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| evmir `block-cse` | 10 | 3 | 5 | 2 | 0 |
| evmir `block-layout` | 368 | 299 | 68 | 1 | 0 |
| evmir `cfg-simplify` | 7 | 5 | 2 | 0 | 0 |
| evmir `cfg-simplify-early` | 291 | 132 | 151 | 8 | 0 |
| evmir `compact-pushes` | 118 | 73 | 44 | 1 | 0 |
| evmir `copy-loop-blocks` | 2 | 2 | 0 | 0 | 0 |
| evmir `dce` | 2 | 0 | 0 | 2 | 0 |
| evmir `inline-returns` | 14 | 7 | 6 | 1 | 0 |
| evmir `late-word` | 19 | 0 | 18 | 1 | 0 |
| evmir `loop-layout` | 29 | 27 | 2 | 0 | 0 |
| evmir `outline` | 7 | 1 | 6 | 0 | 0 |
| evmir `peephole-early` | 35 | 11 | 24 | 0 | 0 |
| evmir `reorder-expressions` | 1 | 1 | 0 | 0 | 0 |
| evmir `reorder-pushes` | 5 | 0 | 5 | 0 | 0 |
| evmir `stack-normalize` | 5 | 0 | 5 | 0 | 0 |
| evmir `stack-reschedule` | 17 | 0 | 17 | 0 | 0 |
| evmir `tail-merge` | 49 | 45 | 3 | 1 | 0 |
| evmir `terminal-dedup` | 124 | 106 | 18 | 0 | 0 |
| mir `byte-run-loads` | 14 | 4 | 8 | 2 | 0 |
| mir `call-cleanup` | 1 | 0 | 0 | 1 | 0 |
| mir `cfg-simplify` | 144 | 121 | 0 | 23 | 0 |
| mir `check-elim` | 7 | 2 | 4 | 1 | 0 |
| mir `coalesce-allocs` | 1 | 1 | 0 | 0 | 0 |
| mir `cse` | 48 | 25 | 21 | 2 | 0 |
| mir `dce` | 86 | 13 | 53 | 20 | 0 |
| mir `dead-arg-elim` | 11 | 0 | 11 | 0 | 0 |
| mir `egraph` | 116 | 35 | 58 | 23 | 0 |
| mir `evm-inst-schedule` | 32 | 0 | 13 | 18 | 1 |
| mir `function-dce` | 3 | 0 | 0 | 3 | 0 |
| mir `indvar-simplify` | 2 | 0 | 2 | 0 | 0 |
| mir `inline` | 58 | 2 | 52 | 4 | 0 |
| mir `integer-cleanup` | 35 | 0 | 19 | 16 | 0 |
| mir `jump-threading` | 52 | 41 | 6 | 5 | 0 |
| mir `licm` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-abi` | 65 | 57 | 2 | 6 | 0 |
| mir `lower-abi-encode` | 16 | 0 | 16 | 0 | 0 |
| mir `lower-alloc` | 19 | 2 | 16 | 1 | 0 |
| mir `lower-arithmetic` | 16 | 0 | 11 | 5 | 0 |
| mir `lower-builtins` | 6 | 3 | 3 | 0 | 0 |
| mir `lower-checks` | 31 | 0 | 24 | 7 | 0 |
| mir `lower-dispatch` | 73 | 0 | 0 | 73 | 0 |
| mir `lower-evm-shaped` | 3 | 2 | 0 | 1 | 0 |
| mir `lower-immutables` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-integers` | 55 | 0 | 33 | 22 | 0 |
| mir `lower-mapping-slots` | 32 | 0 | 17 | 15 | 0 |
| mir `lower-memory-objects` | 27 | 1 | 26 | 0 | 0 |
| mir `lower-memory-zero` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-slices` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-structs` | 1 | 0 | 0 | 1 | 0 |
| mir `memory-dse` | 10 | 0 | 9 | 1 | 0 |
| mir `memory-dse-unread` | 1 | 0 | 1 | 0 | 0 |
| mir `merge-aborts` | 42 | 7 | 18 | 17 | 0 |
| mir `place-reads` | 33 | 0 | 16 | 17 | 0 |
| mir `sccp` | 12 | 7 | 2 | 3 | 0 |
| mir `simplify-phis` | 1 | 0 | 1 | 0 | 0 |
| mir `specialize` | 1 | 0 | 0 | 1 | 0 |
| mir `spell-width-tests` | 38 | 1 | 19 | 18 | 0 |
| mir `storage-dse` | 10 | 10 | 0 | 0 | 0 |
| mir `storage-promotion` | 12 | 12 | 0 | 0 | 0 |
| mir `word-sequence` | 6 | 1 | 4 | 1 | 0 |
| mir `word-sequence-early` | 1 | 0 | 1 | 0 | 0 |

The original report preserves one OS permission error on WETH's
`evm-inst-schedule` capture for `DOMAIN_SEPARATOR`. A separate retry of
the same pair at three seconds timed out. There were zero counterexamples.
The full run verified 76 more captures than the previous full audit.
All 16 `lower-abi-encode` captures were unsupported at this checkpoint.

### Strict ABI encoder audit

The serial gate `evaluate --all --require-verified --pass lower-abi-encode
--timeout 120 --seed 0` verified all 16 captures. The check IDs, before and
after capture hashes, and compiler input and output fingerprints match the
full audit above. Normal and validating compilations match on ABI and
creation and runtime bytecode for all three contracts.

The same strict command after the DCE update again verified 16 of 16 with
the same capture IDs, raw hashes and compiler inputs and outputs.
`target/tv-lower-abi-encode-current/results.json` has SHA-256
`f9f513d0b8b7f79d1b88bdcd11de32ab7674f4145bfb5955ec5c4eca82ae2ad4`.
The checker fingerprint is
`dd1a9d4018c2f81eaa651d974e91d04f7149b95921bac94cc29e32fbfd346b90`.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 0 | 0 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 2 | 2 | 0 | 0 | 0 |
| Morpho Blue | 14 | 14 | 0 | 0 | 0 |
| Total | 16 | 16 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Compiler | `64f563d6e8b2102f849dc69cb5fbea91179ce4a7c765c8205217da7c0d7c4b76` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Checker on Lean 4.34.1 | `3c067323175dbddd52323f07be4f388587a769ad9a1bed7039451a07b9adfa95` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| `target/tv-lower-abi-encode-verified/results.json` | `5b3111737159e4ec801692944c967aa1fa5f2ece0a5ffe59546ac5ebfd7a5371` |

Thirteen checks compare ordered effects in the selected typed MIR function
and its direct call closure. The kernel checks the allocation or scratch
pointer, selector stores, static words and loaded tuple fields, and the
head, padding, length and calldata copy of the dynamic callback. Equal
requests preserve the responses and failures of a shared handler. The proof
checks rewritten encoder helpers separately and binds unchanged closed
callee bodies on both sides.

The other three checks compare terminal returndata for two byte strings and
one word array. The kernel checks the incoming control flow and its effects
at every logical work bound. It interprets the actual lowered return suffix
against the copying encoder's byte model, including offsets, length, overlap
and padding. The cut requires the same journal, a live length header, an
object with room for its preceding word, and bounded memory with disjoint
scratch. The final frame discards its private memory.

Independent Rust replay confirms the normal callback and dirty-padding
examples agree across lowering. Moving the callback's calldata source and
changing the zero-copy padding store both lose verification and produce
matching Lean and Rust counterexamples. The automatic proofs rely on the
typed importer and the stated semantic adapters. Exact physical gas and
correspondence to the emitted instructions remain open.

### Strict integer lowering audit

The serial gate `evaluate --all --require-verified --pass lower-integers
--timeout 120 --seed 0` verified all 55 captures within 6.2 seconds each.
Every capture ID and before and after hash matches the full audit above.
The compiler, replay executable, manifest and compiler input and output
fingerprints also match. Normal and validating compilations produced the
same ABI and creation and runtime bytecode.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 10 | 10 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 11 | 11 | 0 | 0 | 0 |
| Morpho Blue | 34 | 34 | 0 | 0 | 0 |
| Total | 55 | 55 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Checker on Lean 4.34.1 | `3bd6acfc3e3fffb7f86e876ed5ecaf9641957760b4904f335eb8e3f1b9794848` |
| `target/tv-lower-integers-verified/results.json` | `e48b8f13e3554c03ab164af87f8299dca34024d3ddabbd7576e1e280368d5302` |

The kernel checks contextual effect equality after zero-extending each
narrow source argument into its corresponding target word. At normal
function returns, the target's entire word is compared with the widened
source value. The fixed-width mask lemma covers every input word, and the
one-bit lemma preserves the normalized branch predicate. Address reads,
the heap floor and ordered message and GAS actions use the same logical
context on each side.

Forty-nine certificates expand the acyclic call closure. The other six
check every body in that closure separately. The callers compare call IDs,
full argument words and returned values at the related signatures. A
mutated result mask in a widened callee loses verification and independent
Rust replay agrees with Lean on the resulting return difference.
The proof uses the typed importer and the stated entry relation. Physical
frame placement and instruction gas remain separate obligations.

### Strict storage slot lowering audit

The serial gate `evaluate --all --require-verified --pass lower-mapping-slots
--timeout 120 --seed 0` verified all 32 captures. The capture IDs, before and
after hashes and compiler input and output fingerprints match the full audit
above. The compiler, Rust replay and manifest fingerprints also match.
Normal and validating compilations produced identical ABI and bytecode.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 0 | 0 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 7 | 7 | 0 | 0 | 0 |
| Morpho Blue | 25 | 25 | 0 | 0 | 0 |
| Total | 32 | 32 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Checker on Lean 4.34.1 | `0b824cd105d9b84b3d835995b3bb5395e541ced997d324f15e3b2949154b41ae` |
| `target/tv-lower-mapping-slots-verified/results.json` | `db9c716daef075a7f874809f134bbe85425758a358d23078fd0986770efd8280` |

Lean relates the semantic array data slot and fixed-width mapping slot to
their one or two scratch stores followed by Keccak. The theorem preserves
the hash input, the returned slot, active memory extent, scratch bytes and
the rest of the frame for arbitrary aligned byte memory within 64 KiB.
It holds for an arbitrary shared hash function. The graph certificate
checks the expanded source against the imported target operations, including
write addresses, sizes and live operands through loops. Eighteen checks
compose direct calls using the checked callee graphs. The work bound counts
logical actions after expanding the source hashing operations.

Independent Rust replay agrees with Lean after cutting a two-word hash down
to one word. The modified pair loses the automatic proof. The certificate
uses the typed importer and the memory-domain premises above. Dynamic byte
keys, physical gas and bytecode placement remain outside this audit.

### Strict memory object lowering audit

The serial gate `evaluate --all --require-verified --pass lower-memory-objects
--timeout 120 --seed 0` verified all 27 captures. The check IDs and before and
after IR hashes match the full audit. All three contracts produced identical
ABI and creation and runtime bytecode with and without validation.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 3 | 3 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 5 | 5 | 0 | 0 | 0 |
| Morpho Blue | 19 | 19 | 0 | 0 | 0 |
| Total | 27 | 27 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Checker on Lean 4.34.1 | `22f58cb27f23dbc4caeeeaa43f0972c6e43e15cfa8a9e9529a525fe34e6d13ec` |
| `target/tv-lower-memory-objects-complete/results.json` | `9b6f3b8f0e97d535050fcf66aae18d7903a1c180c4eb324544c914d81fc52fa6` |

The kernel checks that full-word pointer addition and the explicit word
addition compute the same address. The carrier proofs check casts and
chained offsets through every path, including loops with simultaneous phi
inputs. Both sides preserve the subsequent memory requests, allocation
requests, state effects and control transfers under shared handlers.
The certificate checks the selected function and separate reachable bodies
at their existing typed interfaces. Physical pointer validity, stack
placement and gas remain separate obligations. The compiler's return-memory
annotation is excluded from this MIR execution projection.

Independent Rust replay agrees with Lean on a different returned word
after changing a load's offset. The mutated pair loses the automatic
proof. The fixtures keep both access ranges inside the same active memory.

The combined strict gate for ABI encoding, integer lowering, storage slots
and memory objects verifies 130 of 130 captures with the same budget. All
four pass counts and original capture hashes match their individual gates.
The combined run records 13 WETH, 25 ERC20Mock and 92 Morpho Blue proofs.
There are zero unsupported, timed out or failed checks. The compiler and
independent replay hashes match the full audit above.

| Combined gate artifact | SHA-256 |
| --- | --- |
| Checker on Lean 4.34.1 | `9fc27e920f57d5f4e64caa76ece59c3b0ca68396f91712adb0322b732c2a76f4` |
| `target/tv-four-mir-gates-final/results.json` | `cc3e6f1f89b5e5427d213254ec03b89c71845787b422c5ddcda1289bbe8335df` |

### Strict checked arithmetic and slice audit

The combined serial gate verifies all 147 captures for six MIR passes
at 120 seconds per prover invocation. Capture IDs and before and after
hashes match the full audit. All three contracts produce identical ABI
and creation and runtime bytecode with and without validation. The
compiler, replay executable and corpus manifest fingerprints match the
full audit above.

| Pass | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| `lower-abi-encode` | 16 | 16 | 0 | 0 | 0 |
| `lower-arithmetic` | 16 | 16 | 0 | 0 | 0 |
| `lower-integers` | 55 | 55 | 0 | 0 | 0 |
| `lower-mapping-slots` | 32 | 32 | 0 | 0 | 0 |
| `lower-memory-objects` | 27 | 27 | 0 | 0 | 0 |
| `lower-slices` | 1 | 1 | 0 | 0 | 0 |
| Total | 147 | 147 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Checker on Lean 4.34.1 | `17166f34154c43450e929b514beed14f6749d2bc7a4b6670151f5168c3855856` |
| `target/tv-six-mir-gates-final/results.json` | `170db1919b90d505f2a5aa128650d2f9aa166f3b0459d5fb6b9a07265eac92fb` |

The arithmetic proof covers unsigned addition and subtraction at 128
and 256 bits and unsigned multiplication and division at 256 bits.
The kernel checks the overflow and zero tests against the semantic
operations, including the quotient test after a wrapped multiplication.
The panic staging proof preserves exact failure bytes for codes 0x11
and 0x12 in aligned byte memory within the 64 KiB model. The graph
certificate compares ordered actions, failure branches and callee
bindings after expanding the source operations.

The slice proof checks that projecting either field of a defined typed
construction yields its original operand. The ERC20Mock storage path
retains those values across loops, shared blocks and direct calls.
Independent Rust replay agrees with Lean on a changed arithmetic
panic code and a changed value after slice projection. Both regressions
lose the automatic proof. These checks use the trusted typed importer
and action binding. Physical execution cost and backend placement
remain separate obligations.

### Strict conditional check audit

The serial gate `evaluate --all --require-verified --pass lower-checks
--timeout 120 --seed 0` verifies all 31 pinned captures. Raw capture IDs,
before and after MIR hashes, compiler and replay binaries, manifest and
contract input and output fingerprints match the full audit. Normal and
capture compilations match on ABI and creation and runtime bytecode.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 0 | 0 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 6 | 6 | 0 | 0 | 0 |
| Morpho Blue | 25 | 25 | 0 | 0 | 0 |
| Total | 31 | 31 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Checker on Lean 4.34.1 | `b34c97623e605bce13e0711317efce9f57be5b0107d6aed58a5e5b4492e84c8e` |
| `target/tv-lower-checks-verified/results.json` | `de06f1b47bb14dfe2921e0ea6d85bebc35e1d52920504c0d8bff07597ce8b761` |

[LowerChecks.lean](SolarTV/MIR/Transforms/LowerChecks.lean) checks the
passing branch and the exact bytes of empty reverts, panic `0x41`,
short `Error(string)` payloads and custom errors with one or three
static word fields. The custom-error proof compares the direct encoder
at the free-memory pointer with the lowered stores at scratch addresses.
The source cursor and payload extent must fit the 64 KiB memory model.
Failure discards private frame memory. The checks use defined operands
and the compiler's default revert-string mode.

The kernel compares the normalized graphs and composes the direct
calls, including the module capture that adds the private `revert_error`
helper. The helper must have the checked body and interface, and the
independent typed module export must retain the same external roots.
The importer, source guard expansion and action binding remain trusted.
The proof compares unpriced MIR behavior under shared effect handlers.

Independent Rust replay agrees with Lean before and after lowering.
Changing the short-string helper's length store or a custom-error word
loses the automatic proof and produces a replayed revert-data difference.

The combined serial run repeats the six MIR gates above and adds
`lower-checks`. All 178 captures verify at the same 120-second budget,
including 16 of 16 ABI encoder captures. It checks 13 Solady WETH, 33
ERC20Mock and 132 Morpho Blue captures. The checker and input fingerprints
match this audit, and the combined capture IDs and MIR hashes match the
full audit. The SHA-256 of `target/tv-seven-mir-gates-final/results.json`
is `7fe555908533a7fd3c937931daa51008ae9cd65345fee7a105dffa0e32542de1`.

The live `-Zsolar-tv` hook also routes the added helper to the module
checker. The MIR hook regression produces two kernel-verified records
and identical MIR output. A separate Solidity run for `Check.checked`
produces kernel-verified function and module records and preserves ABI
and creation and runtime bytecode. Its compiler, input and output
fingerprints are saved in
`target/tv-lower-checks-dev/live-contract/summary.json`.

### Strict memory-zero audit

The serial `lower-memory-zero` gate verifies the sole pinned capture in
Morpho Blue's `extSloads`. The capture ID, before and after MIR hashes,
and all three contracts' input and output fingerprints match the full
audit. Normal and capture compilations match on ABI and both bytecode
outputs with the rebuilt compiler.

| Artifact | SHA-256 |
| --- | --- |
| Compiler | `b9ff216a511be165ff8187ca3469d741f087977f59d71d6745f7d0494e26e753` |
| Checker on Lean 4.34.1 | `474d2ab7a1236e5397f27490d9216b640647446a5620a888ba72d5125a228fba` |
| `target/tv-lower-memory-zero-verified/results.json` | `289fbbc69e0c5108edec620ebc3d2a6040219e72d2721bb82f56379f55ee5838` |

The kernel checks that copying from `CALLDATASIZE` writes the requested
number of zeros and preserves the memory expansion and bytes outside
the destination. The calldata length must fit the word representation.
It also checks the 32-byte fast path using a zero word store. The typed
CFG certificate binds the full-width destination carrier, count and
live values through the surrounding loop. The importer and unpriced
effect handlers retain the same scope as the other MIR graph proofs.
Copying from offset zero instead produces a matching Lean and Rust
counterexample with nonzero calldata.

### Strict allocation lowering audit

The serial gate at 120 seconds per prover invocation verifies **19 of 19**
`lower-alloc` captures. The same run verifies all 31 `lower-checks`, 16
`lower-abi-encode`, and one `lower-memory-zero` capture, for 67 of 67.
Capture IDs and before and after MIR hashes match the full audit. The
normal and capture compilations match on ABI and both bytecode outputs,
and the three contracts' input and output fingerprints match that audit.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 1 | 1 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 2 | 2 | 0 | 0 | 0 |
| Morpho Blue | 16 | 16 | 0 | 0 | 0 |
| Total for `lower-alloc` | 19 | 19 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Compiler | `bc8f77555ef879823f0560ec38bd0faf2791940ad0a213be36edfe491996e49b` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Checker on Lean 4.34.1 | `41d31b1a81f24705c85ff982ba630c473c7628541ef9fa598dc372b08f202bc9` |
| `target/tv-lower-alloc-final/results.json` | `3f0181b00233a5bc2ef72a405fbb76f1713e9c7a66eed5b935649c00e58a5ce1` |

The kernel checks the FMP slot read, wrapping and uint64 limit tests,
successful cursor store, optional zero fill and exact `Panic(0x41)` bytes.
The shared checked allocation helper has a direct interpreter certificate
for its returned pointer and shared memory, including the one-result return
convention. The module certificate covers 48 functions with the same public
bindings and composes the procedure table after checking it in nine serial
Lean modules. The live compiler also checks the added helper. The prior
allocation-prefix, fresh-heap and effect-tree cases retain their certificates.

The graph proof uses aligned memory within the 64 KiB model. An infallible
reservation requires a valid source cursor bump. A substituted heap floor
requires the named placement to equal the live FMP in an active slot at
the allocation. Creation and dispatch activations are excluded from that
substitution. The typed exporter, source expansion and action binding
remain trusted. Establishing the heap-floor premise from the ABI entry
and emitted layout and pricing the physical execution are separate tasks.

The 42 focused tests exercise the live checker, retain the rejected
heap-floor substitutions and replay the uint64-limit and wrapping failures.
Changing the shared helper's panic code loses the proof and produces
matching Lean and Rust revert-data counterexamples in both failure cases.

### Strict builtin lowering audit

The serial gate at 120 seconds per prover invocation verifies all six
`lower-builtins` captures and rechecks all 16 `lower-abi-encode` captures.
Each check uses the capture ID and before and after MIR hashes from the
full audit. The three pinned inputs and compiler output fingerprints match
that audit. Normal and capture compilations produce identical ABI and
creation and runtime bytecode for each contract.

| Pass | Solady WETH | OpenZeppelin ERC20Mock | Morpho Blue | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `lower-builtins` | 0/0 | 3/3 | 3/3 | 6 | 0 | 0 | 0 |
| `lower-abi-encode` | 0/0 | 2/2 | 14/14 | 16 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Compiler executable | `7d123ab7483266256ead8e4b3d86cbce427376f186b5714969110d19139335e8` |
| Serial compiler launcher | `4e5a4f6faa73f5d017a534afaa4d3f9a8c052abe1c2435f192108577b79239e9` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Checker on Lean 4.34.1 | `c46f7750f47a89de784bdecbb707853a4b3b540143a6a256c5097652c4f5cdaa` |
| `target/tv-lower-builtins-final/results.json` | `5161da37e0fbb53d945f5e1b249e21ad7002994f9f9888926d40df093d419763` |

The storage-byte load theorem checks arbitrary headers, the rounded extent
and its overflow guard, checked allocation, short payload masking, and the
long loop's unsigned guard, index advance and memory offsets. It preserves
the returned pointer, live frame and warm slots on success, and checks the
exact malformed-header panic bytes. The module certificate compares the
resolved caller table and both replaced function signatures. The two
replacement equations compose in any shared context using those logical
frame effects. The only added function is the private clear helper, whose
body and call binding match the writer expansion. The live hook checks
that module path and preserves emitted MIR.

The linked certificate uses unpriced MIR effects, at most 64 KiB of active
memory and 2048 words per storage loop, and sufficient logical work and call
depth. It trusts the typed exporter, constructor renderer and association
between the exact SSA shapes and their operational projections. A changed
phi initializer, loop stride, panic code or retained caller binding loses
this proof. Independent Rust replay and the kernel agree on the altered
stride's returned word and memory counterexample. The Morpho checks cover
the two local builtin rewrites and the priced emitted CALL continuation,
at its reported boundary.

### Pinned egraph scalar reuse audit

Two serial runs of `evaluate --all --pass egraph --timeout 10 --seed 0`
checked the same 116 MIR captures. The capture IDs and both source hashes
match the full audit above. The compiler and all three input fingerprints
match between runs. Normal and capture compilations produce identical ABI
and creation and runtime bytecode for each contract.

| Contract | Selected | Baseline verified | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 16 | 5 | 11 | 4 | 1 | 0 |
| OpenZeppelin ERC20Mock | 15 | 7 | 15 | 0 | 0 | 0 |
| Morpho Blue | 85 | 25 | 49 | 33 | 3 | 0 |
| Total | 116 | 37 | 75 | 37 | 4 | 0 |

The baseline had 72 unsupported and seven timed-out checks. Each of its
37 verified checks still verifies. The 38 new checks all use the scalar
reuse certificate.

| Artifact | SHA-256 |
| --- | --- |
| Compiler executable | `7d123ab7483266256ead8e4b3d86cbce427376f186b5714969110d19139335e8` |
| Serial compiler launcher | `4e5a4f6faa73f5d017a534afaa4d3f9a8c052abe1c2435f192108577b79239e9` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Checker on Lean 4.34.1 | `56498749949e5d9ecb65329db5ddfc0a3e720c85fffe21c522ed209131d58556` |
| `target/tv-egraph-baseline/results.json` | `25169a148dce391a0e37a04e60e3240a6bd7736627b73ef011e0e75242d37cdf` |
| `target/tv-egraph-scalar-reuse/results.json` | `232ef90e9b0962722019b2c06409ba4eb755e70e6e1bf90c060fa36bccb7dbf7` |

The kernel checks the cast, mask, shift and predicate identities, plus
edge-closure witnesses showing that a reused definition dominates its
cross-block use. The graph certificate checks ordered actions and live
operands after normalizing total scalar definitions and uniform phis.
CALLER is fixed per activation. GAS, message calls and mutable state
operations retain their requests and their shared responses. The checker
trusts the validated SSA export and the bindings from the original
instructions to the scalar and action projections. It makes a logical
MIR claim under defined typed values and shared handlers.

The regression compiles a real `egraph` rewrite with cross-block CALLER
reuse and a live GAS sample. Replacing that sample with a constant loses
the certificate. Using the same fixed reply on both sides lets Lean and
the independent Rust interpreter replay a changed storage slot. The 32
focused tests also cover CFG, dead SSA and builtin lowering.

The remaining captures include rewritten call operands, removed
returndata extent checks and additional scalar range and predicate
obligations. Pricing the GAS sample and binding those effects to the
emitted instructions remain open.

### Complete pinned egraph audit

The serial gate `evaluate --all --require-verified --pass egraph --timeout 15
--seed 0` verifies all 116 MIR captures. The capture IDs, both MIR hashes,
and compiler input and output fingerprints match the preceding egraph audit
and the full audit. Normal and capture runs again produce identical ABI,
creation bytecode, and runtime bytecode.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 16 | 16 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 15 | 15 | 0 | 0 | 0 |
| Morpho Blue | 85 | 85 | 0 | 0 | 0 |
| Total | 116 | 116 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Compiler executable | `7d123ab7483266256ead8e4b3d86cbce427376f186b5714969110d19139335e8` |
| Serial compiler launcher | `4e5a4f6faa73f5d017a534afaa4d3f9a8c052abe1c2435f192108577b79239e9` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Solady WETH input | `b751f0da2642fba80d176248ff576b06d45601fda253d9de32f9a78e3b70a4c0` |
| OpenZeppelin ERC20Mock input | `c85c24c264a2ae9a97b32050f93426376c8cb20e15f6e5ad5dc1414ee46ce2c4` |
| Morpho Blue input | `01f4aaf9db4940ea9a62aba808ba1655ef399c671b6ed29875f8a91c3efeba53` |
| Checker on Lean 4.34.1 | `a2a4c7597f7b1dea8c6481dc9b29e0c24ab8fdfb6bd7f2892f8b80684d98974e` |
| `target/tv-egraph-final/results.json` | `ebb6ffee0d7978a8a7d108b8559971d667ecba4d63d15304cda4cac91391796e` |

The kernel checked 105 typed control-flow certificates, five ABI-context
effect proofs, three compositional effect trees, and three universal
caller-state proofs. The control-flow certificates compose normalized
predicate, slice, offset and packed-word identities with the retained
actions. Six larger Morpho procedure tables are checked in serial Lean
modules.

The raw message proof preserves `CALL`, `CALLCODE`, `DELEGATECALL` and
`STATICCALL` behavior when a destination is replaced by its low 160 bits
or an offset is erased from an empty memory range. Ten captures use the
target-width projection, and WETH `withdraw` uses both empty range
projections. The `CODESIZE` value can be discarded when the code image is
defined and the value is dead. Nine captures check removed returndata
extent guards under a live return-buffer length below `2^255`.

The compiler parse, SSA export and source-to-action projection remain
trusted. The logical MIR certificates retain the order of GAS and child
effects under shared replies. The local message lemma compares the
modeled budget and child execution for the same supplied GAS sample.
Physical GAS correspondence before the sample, ABI entry and backend
mapping remain separate obligations.

A live `egraph` regression confirms the typed operand and return-extent
rewrite. Giving the output range a nonzero size or changing the rounding
constant loses the certificate. With a fixed GAS reply, the independent
Rust interpreter confirms equal observations on the compiler pair for a
target word with dirty high bits. The nonempty output mutation changes
the observed memory.

### Complete pinned MIR DCE audit

The serial gate `evaluate --all --pass dce --pass egraph --timeout 15
--seed 0` verifies every MIR DCE capture and rechecks all 116 MIR egraph
captures. Both groups match the full audit's capture IDs, raw before and after
hashes, and compiler input and output fingerprints. Normal and capture runs
agree on ABI, creation bytecode, and runtime bytecode.

| Contract | MIR DCE | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 25 | 25 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 5 | 5 | 0 | 0 | 0 |
| Morpho Blue | 56 | 56 | 0 | 0 | 0 |
| Total | 86 | 86 | 0 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Compiler executable | `7d123ab7483266256ead8e4b3d86cbce427376f186b5714969110d19139335e8` |
| Serial compiler launcher | `4e5a4f6faa73f5d017a534afaa4d3f9a8c052abe1c2435f192108577b79239e9` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Solady WETH input | `b751f0da2642fba80d176248ff576b06d45601fda253d9de32f9a78e3b70a4c0` |
| OpenZeppelin ERC20Mock input | `c85c24c264a2ae9a97b32050f93426376c8cb20e15f6e5ad5dc1414ee46ce2c4` |
| Morpho Blue input | `01f4aaf9db4940ea9a62aba808ba1655ef399c671b6ed29875f8a91c3efeba53` |
| Checker at corpus run on Lean 4.34.1 | `6024c1ca8ae810802d75cd4b9f5e6c10731b4213b5d9b689637adc237f6ee4e1` |
| `target/tv-dce-final/results.json` | `74974dffb0c36828faeac8f15d76f9cbd4608ff8717cffb8101817fc0942aa9d` |

The kernel checked 68 typed control-flow certificates, five slice projection
proofs, four compositional effect trees, four universal caller-state proofs,
four cyclic dead SSA simulations, and one ABI-context effect proof. Typed
calldata word reads and size use the immutable input of the activation.
Product insertion only updates its typed result, including construction
starting from an undefined product. The two WETH hook pairs use identical
void bodies that return at entry. With defined typed arguments and sufficient
depth and logical work, the checked callee steps restore the exact caller
frame. The structural importer binds the operands and the callee bodies.

For Morpho Blue's `createMarket`, the unique predecessor ends in a raw CALL
with a 32-byte output at offset zero. Lean checks that any continuing CALL
activates the output word, including when the child supplies fewer bytes.
The retained returndata size check can still revert. In aligned memory up
to 64 KiB the dead load cannot expand memory. The Rust replay regression
observes `MSIZE = 32` with or without the load when a child returns no bytes.
Setting the call's output size to zero removes the certificate and produces
different replay results, 32 before DCE and 0 after it.

These action proofs use defined SSA inputs and a trusted typed projection.
Physical gas and the connection to executable ABI and backend state remain
separate obligations. After the corpus run, the checker added the calldata,
empty-call and active-memory premises to its audit records and formatted
the Python sources. Serial rechecks of `permit`, `withdraw`, and
`createMarket` produced the same proof correspondence with these premises
recorded. Those rechecks used the checker fingerprint
`dd1a9d4018c2f81eaa651d974e91d04f7149b95921bac94cc29e32fbfd346b90`.

### Pinned EVM IR DCE audit

The same serial run selected two EVM IR DCE captures. Lean checked WETH's
terminal deletion of two POPs at a proved minimum incoming height of three.
Morpho Blue's corresponding three-POP deletion timed out at 15 seconds.
A serial retry at 120 seconds per Lean invocation checked the exact same
capture hashes. Lean proved that at least four words reach the cut and that
both suffixes reach STOP with equal modeled observations in the unpriced
scope. Across the two checks and the retry, both captures verified.

The retry report `target/tv-dce-final/morpho-stop-retry.json` has SHA-256
`c33cdd542ea4e028abaa9248d8915379fe6868b6a1935a661633c2a7437efd46`.
It uses the checker fingerprint
`64a3310b4680b56db7ca88296dc726e6f076ea708d12339287e07d7b1954e734`.

### Pinned MIR CSE audit

The serial gate `evaluate --all --pass cse --timeout 15 --seed 0` checked
all 48 pinned MIR captures before and after the state-read extension.
Both runs matched the full audit's capture IDs, raw before and after hashes,
and input and output fingerprints. Normal and capture runs agreed on
ABI, creation bytecode and runtime bytecode.

| Contract | Selected | Baseline verified | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 4 | 4 | 4 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 6 | 5 | 6 | 0 | 0 | 0 |
| Morpho Blue | 38 | 17 | 35 | 3 | 0 | 0 |
| Total | 48 | 26 | 45 | 3 | 0 | 0 |

| Artifact | SHA-256 |
| --- | --- |
| Compiler executable | `7d123ab7483266256ead8e4b3d86cbce427376f186b5714969110d19139335e8` |
| Serial compiler launcher | `4e5a4f6faa73f5d017a534afaa4d3f9a8c052abe1c2435f192108577b79239e9` |
| Independent replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Solady WETH input | `b751f0da2642fba80d176248ff576b06d45601fda253d9de32f9a78e3b70a4c0` |
| OpenZeppelin ERC20Mock input | `c85c24c264a2ae9a97b32050f93426376c8cb20e15f6e5ad5dc1414ee46ce2c4` |
| Morpho Blue input | `01f4aaf9db4940ea9a62aba808ba1655ef399c671b6ed29875f8a91c3efeba53` |
| Checker on Lean 4.34.1 | `f538017c18693711f3bb143a6777dd716450ba829f207c9361c341139f0022b2` |
| `target/tv-cse-baseline/results.json` | `cae63a664f47630c018c00d577881979b22968c6368de67c30a620312fc429ce` |
| `target/tv-cse-verified/results.json` | `0c0bd7726a0e9af4cddde241daf2450cf3406f981393f691de3b51d5a2d7e34f` |

For each reused read, the kernel checks that every incoming path in the
instruction graph passes the producer and then preserves the fact. The
certificate checks backward closure through diamonds and backedges.
Semantic lemmas establish active memory words after successful MLOAD or
MSTORE, preserve their value across expansion and disjoint byte writes,
and prove that rereading a warm storage slot keeps the world unchanged.
Words at distinct offsets from the same modular storage base are distinct.
Repeated mapping computation uses identical hash input and the same
scratch bytes at offset zero.

Of the 19 newly verified captures, 14 use storage read reuse, 13 use mapping
scratch reuse, and 11 use memory word reuse. These categories can overlap.
The checker derives direct-callee summaries by scanning paths that can
return to the caller. The proof of scalar return publication preserves
memory in that summary. The typed Rust exporter and the binding from raw
actions to their abstract footprints remain trusted. Memory is aligned and
bounded by 64 KiB. The certificates use unpriced MIR and shared action
replies.

The independent Rust regression retains the same result when one branch
writes the adjacent slot. Changing that branch to overwrite the cached
slot loses the certificate and returns 19 in the original and 0 in the
rewritten function. The three remaining captures are Morpho Blue's
`createMarket`, `setAuthorizationWithSig`, and `_accrueInterest`. They
need a live heap invariant connecting the free-memory pointer and
allocated objects before the checker can establish disjoint writes.

A separate heap probe replays a store through the current free-memory
pointer followed by a load from an older allocation. Both versions return
7 with an initial pointer of 128. With a pointer of zero, the older object
overwrites the reserved pointer word and the original returns 17 while the
forwarded version returns 7. Local Lean lemmas check that a continuing
allocation publishes its nonwrapping end and that a disjoint byte store
preserves the published word. The initial CSE gate leaves cursor propagation
through those captures unproved.

### Strict initialized-heap CSE audit

The strict serial gate `evaluate --all --require-verified --valid-heap
--pass cse --timeout 15 --seed 0` verifies all 48 pinned captures. The
capture IDs, raw before and after hashes, input fingerprints and compiler
outputs match the full audit and preceding CSE gate. The normal and capture
runs again agree on ABI, creation bytecode and runtime bytecode.

| Contract | Selected | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 4 | 4 | 0 | 0 | 0 |
| OpenZeppelin ERC20Mock | 6 | 6 | 0 | 0 | 0 |
| Morpho Blue | 38 | 38 | 0 | 0 | 0 |
| Total | 48 | 48 | 0 | 0 | 0 |

The compiler, launcher, replay binary, manifest and input hashes are those
in the preceding CSE audit. The checker on Lean 4.34.1 has fingerprint
`fe88a17abd058390bd248f24b207efb57af87575699befe742fb216b6100e670`.
`target/tv-cse-heap-final/results.json` has SHA-256
`7ba1f7f8b3cd103a530f2bc3730ee032b08c9acdc1065997c61e78a863d8b720`.

The three certificates for `createMarket`, `setAuthorizationWithSig`, and
`_accrueInterest` use `kernel-checked-valid-heap-read-reuse`. At the selected
logical MIR entry, aligned memory up to 64 KiB covers the cursor at byte 64.
The initial free-memory pointer lies between 128 and the uint64 maximum.
Lean checks cursor offsets along all admitted edges and proves the result
and exact cursor word of a successful nonzeroing allocation. A normal
return with a typed scalar or pointer can compose a cursor summary.
The `createMarket` decoder advances its caller's cursor by 160 bytes and
returns the old cursor. The kernel checks that continuing raw messages
preserve active words outside the output range in caller memory.

The trusted typed exporter binds transfers, summaries and operands to
the source effects. The heap analysis covers acyclic bodies and forgets
a cursor when its incoming paths cannot establish a common offset.
The initialized-heap contract applies to the selected logical MIR entry.
The `createMarket` cut below binds the resumed ABI wrapper. The other
activations and pricing the rest of the emitted execution remain open.

The independent regression returns 7 on both sides with an initial pointer
of 128. With a pointer of zero it returns 17 before reuse and 7 after
reuse. Redirecting a write to the old allocation produces the same
difference at a valid pointer of 128 and loses the certificate even with
`--valid-heap`.

### Metered Morpho heap entry cuts

The `heap-entry` command checks the exact pinned Morpho runtime image,
SHA-256 `d2c1c3d7bb22df8c2e0e9853f10f2657af1c547028e7287b0452a9943a23cd2e`.
Lean checks each emitted dispatch instruction from PC zero to the FMP
store, including the selector read, calldata length checks, jump targets,
stack words and exact debit. The store writes 288 to bytes 64 through
95 and establishes the initialized-heap contract from the emitted frame.
The proofs quantify calldata with the stated selector and minimum length
below `2^256`, zero call value, and arbitrary initial journal.

| Public route | Selector | Minimum bytes | Instructions | Gas | Cut PC |
| --- | --- | ---: | ---: | ---: | ---: |
| `createMarket` | `0x8c1358a2` | 164 | 61 | 250 | 1442 |
| `setAuthorizationWithSig` | `0x8069218f` | 260 | 47 | 191 | 6639 |
| `accrueInterest` | `0x151c1ade` | 164 | 47 | 185 | 1442 |

Independent Rust replay reached the matching cut with zero gas remaining
at each exact budget, and ran out of gas one unit below each budget.
Lean also checks the rollback result for every smaller budget.
Changing the shared store immediate from 288 to 256 rejects the original
certificate at the same PC. Lean and Rust confirm the changed floor at
the same 250-gas boundary when the check requests 256 explicitly.

The command reports both the initialized cursor and the requested fresh
heap extent. The kernel checks the byte-image binding and selector path
through the terminal store. The independent replay proposes the path and
checks both gas boundaries. Reports in `target/tv-routed-heap/` use checker
fingerprint `b79a2d0e1111bcf0c4acd44820fc64689886f6e37db09dd3bd0659742bad7a81`.

| Route report | SHA-256 |
| --- | --- |
| `createMarket.json` | `caea1fa51c591ff42fc9bd2f82c618de86e6127586efe16fccb1aa0170b6b745` |
| `setAuthorizationWithSig.json` | `c4dd078660f3334926ac5e6e6f7d929c1417f0123319959f158653c9e0f00d6f` |
| `accrueInterest.json` | `fd6ad8b98610f920055177d76f5773ab1083b98bb4e305314c40caed99ce5ca3` |
| `changed-floor.json` | `978e7b1adce8cd7d22dfb11c608c72fdc736e5cb605890cbb5ceb71a41e250ef` |

The entry-cut proofs establish the physical heap producer. The decoder
and `createMarket` wrapper checks below extend that boundary. The internal
`_accrueInterest` activation and the emitted suffix remain open.

### Emitted Morpho decoder cuts

Standalone certificates extend the three heap-entry paths through their
five fixed calldata words on the same pinned runtime. In each case the
terminal memory is nine words `[0, 0, 288, 0, w0, w1, w2, w3, w4]`. The
proofs assume the matching selector, zero call value, and the preceding
route's calldata length bounds. The two market routes assume that the
first four words fit in 160 bits. The authorization route assumes that
the first two words fit in 160 bits and that the third word is 0 or 1.
The initial journal is arbitrary. Lean checks that the journal and
returndata are unchanged at the cut.

| Public route | Instructions | Gas | Cut PC |
| --- | ---: | ---: | ---: |
| `createMarket` | 110 | 448 | 1872 |
| `setAuthorizationWithSig` | 92 | 365 | 6698 |
| `accrueInterest` | 96 | 383 | 7111 |

Lean checks the exact debit and rollback for every smaller budget. Rust
independently reached each cut with nonzero calldata words and zero gas
remaining at the exact budget, and ran out of gas one unit below it. The
market paths also check the stack-carried return to the two distinct
continuations and the live object pointer of 128. Changing the store
address immediate at PC 1466 from 160 to 192 made its original Lean byte
binding false. Rust reached the same cut with bytes 160 through 191 zero
instead of the second calldata word. Lean also independently rejected the
original heap-pointer certificate against the changed immediate at PC 1436.

| Artifact in `target/tv-routed-heap/` | SHA-256 |
| --- | --- |
| `DecodercreateMarket.lean` | `20875ae98668ff817e7eee920540ff1854a576fce2eeb74cdd8f86945eaf4bc3` |
| `DecodersetAuthorizationWithSig.lean` | `07f1683475281512052411761b1341a8444ba7a5451cb6a75f3c47d46cad7c44` |
| `DecoderaccrueInterest.lean` | `f8281aa740623d9dc71c429ba266f4b1872a592aedccf8de2c7b9b8ea280e17f` |
| `decoded-market-replay.json` | `33403861aa989d54defcef6bd412714333f8a0e0c5bbb955d17e200ac8532dae` |

The independently exported `decode_calldata_type` helper in both sides of
the `createMarket` CSE capture has SHA-256
`6cb35e8703c38b45648ce47e815bdfd27a9eacabe82f853f70d5aee43c3c907e`.
The direct MIR interpreter starts this helper with `[0, 0, 128]` in
memory. Lean checks its 160-byte allocation, canonicality checks and
stores. The result returns pointer 128 and preserves the journal.
`MarketDecoderAgreement.lean` in the same directory has SHA-256
`7547de8627e9a20c54821a2d96464d0bdba8cc7bccec0c102da510371f4cba4e`.
It checks that the actual `createMarket` bytecode reaches PC 1872 with
the same byte memory and with the MIR result pointer at the top of its
stack. The theorem uses the typed MIR export and a separate logical heap
initialization premise.

### Resuming the Morpho `createMarket` wrapper

Lean checks the `createMarket` selector in the exported routing table and
both complete 46-function MIR tables from the actual CSE capture. The
direct interpreter reaches block 1, instruction 1 after 35 transitions
on both sides. The certificate checks the saved caller and the ordinary
internal return, which puts pointer 128 in SSA local 3. The new
`MIR.State.Prefix` theorem composes those transitions with ordinary MIR
execution for every remaining work budget. Finalization still uses the
original frame as the rollback checkpoint.

The emitted runtime reaches PC 1872 with the same private memory and
returndata, the same journal, and the returned pointer on top of the
operand stack. The bytecode certificate charges 448 gas. The proof uses
the canonical market calldata premises above and initializes the logical
wrapper's memory with `[0, 0, 128]`. The MIR configuration must expose the
five prefix opcodes, admit an internal call, and leave the helper's scalar
return unplaced. The remaining configuration and execution work are left
open for the continuation.

Rust independently executed the captured MIR decoder on both sides. The
returned pointer and all 288 active memory bytes match the emitted EVM
probe for the same input. Changing the decoder allocation from 160 to
192 bytes changes the MIR heap pointer from 288 to 320. The original
binding rejects the independently exported changed helper. Correspondence
after the resumed wrapper and the other public and internal entries
remains open.

| Artifact in `target/tv-routed-heap/` | SHA-256 |
| --- | --- |
| `MarketWrapperFull.lean` | `683e2024c8cf9a145a99ff23ed3fb7ae2efa434d817eaccdd1840c537c0c3ae9` |
| `MarketWrapperBytecode.lean` | `38887dc699d7a960f9172cc6f1e89d6617ce4790cace228a31d08b48e2b4b350` |
| `mir-decoder-native.json` | `3fa71a237ba72898db874748ad5b826f4e51ce4d6a2c34ba7ec52197cd61f502` |

### First Morpho storage read from ABI entry

Lean composes the selected `createMarket` route with both typed MIR wrappers
from the CSE capture and the emitted bytecode from PC zero. The MIR cut is
after 43 transitions. The emitted cut is PC 1901 after 132 instructions.
The cuts agree on active memory, the journal with the slot warmed, and the
storage word in MIR local 14 and on top of the EVM stack.

The emitted prefix costs 603 gas before the storage charge, so the cut
requires 703 gas when warm or 2,703 when cold. For every smaller budget,
Lean checks out-of-gas with the initial journal restored, given sufficient
model work to execute the prefix. The MIR proof retains the decoder's
entry memory and scalar return-placement premises. Both sides use the
canonical calldata and zero-value conditions of the decoder certificate.

Independent Rust execution confirms the same slot and value on both MIR
sides, the emitted stack and memory at each exact gas threshold, and
out-of-gas one unit below each threshold. Correspondence past this cut
remains open.

| Artifact in `target/tv-routed-heap/` | SHA-256 |
| --- | --- |
| `MarketFirstReadEVM.lean` | `ff909fac33d51e4337ba04ce1c2b03c60085b97c6ef8a53830c0f60a0bd32f38` |
| `MarketFirstReadMIR.lean` | `46cbb98e08348f8be463ff1493e902c3e6602f9adc53cf92e1509a55b9366e95` |
| `MarketFirstReadAgreement.lean` | `827c2126831c037f67fd38f022d7d03a011a4cad485273b6ae79324de3ae90df` |
| `market-first-read-native.json` | `4ffb062549c4cf028cb72dff2d58311553f7ae50eae9be44ba567d3842f75b52` |

### Full corpus after MIR and backend coverage

The serial audit at `01b6201301` used `evaluate --all --timeout 3 --seed 0`.
Every capture ID, pinned source and emitted output matches the preceding
complete audit. Each contract also emits identical ABI, creation bytecode
and runtime bytecode in the baseline and capture compilations. The proof
budget is three seconds per checker call. Larger targeted gates keep their
own counts and budgets.

| Artifact | SHA-256 |
| --- | --- |
| Compiler executable | `7d123ab7483266256ead8e4b3d86cbce427376f186b5714969110d19139335e8` |
| Serial compiler launcher | `4e5a4f6faa73f5d017a534afaa4d3f9a8c052abe1c2435f192108577b79239e9` |
| Pinned Rust replay | `649ae6a1ae51d75ca501093d1a0f0a1181ff098d4f386e09b6e5754c01374365` |
| Lean checker on 4.34.1 | `c1e21b1c3873121dc3adf07e9502900196744fe7c52370de649719d8955437e5` |
| Corpus manifest | `b08af7eb00ac67a5ef1200d50273beedc48d5517838ed67504bd1a76ba96b7b7` |
| Complete results.json | `5f8488b21551ea6732b476a30c1c0e99640099eb1228398a43b0f9f64d7fe8a5` |

| Contract | Input SHA-256 | ABI and bytecode output SHA-256 |
| --- | --- | --- |
| Solady WETH | `b751f0da2642fba80d176248ff576b06d45601fda253d9de32f9a78e3b70a4c0` | `671d642b2c47309ccd86125a4b1e35b281d4d2c367df2b88acacfcffb1c3c978` |
| OpenZeppelin ERC20Mock | `c85c24c264a2ae9a97b32050f93426376c8cb20e15f6e5ad5dc1414ee46ce2c4` | `5e5fb3a0613c6ec21d8b61be398fc1ee325fcce75b144d13107356276d288213` |
| Morpho Blue | `01f4aaf9db4940ea9a62aba808ba1655ef399c671b6ed29875f8a91c3efeba53` | `8ef2219d25d2b256dad76949a880ba655074bee6eb7f53bdf38e14381f43d8a0` |

| Contract | Captured | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Solady WETH | 330 | 242 | 60 | 28 | 0 |
| OpenZeppelin ERC20Mock | 372 | 266 | 47 | 59 | 0 |
| Morpho Blue | 1,526 | 884 | 462 | 180 | 0 |
| Total | 2,228 | 1,392 | 569 | 267 | 0 |

| Layer and pass | Captured | Verified | Unsupported | Timed out | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| evmir `block-cse` | 10 | 4 | 5 | 1 | 0 |
| evmir `block-layout` | 368 | 299 | 68 | 1 | 0 |
| evmir `cfg-simplify` | 7 | 5 | 2 | 0 | 0 |
| evmir `cfg-simplify-early` | 291 | 130 | 151 | 10 | 0 |
| evmir `compact-pushes` | 118 | 73 | 44 | 1 | 0 |
| evmir `copy-loop-blocks` | 2 | 2 | 0 | 0 | 0 |
| evmir `dce` | 2 | 0 | 0 | 2 | 0 |
| evmir `inline-returns` | 14 | 7 | 6 | 1 | 0 |
| evmir `late-word` | 19 | 0 | 18 | 1 | 0 |
| evmir `loop-layout` | 29 | 27 | 2 | 0 | 0 |
| evmir `outline` | 7 | 1 | 6 | 0 | 0 |
| evmir `peephole-early` | 35 | 11 | 24 | 0 | 0 |
| evmir `reorder-expressions` | 1 | 1 | 0 | 0 | 0 |
| evmir `reorder-pushes` | 5 | 0 | 5 | 0 | 0 |
| evmir `stack-normalize` | 5 | 0 | 5 | 0 | 0 |
| evmir `stack-reschedule` | 17 | 0 | 17 | 0 | 0 |
| evmir `tail-merge` | 49 | 45 | 3 | 1 | 0 |
| evmir `terminal-dedup` | 124 | 106 | 18 | 0 | 0 |
| mir `byte-run-loads` | 14 | 14 | 0 | 0 | 0 |
| mir `call-cleanup` | 1 | 0 | 0 | 1 | 0 |
| mir `cfg-simplify` | 144 | 120 | 0 | 24 | 0 |
| mir `check-elim` | 7 | 2 | 4 | 1 | 0 |
| mir `coalesce-allocs` | 1 | 1 | 0 | 0 | 0 |
| mir `cse` | 48 | 37 | 3 | 8 | 0 |
| mir `dce` | 86 | 66 | 0 | 20 | 0 |
| mir `dead-arg-elim` | 11 | 0 | 11 | 0 | 0 |
| mir `egraph` | 116 | 90 | 0 | 26 | 0 |
| mir `evm-inst-schedule` | 32 | 0 | 14 | 18 | 0 |
| mir `function-dce` | 3 | 0 | 0 | 3 | 0 |
| mir `indvar-simplify` | 2 | 0 | 2 | 0 | 0 |
| mir `inline` | 58 | 2 | 51 | 5 | 0 |
| mir `integer-cleanup` | 35 | 0 | 19 | 16 | 0 |
| mir `jump-threading` | 52 | 42 | 6 | 4 | 0 |
| mir `licm` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-abi` | 65 | 57 | 2 | 6 | 0 |
| mir `lower-abi-encode` | 16 | 15 | 0 | 1 | 0 |
| mir `lower-alloc` | 19 | 11 | 7 | 1 | 0 |
| mir `lower-arithmetic` | 16 | 9 | 7 | 0 | 0 |
| mir `lower-builtins` | 6 | 5 | 1 | 0 | 0 |
| mir `lower-checks` | 31 | 31 | 0 | 0 | 0 |
| mir `lower-dispatch` | 73 | 0 | 0 | 73 | 0 |
| mir `lower-evm-shaped` | 3 | 2 | 0 | 1 | 0 |
| mir `lower-immutables` | 1 | 0 | 1 | 0 | 0 |
| mir `lower-integers` | 55 | 54 | 1 | 0 | 0 |
| mir `lower-mapping-slots` | 32 | 22 | 10 | 0 | 0 |
| mir `lower-memory-objects` | 27 | 26 | 1 | 0 | 0 |
| mir `lower-memory-zero` | 1 | 1 | 0 | 0 | 0 |
| mir `lower-slices` | 1 | 1 | 0 | 0 | 0 |
| mir `lower-structs` | 1 | 0 | 0 | 1 | 0 |
| mir `memory-dse` | 10 | 3 | 6 | 1 | 0 |
| mir `memory-dse-unread` | 1 | 1 | 0 | 0 | 0 |
| mir `merge-aborts` | 42 | 7 | 20 | 15 | 0 |
| mir `place-reads` | 33 | 29 | 0 | 4 | 0 |
| mir `sccp` | 12 | 9 | 2 | 1 | 0 |
| mir `simplify-phis` | 1 | 0 | 1 | 0 | 0 |
| mir `specialize` | 1 | 0 | 0 | 1 | 0 |
| mir `spell-width-tests` | 38 | 1 | 20 | 17 | 0 |
| mir `storage-dse` | 10 | 10 | 0 | 0 | 0 |
| mir `storage-promotion` | 12 | 12 | 0 | 0 | 0 |
| mir `word-sequence` | 6 | 1 | 4 | 1 | 0 |
| mir `word-sequence-early` | 1 | 0 | 1 | 0 | 0 |

The verified count moved from 1,059 to 1,392 under the same budget.
A timeout or unsupported result leaves
the corresponding check open under this budget. Failed checks include
0 tool errors, 0 inconclusive results and
0 independently confirmed counterexamples.

Matching saved targeted reports to the same capture IDs identified MIR
inlining as the largest outstanding group at this baseline. It verified two of 58
`inline` captures, with 51 unsupported and five timeouts. The checker
rejected 36 after inlining changed a retained body and removed a call
target. Another 12 reached a function-local route that excludes GAS.
A rooted module certificate needs to retain both the changed entry
closures and their ordered effects.

### Strict MIR inlining audit

The serial `evaluate --all --require-verified --pass inline --timeout 120
--seed 0` run verifies all 58 pinned captures. Every capture ID and input
and output fingerprint matches the full audit. Normal and capture
compilation agree on ABI, creation bytecode and runtime bytecode.

| Contract | Verified | Changed entry proofs | Certificate SHA-256 |
| --- | ---: | ---: | --- |
| Solady WETH | 16/16 | 8 | `04f92117faa8b23214e5d9bf84e751b40febdcda16963483dee69f16029627e6` |
| OpenZeppelin ERC20Mock | 13/13 | 5 | `91667bab756b4982e03d10b009489f7060676c5f29bc3a748d18d74728db11a1` |
| Morpho Blue | 29/29 | 10 | `8426cff9af58e362c66dc5bff34f0e04c02bc35a47229f0e10c55634b4bbf7aa` |

The compiler executable fingerprint is
`f996616dcf2ebacf05fc5cc439f4814dc38fe953fad90b698754281c59ef139c`.
The serial launcher and replay fingerprints match the full audit above.
The Lean checker fingerprint is
`57bb55b11c5853e4af58e9e4d75d8a51ebfdd39831223db836e6710a38260930`.
The complete `target/tv-inline-final/results.json` has SHA-256
`a734499fe221b568aadc8789dc5f2a5d4b6197fa7a77ea3481cbbe2a2a720c2b`.

Lean checks the public entry table and the closed set of unchanged
callees. It compares the expanded typed effect trees of changed entries
for every logical argument, environment, initial state and shared effect
handler. Calls to unchanged function closures keep a named request.
The kernel checks initialization of admitted two-word products.
Checks, messages, returndata and GAS keep their operands and order.
The typed renderer and bindings from these requests to their concrete
handlers are trusted. Physical gas and backend placement remain open.

The live hook emits a module certificate for a Solidity storage-reading
helper. Normal and validation-enabled compiles produce identical ABI,
creation bytecode and runtime bytecode, with contract output SHA-256
`99df4a4cd52613c63c437e1b40c55780477d1ad64c4ba197ef67fdc8c7ae7fd1`.

Independent Rust replay gives the same maximum allowance on the
original and inlined WETH pair for the Permit2 path. Changing the
inlined flag gives zero and loses the module certificate. The replay
under `target/tv-routed-heap/inline-weth-native.json` has SHA-256
`2e037a498ad0103ab99a6eae3ceebefd01c6d6a2ceb3483ccf06af603203589d`.

### Stack-carried continuations

The physical indexed-entry certificate also tracks literal words through
PUSH, DUP, SWAP and untouched stack suffixes, retaining the top 32 facts.
The kernel connects each abstract stack motion to the bytecode interpreter,
including the parent's stack after CALL or CREATE resumes. Branch conditions,
world state and child responses remain arbitrary. Closing the graph proves
that every path to the table crosses a checked selector producer, with the
existing code-prefix and final-image assumptions.

The Solidity dense-switch regression shares an internal storage-reading helper
between two public functions. Its original emitted bytes receive physical-entry
coverage. Redirecting the second function's return address into the table
removes that coverage while preserving the local guard proof. Independent
raw-byte replay confirms a changed output and storage write for `other(0)`.
Compiling with and without validation preserves ABI and both bytecode outputs.
Spilled addresses, arithmetic-derived jump targets and backend placement
correspondence remain open.

The continuation certificate independently exports source label pushes and
dynamic jumps. The proposed graph associates each reachable source point with
the tags on its stack. Lean checks the entry, stack operations, both successors
of each branch, and the destination of every dynamic return against the source
graph. Ordinary operands must be untagged. The proof shows that a finite shape
and a full shadow stack track the same tags, and that changing the addresses
of tracked labels cannot change the operands of an admitted consumer.

After this check, the abstract graph replaces a label push with a dummy word
and a resolved return with a pop to its checked destination. The CFG proof
checks the resulting pair for every incoming height and logical work bound.
Physical PCs and exact gas remain outside the proof. Code-image observers,
external messages and dynamically computed jump destinations are excluded.

The empty-calldata WETH deposit slice from the pinned capture checks the
return push redirected from `bb36` to `bb27`. The proof starts at that push,
includes the storage and log effects of the helper, and follows the return
into the continuation. Redirecting it to the error block changes success to
revert, confirmed by the kernel and independent Rust execution. The full
dispatcher includes external message paths outside this certificate.

A serial recheck of the 159 frozen `cfg-simplify-early` regions whose exports
previously rejected a stack-carried label verified four at eight seconds per
proof. Another 15 timed out and 140 remained unsupported. Retrying the ten
smallest timed-out graphs at 25 seconds verified seven more, for 11 verified
and eight still timed out. ABI, creation bytecode and runtime bytecode matched
for all three corpus contracts.

### EVM interaction traces

The interaction trace route extends the continuation certificate for
`CALL`, `STATICCALL`, `EXTCODESIZE` and `GAS`. It checks each opcode's
stack arity against the typed request decoder. The relocation proof checks
that these requests cannot observe a stack-carried label. The call decoder
uses the same zero-extended byte reads as the state interpreter.

The proof stops before the reply at an arbitrary request ordinal. Earlier
requests use shared responses and the remaining code uses shared
position-independent handlers. This compares every finite request prefix
and distinguishes a halt before the specified request. It observes calls
before admission and checks their requested gas. Actual parent and child
gas, code-size replies, deployed code identity and reentry remain separate
obligations. The CLI requires explicit `--interaction-trace` selection.

The CFG certificate checks both the continuation and jump-chain projections
with indexed trees. Lean checks the subtree sizes and proves the lookups
agree with the original ordered rows at every index. The kernel still checks
every edge and opcode against the full source graph. Identical projections
use graph equality. When jump growth differs, the kernel checks the local
stack bounds before proving the projections agree. The generated tables are
used only in the kernel, so Lean skips executable code generation for them.

The serial recheck verified all 115 eligible frozen `cfg-simplify-early`
captures with a single 80-second proof limit. The median check took 12.2
seconds and the slowest took 50.5 seconds. A 230-point Morpho region that
previously timed out at 32 seconds now verifies in 6.5 seconds. The previous
mixed-budget run verified 10 captures and timed out on 105. Normal and
capture builds produced identical ABI, creation bytecode and runtime
bytecode for all three pinned contracts.

### Gas at a sampled CALL

The Cancun sampled-call proof starts at an emitted `GAS` followed by a
value-capable `CALL` with zero input and output offsets and lengths. It
derives the child budget from the physical gas sample, the account and
value charges, and the EIP-150 cap and stipend. Its result also covers an
unpaid `GAS` or `CALL`, a static violation, and depth or funds rejection.
The pair theorem needs matched gas and state at the `GAS` instruction,
including active memory and the account journal.

On the frozen WETH withdrawal, one `cfg-simplify-early` pair forwards
79,852 gas on both sides with 100,000 entry gas. Another pair spends 12 fewer
gas getting to the sample and forwards 79,840 before and 79,852 after. At
21,218 entry gas the first image of that pair runs out of gas at `CALL`,
while the second admits the call with the 2,300 stipend and succeeds. The
regression checks both pairs against the assembled bytecode using Lean and
the independent Rust replay. The prefix from ABI entry is replayed for these
inputs, and call admission is symbolic at the matched `GAS` cut.

### Compact PUSH recipes

The checker matches each recipe to consecutive instructions and successors in
the independently exported EVM graph. It runs the recipe on a private stack,
then `run_summary` lifts the result to every incoming stack. It proves the
temporary peak, the words left above the unchanged incoming stack, and the
exact local execution charge for the target fork. The projected graph retains
that stack guard and the resulting literal pushes. Adjacent matching pushes
can join the recipe when they reach the same peak before the next effect.

The CFG simulation checks both successors of a branch and every matched
action. It uses the supplied entry height bound, or checks every architectural
entry height when no bound is supplied. The default check uses gas-insensitive,
position-independent handlers. Both modes require enough gas for every reached
charge and admission check. Physical transfer prices and the compiler's
exporter and schema binding are outside the proof.

The recipe checker can compose its proof with continuation provenance.
Lean checks each stack-carried return from the original labeled graph.
Every recipe scan excludes the projected zeros used as relocation
placeholders. The opt-in interaction trace gives the continuation shared
replies to `GAS`, `EXTCODESIZE`, `CALL`, and `STATICCALL`. Recipe charges and
stack peaks remain checked. The proof compares the ordered requests under
those replies.

The entry trace independently imports the first physical block and uses
`matchingEntry` to instantiate the contextual theorem with an empty stack.
The same invariant checks every reachable continuation and temporary stack
guard. Immutable PUSH terms retain the exact slot and width, and share the
word supplied by deployment. They cannot be evaluated as literal recipes.
Deployed code identity and physical relocation charges remain outside the
certificate.

For `--entry-stack`, the kernel checks the two caller graphs independently.
`StackReachability.projected_cap` binds point zero to the physical entry and
an empty tag shape. Each projected successor preserves its certified height
bound, including both directions of a conditional and every resolved dynamic
return. The local compact PUSH theorem uses that cap with equal live-in
state. The caller certificates place no equivalence requirement on unrelated
rewrites in the prefixes.

For inherited returns, `caller_mask` checks the region's tagged stack slots
in every context reached by each entry projection. The regional projection
uses the slot depth as its logical identity, so the proof covers callers at
different physical offsets. At a transfer out of the region,
`inherited_return_word` recovers the actual address and the remaining stack.

Mixed compact PUSH and address-fold rewrites use `StackWords.invariant` to
prove the carried base word. A literal establishes the fact, stack shuffles
and the untouched tail preserve it, and every edge into the use must justify
it. The funded address-fold theorem checks the actual PUSH, DUP and ADD
sequence, including its temporary stack peak and charge.

Constructor entry traces match the exact instruction sequences for in-bounds
reads of a sole declared data suffix or the argument bytes appended after it.
The checker independently reads the assembled PUSH operands and verifies
the data suffix. The relocation lemmas derive the same bytes and argument
length under arbitrary code prefixes and shared arguments.

The pinned WETH `name()` capture checks the return-data path. With its next
literal push included, the compared Cancun sequences charge 6 and 12 gas and
both peak at two temporary words. The compiler's existing 1,023-word fixture
has no matching push to absorb the new stack peak. The original succeeds and
the compacted sequence overflows, confirmed by Lean and Rust execution.

At eight seconds per proof, the serial recheck verified 58 of the 103 frozen
`compact-pushes` captures that were previously unsupported. The new
certificate verified 48 and the existing stack-prefix route verified 10.
The other 45 remained unsupported. The ABI and creation and runtime bytecode
matched the pinned outputs for all three contracts.

A serial recheck of those 45 captures used an 80-second limit. All 45
remained unsupported with the original flags. The explicit interaction trace
verified eight captures, one WETH and seven Morpho, at every architectural
incoming height. Of the remaining 37, 20 lacked a sufficient entry-height
bound for the added stack slot and 12 had unresolved dynamic returns. The
normal and capture compiler outputs matched for all three pinned contracts.

The entry-height certificates verified 22 more. The last 15 verified after
checking eight inherited return paths, three constant address folds and
four constructor code-read paths. Across the serial rechecks, all 103 frozen
`compact-pushes` captures verified under their recorded proof conditions.

### Shared MIR tails

The certificate pairs source cursors instead of assigning one global
counterpart to every block or register. The source exporter binds each phi
input to its actual predecessor. The certificate checks all successors and
records which local pairs must hold equal values at each cursor. Each action
receives the same ordered operands. A successor's local pair must either
agree already and remain untouched, or be written at the same result
position by the matched action. Phi groups apply this test to parallel reads
and writes. Induction over logical work proves the same observations through
joins, loops and shared failure paths.

The importer and its typed action descriptors remain trusted. The theorem
quantifies a shared handler for each descriptor, with `GAS` and message
effects retained in order. Physical gas, ABI frames and runtime callback
binding remain separate obligations. The regression checks actual
`cfg-simplify` output that shares two storage-writing tails with distinct
local IDs. Independent MIR replay confirms the same storage and return
values on both former tails, and detects a rewrite that stores the wrong
local.

Literal `i1` branches are retained in the source graph. The kernel follows
their selected edge and the typed transition lemma establishes the same
predecessor as an unconditional jump. A flat product seeded by `undef`
starts with an empty field mask. The kernel tracks each inserted field and
requires a complete mask at every use outside the construction. The mask
lemmas use the interpreter's actual product update and undefined field
semantics. The certificate checks unchanged callees by equality of their
exported action graphs. It checks groups of points from changed functions
against one indexed table so the kernel can reuse the same source cursors.

The serial recheck selected 26 frozen `cfg-simplify` and `jump-threading`
captures that had failed structural matching on shared nodes. At eight
seconds per proof, 15 verified, five timed out and six remained unsupported.
A 20-second retry brought that total to 19. The current certificate verifies
all 26 with the 20-second proof budget. All three pinned contracts kept their
ABI, creation bytecode and runtime bytecode.

Matched word actions can use wrapping i256 addition and subtraction with
complementary constants. The kernel checks the normalized keys and proves
that the typed operations return the same word for every input. A callee
can also retain different full-word pointer casts. Both carrier graphs
must erase to the same checked graph, using an invariant that invalidates
cached copies when a loop overwrites their source. Four of the eleven
frozen egraph captures that previously required a loop invariant now verify.
They cover ERC20Mock `name`, `symbol` and the storage-bytes load and store
helpers. The regression replays a wrong subtraction through a two-iteration
loop in both interpreters.

An adjacent i256 increment and its overflow test can form one action with two
results. The kernel proves that `sum < previous` and `sum == 0` agree on every
word, preserving both the sum and the branch condition. That certificate
also verifies the frozen Morpho `extSloads` egraph rewrite. The regression
changes the zero test and replays the wrong result at the wrap boundary.

The CFG proof also compares power-of-two multiplication and division with
their shifts, doubling with a one-bit left shift, and high-bit masks with
their complement forms. It checks full-word behavior, including shifts at
or above 256. A mask sequence can discard its scalar temporaries only when
the source uses show they are private or dead. This verifies a sixth frozen
egraph loop capture, the word-scaled ERC20Mock storage-bytes store. The
regression independently replays a wrong shift count.

Same-phase carrier proofs also compose additive and subtractive word
offsets through full-width pointer casts within a block. The kernel checks
the original offset sequence against its proposed total at every input
word. The graph checks that the base and ordered memory effects match.
This verifies the frozen ERC20Mock constructor and another Morpho
`extSloads` rewrite. A changed store address is rejected and replayed.

Same-phase carrier proofs retain the exact types of slice and product
operations. That lets them prove all six frozen MIR CSE loop captures,
including the storage-bytes helper passed a slice and the constructor
that calls it. The proof checks which full-word pointer copies each path
may reuse. The regression changes the load address and reproduces the
wrong result on the original memory input.

Calldata size is a word from the current activation's immutable input.
The carrier proof shares this word across effects in the activation.
Volatile returndata size remains an ordinary operation. This verifies a
third frozen Morpho `extSloads` rewrite. The regression substitutes
returndata size and independently reproduces a different return value.

The serial recheck verifies all 19 previously unsupported loop-related
captures. It includes six CSE, two loop-layout and eleven egraph captures.
In ERC20Mock
`load_storage_bytes`, a right shift bounds the length, and the mask
preserves the bound. Lean proves the 32-byte rounding and header
addition stay in range, keeping the checked allocation and all its live
inputs. The last Morpho `extSloads` pair composes multiplication and
division identities, immutable slice-length projections, and check
polarity. The comparison can be reused from a single incoming predecessor
when no intervening definition changes its inputs. The kernel checks the
effect of both guard spellings, including their exact failure payload.

The checked MIR call table also supports direct recursive calls. A request
identifies its callee and typed operands, and the state resumes from that
callee's result. The kernel composes the checked bodies by induction on
the number of unfolded calls, with shared argument, entry and resume
handlers. The proof uses the logical MIR frames. ABI, physical frame
placement and gas follow their separate certificates. A real
`cfg-simplify` pair exercises a mutual recursion and independently replays
a wrong base-case value.

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

For terminal stack DCE, a lower-height invariant starts at physical
entry and follows successful transfers through the checked continuation
projection. Both branch edges constrain the floor. The kernel checks the
`POP` sequence and `STOP` at the original cut and proves the removed
instructions cannot underflow. The claim is local and unpriced, with
the code before the cut shared. The regression exercises two indirect
callers, including one that becomes too short and fails in raw-bytecode
replay.

The `--entry-trace` layout route starts with an empty stack at the first
physical block on each side. `ControlFlow.contextual_bounded` combines the
checked path projection with the stack simulation at that entry height.
Continuation provenance tracks logical return addresses through stack
operations. A stack-carried conditional jump requires a known local
destination and an ordinary condition. The projection consumes the address
before branching, checks both successors against the remaining stack, and
proves the same two-word admission check even at the 1024-word limit.
An indexed jump preceded by a literal PUSH retains the complete table in
the source export. Lean picks the indexed destination, proves that the
transfer preserves the incoming stack, and independently bounds the peak
on both entry graphs. This includes the pushed selector and scratch space
for the table expansion. The tests exercise real block layout, replay the
wrong table entry on assembled code, and reject a peak above 1024 words.
During byte emission, the indexed-table audit measures original block
label definitions in the byte stream. The independent Rust IR export reads
the ordered logical table before outlined stubs are appended. Lean binds
each logical target to its measured `JUMPDEST`, including repeated targets.
The label-definition and terminator marks from the emitter remain inputs to
this certificate. They are recorded separately from the relocation offsets
used to encode the table.

`EncodedTable.correspondence` checks every in-range selector, follows the
actual edges through packed code or outlined stubs, and lifts the transfer
to any live stack suffix with the exact static charge and stack peak.
Trailing program data cannot invalidate a target in the decoded prefix.
The mask route derives the bound from an encoded `PUSH` and `AND` and
checks their costs and literal against the logical producer. Raw replay
covers high-bit inputs, low gas, and changed entries and masks. The
compiler-bound regression checks an actual Solidity dense switch and a
corrupted table that sends a selector to a different valid block.

For a dense switch, the independent export recognizes the logical guard
and its bound. `IndexedGuard` checks the actual decoded branch against that
bound. Starting at the guard, its hit reaches the table with a proved
in-range selector. Its miss preserves the stack and reaches the fallthrough.
The taken path includes the emitted `JUMPDEST` charge between the guard and
the table. Raw replay checks the edges at their exact local gas thresholds,
including wrapped selectors and a changed range literal.

The optional entry certificate checks a closed set of decoded control
points. Its roots are physical entry, every guard miss, and every proved
table destination. Tables in the same image join the same certificate, so
a destination can lead to another checked table or back to the first one.
Outside a producer cut the graph follows both outcomes
of a conditional jump, and each jump target must come from a known PUSH.
The kernel proves that interpreter continuations stay inside this set
until a producer is reached. The set excludes the table entry. This holds
for arbitrary live inputs, gas, world state, and child responses. The
proof binds a fall off the bytecode to an exact image without trailing
bytes. For an opaque suffix, all resumed PCs must stay in the checked
prefix.

The Solidity dense switch has an entry certificate. Replaying an invalid
function selector against a redirected ABI branch reaches the table
directly and changes a revert into success. The modified artifact has
only the local certificate. Unresolved computed jumps, routes into opaque
data, and oversized reachability checks also leave this scope local.
The decoder still excludes Amsterdam images with extended-stack
immediates, which must be distinguished from `JUMPDEST` bytes.

The normalized graph keeps the largest temporary stack
requirement along each chain of jumps and checks that changed requirements
are safe on every reachable path. The proof shares immutable slots and
supported interaction replies. For recognized constructor reads it also
checks the assembled data suffix and uses common appended arguments.
Physical transfer charges remain outside the proof.

When a complete entry graph exceeds the pairing budget, `--region` with
`--entry-stack` runs the layout check on the selected region. Both
physical entry graphs independently establish its height bound and the
positions of any inherited return labels. The region compares equal
live-in ordinary state, and a return exposes the inherited continuation
slot and remaining stack.

The serial rechecks verified all 70 previously unsupported frozen
`block-layout` captures. They covered layout from physical entry,
stack-carried branches and regions with checked caller bounds. The final
two captures reach the same Morpho dispatcher branch and share a proof of
15,954 matched actions from physical entry. The three contracts' ABI and
creation and runtime bytecode matched the frozen compiler outputs.

For EQ/SUB inversions, `EVM.BranchLayout` checks the exact comparison and
branch against the independently exported graph. The branch must consume
the comparison result and have no other incoming edge or entry. The
word-level theorem checks both transfer directions, the live stack and
stack failure for all inputs. The projected graphs then check the matched
paths, including the stack requirements of their relocated jump chains.

Large entry graphs check each continuation or control-flow row against its
expected result in small kernel-checked ranges. The row lemmas assemble
those results into the same projection checked for small graphs. Each
large proof stage compiles in its own Lean process and imports the
previously checked declarations.

## Remaining obligations

The empty-runtime branch of `lower-dispatch` now has a kernel-checked proof.
The source export supplies an empty set of callable routes. Both modules
keep their shared declarations, constructor root and retained functions.
The appended entry's resolved blocks must form a finite path of
instruction-free unconditional jumps to an empty revert. Lean proves that
the direct interpreter returns the implicit runtime's empty revert with
the original journal and byte memory for every argument-free frame in the
model's memory domain and every work bound at least as large as the path.
This proves the runtime boundary for modules without callable routes.
The routed-dispatch certificate compares the implicit ABI route selection
with the actual interpreter prefix. The kernel checks that an argument-free
wrapper starts with the original journal, byte memory, returndata and
execution state, or that dispatch terminates with the original journal and
empty revert data. Kernel-checked signature agreement preserves the prefix
when the prover sets aside bodies beyond the cut. The retained-function
proof also checks direct and tail-call targets after function ID renumbering.
This covers selector routing, short calldata, receive and fallback, and
nonpayable value guards for unmodified non-library wrappers.

For a hoisted ABI head-size check, selector-only routing is composed with
the original wrapper's actual guard. The source table is renumbered so both
wrapper runs use the same logical runtime return owner. Lean checks that
every source route has the same minimum and an empty reject block, and that
the resolved tables differ only in the first wrapper terminators. On a
short call the original wrapper reaches an empty revert in six MIR steps.
On an accepted call both wrappers reach the same continuation after four
steps, with equal memory, journal, locals, stack and return owner. These
paths hold for arbitrary extra MIR work fuel and symbolic calldata and call
value in the routing domain. The certificate ends at that continuation.
Library routing uses the independently exported route mutability and
deployment-address immutable. The kernel checks direct and delegated calls
for arbitrary 160-bit deployment and execution addresses. The runtime
binding supplies the deployment address without a constructor staging slot.
View routes retain the original frame, and library routes accept call value.

The debug certificate uses the revert-string mode from the compiler
invocation, or the explicit standalone `--debug-reverts` flag. It chooses
the failure reason from the original routes and checks the actual
dispatcher transitions, including the scratch writes. A separate byte
memory theorem proves that those reference writes produce the exact
`Error(string)` bytes for every initial scratch image. Debug ABI lowering
retains the wrapper head checks and their own messages, so the certificate
checks the retained wrappers at entry. Backend placement beyond the supplied
runtime immutable binding and physical gas remain open.

On targets without EVM shift opcodes, the routing proof runs the constant
selector shift in the semantic phase model. The certificate also proves
that division by `2^224` gives the same selector for arbitrary calldata.
The legacy regression checks the actual `lower-shifts` capture for that
divisor. The runtime phase still requires legal opcodes.

The phase-only form of `lower-evm-shaped` has a module identity certificate.
Both original modules pass phase validation and retain the same canonical
MIR after removal of the lowered phase marker. The kernel checks equality of
the independently exported function tables and their direct MIR executions
for every configuration, entry, frame and fuel bound. This uses the same
trusted parser/schema exporter and constructor renderer as type-table erasure.

For executable changes, the checker preserves module declarations, entry
bindings, and function interfaces under the pointer-to-word representation.
The kernel compares direct MIR outcomes over symbolic arguments and arbitrary
aligned byte memory for the admitted acyclic functions and their callers.
This checks cast erasure through memory accesses and validates a nonreturning
call reshaped into a tail call even when the result interfaces differ. The
interpreter checks the return owner's type when values actually return.
A separate local proof handles empty phi-edge forwarders. The kernel compares
the full resolved bodies to the split graph and proves that ordered switch
selection and simultaneous phi assignment preserve the original edge's frame
for every traversal, including backedges. Independent replay exercises a wrong
memory address, tail-call argument and phi input.

When both phases retain the block graph, the carrier certificate compares
the independently resolved bodies as ordered requests. The kernel checks
the copy substitutions and the active-alias invariant on every incoming
edge. Phi groups read the predecessor's values before writing their results,
and a write invalidates the aliases it would make stale. Induction covers
each logical block bound, including the state of an unfinished execution.
The carrier lemmas relate full-width casts to the identity word operation.
Every other request retains its operands, order, and result binding.

The abort-tail certificate requires a resultless direct call followed by
`invalid` or `stop` and an unchanged straight-line revert helper. Lean checks the
actual typed helper body, including its memory stores. The helper's execution
ignores both the saved caller stack and the return owner and cannot produce a
normal MIR return. The graph then checks that both phases invoke that helper
with the same arguments.

These certificates close the full `lower-evm-shaped` captures for the pinned
Solady WETH, OpenZeppelin ERC20Mock, and Morpho Blue cases. WETH's `GAS` remains
a request to the same effect handler on both sides. Deriving its reply from
emitted instruction costs, general recursive direct execution, and physical
stack layout remain separate obligations.

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
simulation for every execution bound. Its region certificate ends at the
dynamic return with the full live payload.

The separate `--return-path=0x40c10f19` check extends the captured layout pair
from the ABI entry through the return of ERC20Mock's `mint`. Lean decodes the
caller PUSH and physical successors from both emitted byte images, checks
return-address stack roles and equates their funded path projections. A
segmented kernel trace and independent Rust replay agree on the live stack,
memory, returndata and final receipt for the explicit Cancun world. Lean also
equates the live journals at the continuation, with the two root code images
erased. Redirecting the caller's PUSH to the boolean return stub produces
32 bytes ending in `01` instead of empty returndata, while preserving the
mint's writes and log. The result is path-local. Alternate branches and
physical gas equivalence remain outside this certificate.

The symbolic `--mint-layout` certificate handles all five exits of the
selected mint entry in a writable Cancun frame. It checks every physical
instruction and its exact debit in both byte images. A checked instruction
also supplies its admission threshold, which can exceed its debit at SSTORE.
Induction gives the first unpaid instruction, full rollback, and the exact
funded result for every incoming gas balance. The two thresholds and costs
are compared over shared symbolic calldata, journal and access warmth.
The proof is a gas refinement. In the cold replay, the after image can mint
with 11 less gas. Minting zero tokens to a nonzero recipient can have the
same minimum in both images when the SSTORE sentry dominates.

`--burn-layout` uses the same certificate generator and storage model for
the other caller of this shared update path. Each byte image checks the
sender mapping key against the words actually stored before KECCAK256. The
proof partitions all incoming gas budgets for successful burns, nonpayable
calls, malformed calldata, the zero sender and insufficient sender balance.
The insufficient-balance path checks the exact custom error bytes. The
out-of-gas replay reaches the supply SSTORE after performing the sender
SSTORE. It checks the original balance and supply and an empty log list.

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
| Whole-function MIR interfaces | Automatic typed products/slices, void/multiple results, acyclic calls and recursive direct-call graphs with checked logical bodies | ABI entry, physical frame and allocation correspondence, recursion that needs different logical actions and broader defined memory domains. |
| EVM entry stacks and indirect transfers | Fixed or symbolic entry stacks, checked physical-entry bounds, stack-carried jump and conditional targets, and compiler-bound table proofs with entry coverage for resolved jumps | Entry coverage through unresolved computed jumps, independent certification of block placement, and broader physical stack, gas and state correspondence. |
| Importer correctness | Certificates for acyclic EVM regions and acyclic typed scalar MIR | Extend independently checked imports to calls, memory, state, ABI entries and symbolic checked powers; audit the remaining Rust and opcode boundaries. |
| Physical code and resource correspondence | Explicit layout assumptions and selected priced execution | Backend layout certificates, complete caller work pricing and gas-sensitive transfers; observable code locations and resource failures must agree. |
| Fork and transaction completeness | Signed set-code authorization processing through Osaka, Amsterdam reservoir initialization, debits, refills, signed-ledger rollback and nested meter transitions for supplied frame charges | Other transaction envelopes, fee and block settlement, Amsterdam stateful opcode pricing and nested-frame integration in the interpreter. |
| Practical evaluation and review | Pinned three-contract sampling plus regression pairs | Wider published corpus, stronger pass coverage, independently reproduced results and external soundness review. |

In the initial sample, the largest blockers were unknown EVM entry heights and
MIR functions outside the single integer-result fragment. Automatic typed source
execution now covers those interfaces; ABI/layout and memory-domain obligations
continue to limit the selected real-contract checks. Removed function
identities, shared declaration/phase changes, internal calls, dynamic return
jumps and local calldata reads also remained unsupported. Lowering a status to
`verified` without addressing its obligation would hide a gap, not close it.
