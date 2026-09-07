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
