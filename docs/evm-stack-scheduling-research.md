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
