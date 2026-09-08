# Stack scheduling research

Source review on 2026-09-06 targets the fresh scheduler, not the deleted backend.
MIR values already have SSA identities. Avoiding their conversion into backend
memory homes is distinct from promoting source-memory slots into SSA, which
belongs in the retained MIR `frame-slot-promotion` pass.

## Primary sources and algorithms

| Compiler and pinned revision | Relevant design | Limits for this rewrite |
| --- | --- | --- |
| LLVM EVM / solx, `9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a` | Expression scheduling, backward propagation of desired stacks, failed-access discovery, lazy spill weights and selective retries | Recursive functions cannot use its ordinary memory-spill recovery; abstract address spaces do not establish physical memory isolation here |
| Vyper / Venom, `6dd5fef7ce71bb9b363ceb94df94080d451f4236` | Data/effect dependency scheduling, successor stack demands, liveness, selective spilling at inaccessible operands, reusable spill slots | Recursive calls are invalid; per-function spill regions rely on its own memory model |
| Sonatina, `8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7` | Private SSA stack identities, inherited single-predecessor layouts, reconciled merge interfaces, bounded reachability rescue, monotone spill discovery | Search policies and final spill-storage machinery carry complexity and compilation cost; their assumptions need independent checking |

Fe at `aa0aef13d11f7d4c9ef608ba78e9e693026945a2` integrates Sonatina but
[pins revision `039a9f5`](https://github.com/argotorg/fe/blob/aa0aef13d11f7d4c9ef608ba78e9e693026945a2/Cargo.toml#L54).
The current Sonatina source reviewed here is therefore not the exact dependency
shipped by that Fe checkout.

Solx's [stack solver](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMStackSolver.cpp)
propagates required stacks backward, finds actual inaccessible slots, selects
low-weight spill candidates from those windows and retries. It retains cheaper
DUP access to a spilled value when available. Its
[expression pass](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMSingleUseExpression.cpp)
shortens safe def/use chains before stackification. The implementation credits
solc's backward-propagation idea; it is not a globally optimal solver.

Venom's [DFT pass](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/passes/dft.py)
respects data and read/write-effect dependencies while arranging computations
for successor demands. Its
[emitter](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/venom_to_assembly.py)
tries accessible non-operand spills before bulk recovery, and its
[deep DUP recovery](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/stack_spiller.py)
spills only the excess prefix. Its
[Mem2Var pass](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/passes/mem2var.py)
promotes restricted allocas between SSA passes; this belongs at our MIR layer.
Its [cleanup safety analysis](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/stack_safety.py)
composes caller/callee growth and verifies emitted peaks, rather than assuming
that a locally small stack proves the complete execution safe.

Sonatina's [stackification contracts](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/mod.rs)
distinguish DUP16's 16 sources from SWAP16's 17 positions.
[Bounded rescue](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/rescue.rs)
can remove dead words, cheap constants or redundant copies before requesting
spills. [Block interfaces](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/driver.rs)
retain predecessor order where possible and reconcile joins. Its
[spill discovery](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/builder.rs)
and [final storage allocation](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/isa/evm/machine/final_spills.rs)
are separate responsibilities, including simultaneous phi interference.

## Consequences for our scheduler

Our existing scheduler already carries SSA identities privately, consumes last
uses, protects caller prefixes and emits checked physical DUP/SWAP/POP. It also
tries selected entry and operand orders. Reimplementing these features would
add little. The larger limitation is its broad spill decision after one pressure failure
and bounded resident exemptions. Selected Phi homes can now be retired through
the checked mixed-edge emitter; other homes still generate memory traffic and
protection against source writers.

The first bounded experiment defers exactly one additional entry word to the
existing pressure simulation. A 17-value top-down reduction needs no DUP17;
rejecting it solely for its entry height needlessly allocates homes. Actual
inaccessible duplications, edge permutations, call protocols and transient
peaks must still reject the stack-only plan. Deeper entries remain conservative
in this experiment. This is a candidate under measurement, not an accepted
performance or correctness claim.

The next larger direction is failure-directed spill selection with a bounded
worklist and coordinated phi-edge interfaces. Another is safely shortening
expression live ranges before physical scheduling. Keep target stack layouts
private to the scheduler; source-aware SSA optimization stays in MIR. Avoid an
unbounded permutation search or copying an upstream state machine wholesale.

Fewer homes can change FMP, MSIZE, source-memory observations, caller layouts
and outlining. It does not by itself fix the open arbitrary-memory readback
bug. Every candidate still needs identical-input UI, runtime, differential,
heavy size, hot gas and quiet compiler-time checks. No upstream memory-safe
assumption, recursion exclusion or aggregate win waives our existing contract.

Pinned file hashes and detailed independent reviews are retained under
`target/codegen-bench/evm-rewrite-candidate/stack-scheduler-prior-art-20260906/`
and `sonatina-stack-study-20260906/`. No upstream implementation was copied into
production. The optional historical EVM-LLVM paper could not be fetched; no
conclusion depends on its unavailable contents.


## September 7 follow-up

Refreshing all three repositories retained the same pinned revisions above.
Solx weights candidates from failed access windows; Venom preserves live operand
copies and prefers accessible non-operand spills; Sonatina distinguishes
rematerializable values from values worth caching and discovers spills through
monotone retries. These mechanisms support a bounded failure-directed allocation
experiment. Preserve mandatory Phi inputs/results, call and writer floors, and
existing home addresses initially; retrying a fixed number of pressure scans
still has a compiler-time cost that must be measured.

A smaller argument-cache trial has been rejected. It reused the existing local
operand replay, retaining repeated external arguments and restoring the exact
original exit stack. In the original calldata-alias fixture it activated the
replay but failed profitability: normalized cost rose from 24 to 27 gas while
remaining 11 bytes. Input, net and peak stack usage were equal; the rejection
was not a conservative pressure check. Focused None/Gas/Size outputs were
byte-identical to the baseline because the trial was never selected. Removing
raw redundant SWAPs cannot improve the already-normalized comparison. Both the
policy and diagnostic patches were removed using their retained reverse patches,
and the two source hashes match the pretrial baseline exactly.

This result motivates examining unsigned carry comparisons in MIR before adding
another scheduler policy: for wrapping `s = x + k`, `s < x` equals `s < k`.
Choosing a cheap constant can shorten the live range of `x`; whether it improves
emitted code remains an experiment. It does not solve broad homing or arbitrary
source-memory readback. Detailed refreshed sources, rejected patches and
measurements are retained in `stack-residency-prior-art-20260907/` and
`argument-residence-workflow-20260907/` beneath the candidate evidence directory.


## Bounded selective retirement

The failure-directed trial now reuses the existing residence constraints,
physical pressure simulation, last-use operand preparation and emission
checkpoint. It considers only single-use optional homes in no-Phi owners with
at most 256 allocated values, restores an old home from the first failed site's
conservative identity pool, and permits at most eight scans. Mandatory homes,
original residents, reservations, entry protocols and the accepted Phi path
remain intact. Actual emission rejects unsupported schedules; the existing
writer guard rejects increased protection cost. No upstream implementation was
copied, and no optimization moved into the assembler.

Two broader drafts were rejected: the first saved bytes but increased mutable
bank execution by 132 gas; allowing multi-use residents grew the checked-locals
fixture by 86 bytes through extra rotations. Reusing last-use preparation and
restricting new residents to single-use values removes both observed
regressions. This restriction is measured policy, not a universal profitability
proof: mixed live operands, repeated loop execution and canonical edges can
still cost more. Exact retry-boundary and late-rollback coverage remain open.

The final installed UI corpus has eight smaller Gas objects and no growth;
all Size objects are exact. The full runtime and archived-project corpus is
unchanged from the preceding candidate. Compiler-time geomean is 3.4615% slower
in the primary run and 0.5129% slower across four reversed-order repeats;
retaining these costs is necessary when comparing future proposals. Existing
sealed-baseline performance debt and arbitrary-memory correctness defects are
not resolved by fewer homes. Detailed measurements and rejected drafts are in
`target/codegen-bench/evm-rewrite-candidate/failure-directed-homes-workflow-20260907/`.


## Spill memory ownership

The September 7 audit checked the same three backend pins and separately pinned
solx frontend documentation at `c1f170e9c1d557058497a3b75c36e773cba7c8e2`.
That frontend pairs with a different LLVM revision from the scheduler study;
the two revisions are not treated as one tested toolchain.

Solx's LLVM backend requires an explicitly reserved spill region and rejects
frames that exceed it. Its frontend documents rejecting required spills around
memory-unsafe assembly, and separately excludes functional dependence on exact
MSIZE or gas. These are source contracts, not transparent preservation of
arbitrary memory. See the pinned [frame finalizer](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMFinalizeStackFrames.cpp#L172)
and [frontend limitations](https://github.com/NomicFoundation/solx/blob/c1f170e9c1d557058497a3b75c36e773cba7c8e2/docs/src/user-guide/04-limitations.md#L32).

Venom places each function's spills beyond all static frames and initializes
dynamic allocation above those regions. Vyper excludes inline assembly.
Together these support disjointness for compiler-generated accesses; they do
not establish safety for arbitrary hand-written addresses. See the pinned
[spiller](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/stack_spiller.py#L28),
[FMP setup](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/venom_to_assembly.py#L280),
and [language restriction](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/docs/solidity-differences.rst#L104).

Sonatina reserves known raw read and write ranges, but unknown ranges impose
a conservative allocation floor rather than proving isolation. Its later fixed
write collector omits dynamic ranges, and the inspected emitter uses physical
MSIZE. This establishes a limit of the inspected backend analysis, not a
confirmed Fe or Sonatina miscompile or a complete frontend contract. See the
pinned [memory preparation](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/isa/evm/prepare.rs#L280)
and [MSIZE emission](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/isa/evm/emit/insn.rs#L352).

The resulting design direction is explicit, proved spill permission at the
MIR-to-stack boundary. Without a disjoint region, use checked stack scheduling,
last-use consumption and rematerialization. Reads, both MCOPY ranges, return
data, hashes, logs, external-call buffers and internal callees all matter;
preserving only source writers cannot establish safety. MSIZE needs a separate
proof because disjoint spills can still expand physical memory. This is a
proposed interface, not an implemented fix. Rejecting previously compiled
inputs when stack scheduling fails would not satisfy the coverage goal.

Source hashes, copies and qualifications are in
`target/codegen-bench/evm-rewrite-candidate/arbitrary-memory-prior-art-20260907/`.
The home-192 counterexample is in `arbitrary-memory-spill-correctness-20260907/`
beneath the same candidate directory. No upstream implementation was copied,
and none of these source memory contracts is assumed to apply here.


## Removing literal frame transport

The memory-contract audit above informed a narrower implemented step: expose
literal frame words to the existing MIR memory analysis before stack scheduling.
This removes a transport load while leaving the semantic store available for
observable raw accesses and subsequent ordinary dead-store analysis. Frame
facts use unknown-region aliasing with exact physical base/offset; distinct
semantic region tags alone do not establish disjointness from raw memory.
No upstream implementation was copied.

Measurements constrain admission to literal values in acyclic functions.
Dynamic forwarding disrupted shared code and grew two Size objects; allowing
literal forwarding in loops added repeated stack rotations and regressed hot
gas despite equal bytecode size. The acyclic version shrinks 58 UI objects
and 19 archived-project objects without growth against its preceding candidate,
and preserves all official runtime labels. It adds 81 physical lines to the
existing MIR pass, with no extra pipeline invocation. Its primary compiler-time
geomean is 2.1467% slower; a noisy reversed-order repeat does not establish a
speedup. This is a measured transport optimization, not general frame-to-SSA
promotion, a proof of spill-memory ownership, or closure of sealed size debt.
See `semantic-frame-forwarding-workflow-20260907/` beneath the candidate evidence
directory for the rejected variants and identical-input comparisons.


A subsequent one-condition trial skipped private SWAPs whose two identities
were equal. It needed no search and preserved every private stack state, but
was rejected: later physical normalization starts with distinct incoming
identities and can choose a longer final permutation after that omission.
Twenty-nine UI contract/mode outputs each grew one creation/runtime byte,
despite aggregate savings. The source was restored. This reinforces the
prior-art distinction between private value-aware scheduling and physical
stack normalization: a local reduction is not an end-to-end performance proof.
The bounded models, frozen candidate and counterexamples are retained in
`top-first-permutation-study-20260907/` beneath the candidate evidence directory.


## Retaining a homed writer address

The next experiment applies the distinction between assigning a spill home and
reloading from it. The pinned solx solver explicitly prices an accessible copy
of a spilled value as DUP instead of PUSH/MLOAD. Venom's operand preservation
and Sonatina's separation of cached identities from final spill storage support
the same direction. These are algorithm references; no implementation was
copied, and their memory-region assumptions do not establish ours.

A fresh Router capture proves none of the 137 bitmap writer templates eligible
for one-word specialization under checked literal/arithmetic low-bit analysis.
The 127 mapped owners have unknown alignment; ten moved owners remain unmapped. The thirteen writes through its sole
reserved allocation do not own these templates. Assuming alignment from the
free-memory pointer would therefore invent a missing contract.

The current scheduler experiment instead retains the immediately produced writer
address above an already frozen stack prefix. It keeps the absolute home store,
all writer protection and the original storage plan. DUP1 and SWAP1 replace
PUSH/MLOAD at equal local gas; an existing zero-home PUSH0 reload is excluded.
The fixed-prefix and unchanged-preparation guards exclude operand interference
without another search or pressure-analysis pass. Production source provenance,
91 actual Router sites, source-map relocation checks and the pending full
acceptance lanes are retained in `writer-operand-cache-workflow-20260907/`.
The current 182-byte Router reduction is an intermediate result, not whole-
rewrite or runtime-performance acceptance.


The retained-address experiment is now committed as `8c52191f`, with final
full/Size runtime gas unchanged and strict whole-project size improvements.
The final benchmark records a 0.99% compiler-time increase, not a speedup.
Detailed source/metadata, runtime and repeated timing receipts are in the
progress record and the retained workflow directory.

A following caller-prefix investigation found no immediate reusable homed copy
at an ICall. Exact substitution into an existing opaque save prefix would
preserve the callee peak even through recursion; absent transitive peak analysis
is not the rejection reason. Current lowering consumes or excludes those copies
before a spilled call. Of Router's eight calls, four begin their block, three
follow source writers, and one follows a dying CALLER argument with no backup.
Crossing those writers needs a separate proof. No call policy was changed.
The exact continuation count is 27 restored caller homes plus one returned-
result store, not 28 caller backups. The source pins and negative census are
retained in `caller-prefix-static-study-20260907/`.


## Terminal transport follow-up

The terminal-tail experiment applies the shared prior-art direction of preserving
SSA values through private stack interfaces. Our MIR already carries the arguments;
the removable stores are introduced at the scheduling boundary. The existing entry
reconciliation now handles a bounded tail-call closure, with a cached certificate
that every descendant observation misses each omitted ancestor frame interval.
This is a local design within our memory contract, informed by the pinned solx,
Venom and Sonatina studies above.

The accepted implementation adds no pass or permutation search. Cache admission
needed its own performance regression test: a nearly exhausted parent initially
poisoned a later terminal-leaf request. Exempting terminal leaves from expansion
budget preserves old admission and bounds recursive work separately from their
once-per-function source scan. `3d6dc3c4` and `5661a94c` retain the implementation,
measured output gains, ancestor/observer controls, and the rejected-draft witness.


## Joint residence and materialization order

The pinned solx commutable-input handling, Venom's dry-run operand orders, and
Sonatina's separation of retained values from rematerialization motivate choosing
these two decisions together. A cached argument with canonical materialization
previously cost extra shuffles. Loading it before a commutative producer's literal
instead allows DUP/SWAP to replace its second calldata read at equal gas.

`b2b6db45` implements one bounded two-instruction candidate in the private
scheduler, preserving the exact exit identities and checking complete normalized
gas, bytes and peak. It keeps the established replay prefix guards. The measured
UI and project size improvements survive those later normalization/sharing
interactions; no upstream memory-region assumption or implementation is imported.
The reversed compiler-time result is 0.45% slower, so this is an output-quality
improvement without a compiler-speed claim. Pinned primary-source links, the
378-case stack model and rejected alternatives remain in
`repeated-argument-carry-study-20260907/` beneath the candidate evidence directory.


## Restoring homes in stack order

The same pinned private-stack contracts support a smaller physical scheduling
change in `eba00ed7`: after a protected source store, restore the two saved homes
in the order already convenient on the stack. The six-word permutation needs
three swaps instead of seven. Both originals are loaded first, and their aligned
homes are disjoint or identical, so reversing these two stores preserves memory.
No upstream implementation or new memory-ownership assumption is imported.

The change removes eleven backend lines and improves generated gas and size
without adding an analysis or pass. Cheaper templates also affect existing
protection choices, which were reviewed separately against their original
guards and actual emitted regions. Compiler timing remains mixed; the fresh
symbolic attempt is incomplete. Full measurements, source/reference proofs,
executed fragments and the unresolved whole-rewrite debts are retained in the
progress record and `writer-restore-order-workflow-20260908/` evidence directory.


## Immutable recipes and the comparison budget

The pinned solx expression pass distinguishes moving one-use work from
duplicating cheap expressions; its special calldata/ADD cases explicitly warn
about code growth. Venom preserves dependency ordering during single-use
expansion, and the inspected Sonatina rescue path rematerializes immediates.
These sources do not establish a general multiplication recipe policy.

The current MUL investigation remains unimplemented: Router has no matching
fixed-calldata/literal roots, while the retained writer fixtures have nonrecipe
homes or internal calls that reject the complete bank. A one-use MUL would
move its five-gas operation once, but shared reloads and wider literals still
need measurement. Existing computed-recipe admission does not globally exclude
GAS, MSIZE or source-memory reads. The independent source and contract audits
are in `mul-recipe-static-study-20260908/` and
`mul-recipe-independent-20260908/` beneath the candidate evidence directory.

The earlier rejection of ordinary sharing solely for losing an intermediate
version's gas gains imposed a stronger rule than the handoff. Final acceptance
compares each matched call and object against sealed revision `9cb036c`; keep
intermediate deltas visible without treating each best-ever result as a separate
requirement. The historical reports remain intact, with the explicit correction
in `global-alias-policy-review-20260908/`. This permits investigating ordinary
sharing within the original joint gas/size budget; it does not waive any sealed
regression, unmatched input, or runtime mismatch.


## Joint ordinary and Phi residence

The September 8 live-segment trials exposed competition between two optional
selection stages. Seven extra ordinary residents displaced nine Phi residents
in a loop fixture, while a larger storage Phi proposal failed writer cost and
lost every previously accepted retirement. More precise liveness did not imply
cheaper transport. All three placement variants were rejected and the accepted
compiler was restored; their measurements are in the progress record.

At the pinned solx revision,
[spill weights](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMStackSolver.cpp#L157)
account for loop uses, and
[common-stack selection](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMStackSolver.cpp#L856)
prices transformations to both successors. Venom's
[edge liveness](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/analysis/liveness.py#L96)
combines matching Phi operands with ordinary live values; its
[Phi emission](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/venom_to_assembly.py#L739)
can rename an incoming stack identity, duplicating it when the old identity
remains live. Sonatina's
[block templates](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/templates.rs#L48)
include Phi results and ordinary carry under one spill set, with joint
[monotone spill discovery](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/builder.rs#L375).
None establishes a globally optimal allocator or our source-memory contract.

These mechanisms motivate one optional pool after stable home allocation,
replacing the competing ordinary and Phi stages. Cyclic uses and incoming-edge
transfers could inform its rank; cyclic membership is only a heuristic, not a
frequency estimate. Writer profitability needs the complete saved-address set:
a single removed home can make its protection template more expensive. A small
shared admission interface to the existing writer chooser should reject that
removal while preserving accepted choices, with actual emission still checking
the final plan. Duplicating alias/availability analysis or adding repeated
emission trials would add compilation cost and complexity without addressing
the accepted-but-worse loop allocation. This is a proposed diagnostic, not an
implemented or measured improvement. Exact source pins, native witnesses and
interface limits remain in `joint-residence-prior-art-20260908/` beneath the
candidate evidence directory. No upstream implementation was copied.


## Wide constants and scheduler compilation cost

The pinned solx stack solver
[retains wide literals needed earlier in backward propagation](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMStackSolver.cpp#L499),
using immediate-width thresholds of eight bytes normally and four in Size.
Sonatina's production builder enables
[block-local immediate caching](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/builder.rs#L541):
four uses and a 17-byte materialization plan normally, three uses and three bytes
in Size. It canonicalizes numeric aliases and keeps this cache separate from
SSA live-out residence. These are reference policies, not tuned thresholds for
this codebase. Venom's inspected
[operand emitter](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/venom_to_assembly.py#L407)
repushes literals and tries local operand orientations; this narrow observation
does not describe every later optimization.

Their compilation budgets are also useful references. solx computes spill weights
lazily, although failed propagation still retries. Venom limits this ordering
choice to individual operand permutations. Sonatina uses bounded local searches,
[query caches and early exits](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/stackalloc/stackify/planner/operand_prep.rs#L145),
but deliberately leaves unary outer queries uncached when key construction would
cost more than it saves. None establishes that repeated full-block alternatives
are cheap. Our materialized-operand milestone improves output while increasing
measured compilation time; a separate exact-duplicate replay gate is under test.

The reduced entry-order fixture contains 47 modulus-related `PUSH28; NOT` plans,
occupying 1,410 of 1,793 Gas runtime bytes. Its ordinary path executes 27 plans
for 162 gas. Sealed bytecode uses a slower outlined construction and more memory
transport, so restoring its shape would sacrifice current runtime quality.
The next artifact experiment retains one repeated modulus inside an already
chosen arithmetic block, paying every deeper access, duplicate and final disposal.
Each replaced plan leaves only three gas for extra shuffles. The incoming winner,
exact outgoing stack and peak must be preserved. No constant-cache implementation
or savings claim follows from the byte census alone. Exact source pins, traces,
limits and the proposed witness are in
`target/codegen-bench/evm-rewrite-candidate/wide-constant-prior-art-20260908/`.


The bounded native witness now reduces the reduced fixture's runtime from
1,793 to 853 bytes. Ordinary/doubling execution falls from 995/1,103 to
980/1,070 gas; all twelve fresh calls across witness, current compiler and solc
pass the four original tuple oracles. A static whole-module capacity proof and
native traces retain maximum stack height 14 and 13 memory words. The third
arithmetic block's cache adds 21 fixed gas and is rejected. The two chosen blocks
still leave 20 bytes of sealed size debt. This is modified emitted-IR evidence,
not a production compiler policy: baseline native IR replay was byte-exact,
and the witness used a separately identified deployment wrapper.

The existing `compact-pushes` adapter already computes physical stack facts and
observer permission. Its absolute incoming bounds can allow the extra local slot
that the successful witness needs; unknown prefixes retain the nonincreasing-peak
rule. This avoids another MIR scheduling replay or caller-analysis framework.
Integration must confirm the actual compiler analysis grants that permission,
retain the selected prefix and exact exit, and charge the complete literal plan,
all duplicates and all setup/release swaps. Models, rejected variants, exact
native traces and source pins are in `wide-constant-prior-art-20260908/boundary/`.


## Eager consumer scheduling

The September 8 experiment distinguishes dependency legality from the choice
of schedule. The pinned solx single-use pass permits data-only motion but sinks
definitions; Venom's DFT separates data and effect dependencies; Sonatina's use
tracker consumes dying values while its block planner walks existing IR order.
None of those inspected paths implements this exact earlier-consumer rule.
The focused primary-source audit is retained in
`target/codegen-bench/evm-rewrite-candidate/raw-memory-residence-revisit-20260908/prior-art/report.md`.

The accepted rule requires two distinct dying inputs, a pressured block and
canonical pure operations. It retains captured mutable reads instead of issuing
them again after writes. Neutral one-input motion and low-pressure motion were
rejected after native code-size regressions. The linear implementation avoids
an effect graph or scheduling search, but its measured compiler-time cost still
requires attention. Fewer spills are not a general proof of source-memory
ownership; that contract remains separate from scheduling profitability.


## Phi-home retirement and writer protection

The September 8 native trace shows a twelve-to-eleven-home bank transition
blocking an otherwise attempted Phi-residence trial. Pinned solx stack-solver
costs distinguish DUP from PUSH/MLOAD transport; Venom transfers Phi identities
through edge liveness; Sonatina coordinates Phi and carry interfaces while
monotonically discovering spills. These are relevant design constraints, but
none of the inspected paths supplies our bitmap protection template or its
minimum profitable bank size.

Our proposed nine-home contiguous and eleven-home bitmap floors follow the
actual template gas costs, with the existing exact-cost and capacity checks
remaining authoritative. Retiring address 7840 leaves its bitmap bit unset;
restoring an obsolete home merely to preserve the old bank is invalid. The
template cost is 55 bytes / 130 gas for the remaining eleven homes,
versus the currently selected 89 bytes / 135 gas. This is a local cost witness,
not proof that the complete Phi trial succeeds or that corpus output improves.
Exact primary paths, pinned commits and hashes are in
`target/codegen-bench/evm-rewrite-candidate/writer-profitable-bank-floor-proposal-20260908/prior-art-evidence.json`.


The broader nine-home contiguous trial was rejected after actual nested calldata
execution grew by 443 gas and ERC7579UtilsTest grew by 29 bytes. Keeping the
contiguous floor at twelve isolated the eleven-home bitmap, but exposed the
same nested regression and a 229-byte MatchAdvancedOrder increase. Both failed
candidates and their unchanged-input comparisons remain retained.

The native veto trace then identified a separate policy error: retiring one of
eleven homes changed writer protection from 55 bytes / 130 gas to 75 bytes /
123 gas. A bytes-or-gas veto rejected that lower-gas Phi schedule. The accepted
Gas-only comparison orders gas first and bytes second. It retains all ownership,
capacity, actual lowering and rollback checks, and adds no scheduling search.
Its local estimate excludes surrounding stack preparation, edge transport and
later outlining, so whole-output measurements remain necessary.

This rule cures both observed corpus increases. All 3,344 heavy objects join by
identity; 277 shrink and none grow. All 175 hot-gas labels remain exact in both
optimization modes. The measured full-workflow compiler average is 4.18% higher;
baseline contention and 54.26 seconds of candidate parser overlap prevent a
causal timing conclusion. The exact source pins, rejected trials, native veto,
metadata review and raw measurements are retained in
`target/codegen-bench/evm-rewrite-candidate/writer-bank-floor-workflow-20260908/`.
These results validate our bounded rule, not an algorithm supplied by solx,
Venom or Sonatina, and do not establish complete rewrite acceptance.


## Checked arithmetic tail sharing

The pinned [solx post-stackification branch folder](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMBranchFolder.cpp#L60)
disables common-code hoisting because implicit stack operands constrain motion.
[Venom terminal merging](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/passes/tail_merge.py#L29)
refuses Phi and nonlocal variable inputs.
[Sonatina suffix grouping](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/isa/evm/late_block_merge.rs#L768)
charges every incoming transfer and its shared marker, but requires a closed
stack suffix. These are useful constraints, not implementations of our live-input
checked-add witness or proofs of its profitability.

A current-only physical witness shrinks the alias fixture from 210 to 153 Gas
runtime bytes, matching sealed. All 21 unchanged call labels pass within sealed
gas limits. It preserves validation and uses the unsigned identity that
`sum < x` equals `sum < k` for `sum = (x + k) mod 2^256`. Sharing the literal-first
arithmetic continuation, preserving the last donor's fallthrough and reusing the
default return are all necessary. This is edited emitted IR with explicit wrapper
provenance, not accepted source compiler output or a metadata proof.

The isolated carry orientation ties current Gas output cost. Existing Size
sharing on that Gas IR yields 179 bytes versus a matched 187-byte control, still
larger than current source Size output at 166 bytes. The medium-tail collector
forms pairs despite a potentially profitable larger group. A conditional-only
group-admission trial preserves all 5,048 complete UI objects across 1,654 matched
IDs and 828 source hashes, with no source-corpus benefit. Its production patch
is therefore reverted and retained as an experiment. The broader proposal was
held before compilation because it re-admitted fixed-exit groups already shown
to grow bytecode. Neither proposal delivers the complete 153-byte witness.
The conditional trial and independent audit are retained under
`target/codegen-bench/evm-rewrite-candidate/conditional-medium-tail-workflow-20260908/`.
Exact primary-source pins, refusals and fresh native calls are retained under
`target/codegen-bench/evm-rewrite-candidate/alias-current-tail-witness-20260908/`.

The conditional collector plus the oriented alias IR reaches 155 Size bytes,
but one overflow label uses 218 gas against sealed 215. All 84 fresh comparison
calls return the expected status and data; the gas regression still rejects
this composition. The independent payload fixture shrinks 99 to 84 Size bytes
with unchanged gas across 32 fresh baseline/candidate calls. These are emitted-IR
experiments, not restored source-level functionality.

A separate entry-residence investigation finds three protected homes in
`SuggestedActionHelper.encode_abi_array.33`. Arguments, reused writer operands
and a Phi source each have independent residence restrictions. Existing caller
MIR shows the input originates from an FMP allocation, but the call-result
interface carries no pointer bound that the callee's alias analysis can use.
An overlapping-input counterexample prevents removing these restrictions
indiscriminately. The bounded stack model and missing proof are retained under
`target/codegen-bench/evm-rewrite-candidate/entry-writer-residence-investigation-20260908/`;
its modeled savings are not generated-code measurements.

## Unit-increment carry tests

The unsigned identity `sum = x + 1; sum < x` equals `sum == 0` for wrapping
256-bit addition. An early MIR trial removes the old comparison operand, but
changes scheduling and branch polarity: 313 UI objects grow despite aggregate
size reductions. A minimal case saves three arithmetic bytes and adds four
transfer/marker bytes. Its 162 focused runtime calls pass; that does not waive
the corpus regressions. The MIR change is reverted and its evidence remains in
`target/codegen-bench/evm-rewrite-candidate/unit-carry-workflow-20260908/`.

A separate physical peephole recognizes two exact six-instruction schedules
and reduces them to `push 1; add; dup1; iszero`. It preserves the opaque stack
prefix, wrapped sum and carry result, lowers peak depth, and leaves scheduler
cost trials unchanged. This follows the placement constraints in the pinned
solx, Venom and Sonatina reviews above: a valid algebraic identity still needs
an affordable physical schedule and preserved control-flow behavior.
Including the new checked-add fixture, the candidate shrinks 42 of 5,052 matched
UI objects, with no growth or same-length changes. Gas creation/runtime totals
fall 50/48 bytes; all Size objects and all 3,344 heavy objects remain exact.
All 318 focused runtime calls pass without gas increases. Full and Size workflows
preserve all 175 labels, and Foundry preserves 1,537 test records. The workspace
passes 11,757 UI cases and 1,557 other tests; the unchanged original alias
assertion still fails and two tests remain skipped.

The final Rust match spells out both complete tuple alternatives. Actual compiler
assembly rejects irrelevant first instructions before examining the suffix;
this is a dispatch observation, not a measured speedup. A quiet six-leg comparison
uses baseline/original/final/final/original/baseline order, with two samples per
build and project. Final mean compiler times are 4.03% lower on v4 and 2.30%
lower on Solmate than the accepted baseline, but 0.59% and 2.23% higher than the
original peephole form. Earlier positive timing deltas remain retained. This
repeat establishes neither a causal speedup nor full compiler-time acceptance.

The final producer independently repeats UI, full runtime, Size and Foundry
checks. Its complete heavy-output fingerprints join retained original-producer
raw JSON captures; no fresh final-producer heavy raw capture is claimed. Two
debug snapshots change only after actual source, bytecode, source-map and event
reviews; original source and FileCheck assertions remain intact. The physical
rule adds 30 production-section lines and no scheduling analysis or assembler
logic. Artifacts, producer hashes, bounded symbolic comparison and timing
limitations are retained in
`target/codegen-bench/evm-rewrite-candidate/unit-carry-physical-workflow-20260908/`.
This bounded improvement leaves the rewrite's alias and sealed size debts open.

## Returned pointers and spill ownership

The follow-up review distinguishes preserving an allocation fact from inventing
one. [solx's LLVM return-attribute inference](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Transforms/IPO/FunctionAttrs.cpp#L1445)
requires allocation/noalias-call roots and capture checks; arbitrary loads fail.
This review does not establish that a selected solx pipeline invokes the generic
LLVM inference. Its reserved frame region is a separate memory contract.
[Venom memory locations](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/memory_location.py#L120)
retain explicit allocation identities, but concrete and abstract locations can
still alias. Its caller return-buffer operand exposes an existing destination.
[Sonatina call-result provenance](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/isa/evm/ptr_provenance.rs#L269)
substitutes returned arguments; a possible non-argument return stays unknown.

Our mapped `toOrders` returns originate in `mload(64)`, not a returned argument
or a retained MIR allocation. None of these mechanisms proves their separation
from the two omitted argument-home intervals. A small exact returned-argument
summary could carry a caller's existing allocation, but would not activate this
case or refine every incoming context of the shared callee. The native boundary
diagnostic now confirms that the actual returned value is an explicit allocation:
it survives memory-object lowering and coalescing, then the same SSA result
becomes `mload(64)` at `lower-alloc`. All three complete ordinary/dump outputs
match with frozen producer `4eb07937`. Public MIR controls distinguish an
allocation forwarded through an identity call, a fresh allocation return, a raw
FMP return and a mixed argument/unknown return. They are lowering checks, not
runtime or noalias proofs. Preserving the local fact and exporting its lifetime
and physical range across all relevant calls remain separate obligations. No
residence floor is relaxed on the strength of source shape.
Eleven primary files, exact revisions, current interfaces and the diagnostic
proposal are pinned under
`target/codegen-bench/evm-rewrite-candidate/return-provenance-prior-art-20260908/`.
This research establishes constraints, not a generated-code performance result.

A separate terminal-anchor check confirms an existing return block and duplicate
suffix in the retained 155-byte oriented alias artifact. Its conservative
203-byte envelope proves one-byte label immediates suffice; it does not fix the
independent overflow gas debt. Current source Size output at 166 bytes already
shares that return and has no eligible donor, so this isolated rule has no
measured source benefit. Three exact native captures and the bound are retained
under `target/codegen-bench/evm-rewrite-candidate/terminal-anchor-bound-20260908/`.
No terminal rule is added on the strength of the edited-IR witness alone.

## Preserving the lowered operation's effect

The allocation-boundary diagnostic exposed a smaller scheduling obstacle:
`lower-alloc` changed an allocation into an FMP load while retaining its default
`memory_write` classification. Late eager contraction treated that load as a
hard boundary. The candidate rebases an explicit old-default effect to the new
operation's default, preserving absent and unequal custom overrides. Allocation
checks, FMP stores, initialization and other metadata remain in the expansion.
An explicitly written classification equal to the old default also rebases; it
is indistinguishable from the builder's default. Effect classification governs
optimization permission, so this is not merely a debug-formatting cleanup.

The actual mapped Suggested loads remain ineligible for useful motion. A smaller
Solidity control instead keeps seventeen independent calldata values live across
a Cell allocation. Observing the allocated extent prevents static deferral.
Rebasing the load permits the existing pure checksum consumers to contract before
the allocation, reducing runtime code 194 to 150 Gas bytes and 205 to 150 Size
bytes. Three exact vectors under both modes and four producers give 24 fresh
calls: candidate execution costs 330 gas versus current 456/489 and sealed 466.
All returndata agrees with mathematical and solc oracles. These are measured
executions, not a general stack-depth or arbitrary-input theorem.

The custom-effect control remains unchanged; erasing its override makes the
refusal FileCheck fail. The old compiler fails the positive ordering check.
The initial Solidity source becomes a deferred allocation and remains byte-exact.
The installed standard matrix passes after correcting tuple syntax in two new
run-call directives; the failed setup is retained and no oracle value changes.
The bounded solsymdiff result agrees over symbolic inputs within the recorded
580-byte calldata, 96-byte output, path, query, depth and timeout limits. Actual
exploration counts are unavailable in the compact result.

All 5,052 existing UI objects and 3,344 heavy objects remain exact. Both runtime
workflows preserve 175 labels and 139 observations; Foundry preserves 1,537
records and 244 size fields. Thirteen existing snapshots change only 94 obsolete
effect annotations; their original source and FileCheck assertions remain intact.
The source correction adds nine MIR production-section lines, no analysis or
scheduling search, and does not solve interprocedural allocation provenance.
Eight paired debug captures preserve complete creation/runtime objects and ABI.
All sixteen moved checksum ADDs retain exact source ranges; their execution
order legitimately changes. This fixture has no invocation/return events, so
populated-event preservation is outside that check.

The first quiet ABBA comparison measured a Seaport compile-time increase from
73.332 to 74.906 seconds (+2.15%), with disjoint two-sample ranges. It remains
an acceptance concern. A serial `-Ztime-passes` diagnostic preserves all 39,324
pass identities and changed flags and byte-identical complete output, but does
not identify the cause: the MIR scheduler totals 1.019 versus 1.020 seconds.
This flag disables parallel contract lowering, so its wall times cannot replace
the ordinary workflow measurements.

A separate simplification combines the shared-producer and effect-boundary scans
and removes 38 MIR lines. Independent review establishes the same nontrivial
islands, consumer order and scratch transitions. Frozen candidate `4fc59cdb`
preserves all 5,056 current UI objects, including the allocation control; its
workspace run passes 11,762 UI cases with only the original alias failure.
Both complete workflow artifact sets, including serialized MIR, remain exact
to the preceding effect candidate. The new quiet ABBA means are Seaport +0.07%,
v4 +0.81% and Solmate +2.49% against the accepted unit-carry baseline. Seaport
and Solmate ranges overlap; v4 ranges are disjoint. Two samples per leg do not
establish a scheduler speedup or explain the earlier Seaport increase. These
compiler costs remain visible tradeoffs behind the generated-code improvement.
The two changes are separate commits, `dccc50b9` and `d59f07c0`, with a net
29-line reduction in MIR production sections. Full evidence remains under
`target/codegen-bench/evm-rewrite-candidate/lower-alloc-effect-workflow-20260908/single-scan/`.

The separate provenance design identifies a conditional returned-pointer lower
bound as a smaller useful fact than exporting a callee-local allocation ID.
It still requires a valid FMP floor in every incoming context and separate
residence/returning-entry proofs. None is inferred from the allocation tag alone.
The uncompiled reduced-source proposal and estimated implementation scope are
retained in `return-provenance-prior-art-20260908/design-followup/`; no optimizer
or residence relaxation is implemented from this static design.

## Calldata carry specialization after sharing

The physical unit-carry rule also applies when the same literal calldata offset
is read again for the overflow comparison. Keeping the first four instructions
and replacing the reload/compare tail with `dup1; iszero` preserves the arbitrary
stack prefix, sum and carry. Calldata is immutable; required inputs stay zero,
net height stays two, and peak falls from three to two. Savings are six gas for
an ordinary offset PUSH, or five with PUSH0. Mutable reads and observations stay
outside the rule.

Applying this early made two Size contracts larger: Linear by eight bytes and
cross-block nullary rematerialization by ninety. The cheaper local sequence lost
sharing opportunities. The revised `late-dce` configuration reuses the existing
final DCE traversal after Size TailMerge/Outline and enables only this new rule
there. Gas keeps the earlier placement. The older six-op carry rule and scheduler
cost trials are unchanged. All four growth objects recover exact baseline bytes.

The strongest pinned analogy is [solx's late target pipeline](https://github.com/NomicFoundation/solx-llvm/blob/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a/llvm/lib/Target/EVM/EVMTargetMachine.cpp#L333):
branch folding and tail duplication precede late unfolding and peepholes.
[Venom's Size sequence](https://github.com/vyperlang/vyper/blob/6dd5fef7ce71bb9b363ceb94df94080d451f4236/vyper/venom/optimization_levels/Os.py#L101)
places literal specialization after CSE, but does not establish the same outlining
policy. [Sonatina's late section merging](https://github.com/fe-lang/sonatina/blob/8e6c99f67cf3f20b9672cab61d8655c2ff33a6a7/crates/codegen/src/isa/evm/backend.rs#L343)
shows that physical shape affects sharing; it is not direct precedent for this
ordering. Our phase choice follows the measured regression, with later layout
and deduplication still checked against complete native outputs.

The combined UI comparison has 1,660 IDs and 5,060 objects: 66 shrink and 4,994
remain byte-exact. Gas creation/runtime totals each fall 39 bytes; Size falls
98/95. The new source returns increment and carry in 47 runtime bytes instead
of 50, costing 109 rather than 115 gas. Sixty-two fresh focused calls agree;
all 175 corpus gas labels, serialized artifacts and 1,537 Foundry records remain
exact. The symbolic comparison agrees within its recorded bounds; its 47-byte
executable prefix is joined separately from its 14-byte metadata trailer.
The inherited debug policy unions the addition and comparison origins and emits
an unknown legacy source-map range for that union; paired captures verify this
policy and byte neutrality. The first quiet late-candidate ABBA measured
Seaport +8.17% compiler time with disjoint ranges. That concern remains open.
Native inspection finds the main peephole body grows from 28,499 to 31,798
bytes and its local frame from 8,976 to 9,888 bytes; this is a lead, not a
causal timing result. A separate helper experiment reduces the main body to
29,183 bytes and its frame to 9,056, while all 5,060 UI objects remain exact.
The helper holds only suffix validation and replacement construction, called
after exact prefix admission. A contemporaneous six-leg comparison measures
Seaport +3.20% versus the allocation baseline and -1.17% versus the larger late
matcher; v4 and Solmate are -3.56% and -1.25% versus baseline. Two samples per
producer do not establish neutrality or a causal improvement. We retain the
compiler-cost concern alongside the output-quality gains.

Paired 5 kHz profiles of the identical full Seaport input preserve complete
output fingerprints. Native-symbol joins find peephole inclusive CPU nearly
unchanged (7.166 versus 7.148 seconds), while stack analysis accounts for a
larger share. Opcode stack-effect lookup alone consumes 3.39%/3.70% of compiler
samples. These profiles guide the next investigation; profiled elapsed times
are not acceptance timings. One baseline CPU-delta sample spans 5.665 seconds,
so sample counts are primary and the anomaly remains recorded. Raw profiles,
commands, source pins and independent audits are retained under
`target/codegen-bench/evm-rewrite-candidate/rematerialized-unit-carry-workflow-20260908/outlined/`.

Fresh replay of the original alias source uses exactly 21 labels and three
frozen producers, with 63 independent deployments. Late Size runtime is 163
bytes versus current 166 and sealed 153. Three account-one labels save six gas;
overflow is 200 versus current 206 and sealed 215. The remaining eighteen
labels are exact to current. Twenty are below sealed and one equal. This
removes no test and leaves ten bytes of sealed size debt; it is separate from
the rejected conditional-sharing policy whose overflow cost was 218.

A separate five-call provenance diagnostic proves that the initial FMP floor
14368 survives all 25 indexed decoder writes. The first missing fact was the
nested-loaded base of `mustUseMatch`'s indirect `mstore v172, v13`. The follow-up
checks all 278 preceding writes and identifies that base as the initialized
allocation `v804`. Fresh nonzero elements make all six null-materialization
branches unreachable, including later loop iterations. This closes that helper
contextually. The next `getStructure.12` call also preserves the original
content graph and FMP floor on returning paths, after checking fresh-copy
bounds, encoder cursor bounds and output buffers separately. A 128-byte output
exceeds its 64-byte reservation but misses the original graph and FMP; terminal
scratch copies can overwrite FMP but revert. This is not an unconditional
readonly summary, and the later paths to `toOrders` remain unreviewed. An allocation-backed memory-content
analysis across calls is needed; an Arg floor or returned-allocation tag alone
cannot express this proof. No frame store is removed from this manual analysis.
Exact paths and the corrected infallible-allocation contract are retained in
`return-provenance-prior-art-20260908/call-facts/`, `content-chain/` and
`structure-chain/`.

## Profile-guided opcode lookup

The paired carry profiles identify opcode stack-effect lookup as roughly
3.4–3.7% of compiler CPU samples. A declaration-derived table preserves every
opcode result without adding an analysis or changing IR. The first Rust const
array form copied 768 bytes per lookup in debug assembly, despite its smaller
function body. A borrowed promoted constant removes that copy; exhaustive
256-value execution and native assembly verify both claims. This is a local
implementation correction, not an algorithm attributed to the upstream projects.

The corrected build retains all 5,060 UI objects and improves quiet ABBA means
against the carry checkpoint by 2.37%/6.67%/3.81% for Seaport/v4/Solmate. Two
samples per producer and project remain the explicit limit. The earlier timing
concerns and rejected table form are preserved under
`opcode-stack-table-workflow-20260908/`; this is not a cross-batch neutrality claim.
