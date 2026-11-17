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

Rechecking those same frozen pairs at 30 seconds now verifies **21 of 24**, with
3 unsupported and no counterexamples, timeouts or errors. Morpho's `lower-alloc`
constructor first raised the count to 16; the OpenZeppelin ERC20Mock
`lower-alloc` constructor then raised it to 17, and WETH's `block-cse` window
raised it to 18. WETH's `block-layout` region then raised it to 19 through the
separately reported source-graph identity certificate. ERC20Mock's
`block-layout` region then raised it to 20 through the stack-layout simulation.
WETH's `lower-alloc` body raises it to 21 under the explicitly reported
fresh-heap placement/FMP producer condition. No other check changes status.
The selected check identities, input hashes and compiler-output hashes are
identical; compilation results are reused.
The compiler fingerprint is
`8a897a40c88678e68e53f3b08b0e86099d5190f94478681ceda9c68e3ef3ae37`
and independent replay is
`b3e69bf9df7c153571cb67cecc99249abcb8a75c4600d0f51bfcdcd4183e182d`.

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

These limits remain explicit. They must be removed with semantics, proofs and
regressions rather than suppressed statuses or stronger unrecorded assumptions.

| Gap | Current boundary | Evidence needed to close it |
| --- | --- | --- |
| General loops | Acyclic expansion, equal pure cyclic interpreter graphs, or exact structural EVM source-graph identity | Inductive simulation/invariants, including phis, state, divergence and failures; real loop-pass pairs and broken invariants. |
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
