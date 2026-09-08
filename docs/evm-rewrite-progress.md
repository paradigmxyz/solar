# EVM rewrite progress

The rewrite remains incomplete. One original UI assertion, incomplete general
memory-ownership evidence and individual sealed gas/size debts still block final
acceptance.
This page summarizes the local allocation-effect milestone on 2026-09-08. The
[handoff](evm-rewrite-plan.md) defines acceptance; [PR #1388][pr] tracks review.
The [checkpoint archive](evm-rewrite-checkpoints.md) preserves the complete
history, baseline hashes, rejected trials and measurement limitations.

## Accepted local state

Main `becd2143` was integrated by merge `e40b84f0`. Recent commits preserve
cheaper Phi writer schedules (`e34bb6e6`) and simplify physical unit-add carry
(`8bf5b0d0`, tests `e9c7f759`, reviewed debug goldens `92fa93ee`). Earlier accepted
literal caching, tail grouping and eager SSA contraction remain in place.

MIR semantics, private stack scheduling, physical block IR and primitive assembly
remain separate. Literal caching uses existing capacity and observer facts.
Tail grouping stays in physical block IR and independently checks each member's
transfer capacity, split boundary and destinations. Neither changes memory homes.
[Prior-art research](evm-stack-scheduling-research.md) records pinned solx, Venom
and Sonatina sources and the limits of their applicability.

## Allocation-effect milestone

`d59f07c0` rebases the old native allocation effect when its result becomes an
FMP load. Absent and unequal custom effects stay intact; writes remain explicit.
The existing eager contraction can then consume live values across that read.
`dccc50b9` independently combines scheduling-boundary scans without changing
schedules. Together they remove 29 MIR production-section lines. The new matrix
and refusal tests are in `ff8222cf`; `f84255d6` updates 13 reviewed snapshots,
removing only 94 stale effect annotations. No existing source or check changes.

The focused checksum falls from 194/205 Gas/Size runtime bytes to 150 and from
456/489 execution gas to 330. Against the sealed executable it saves 59 bytes
and 136 gas in either mode. Three vectors, both modes and four actual producers
agree in 24 fresh calls; bounded symbolic agreement and exact source-map checks
are retained. The final scan simplification preserves that candidate's objects.

All 5,052 preceding UI objects and 3,344 heavy objects are exact. The expanded
UI corpus has 5,056 objects, with only the four added case objects shrinking.
Full and Size preserve all 175 gas labels and 139 observations per compiler;
Foundry preserves 1,537 records and 244 sizes. Final serialized MIR and physical
artifacts are exact to the preceding effect candidate. Workspace has 11,762 UI
passes and the original alias failure, plus 1,557 other passes and two skips.
Clippy and formatting pass. This remains a local milestone, not green CI.

The first effect-only quiet ABBA measured Seaport +2.15% compiler time with
disjoint ranges. Final combined means are Seaport +0.07%, v4 +0.81% and Solmate
+2.49%; v4 ranges are disjoint and the other two overlap. Two samples per leg
do not establish a compiler speedup or explain the earlier slowdown. A serial
pass-timing diagnostic preserves complete output and all pass identities but
changes parallel execution, so it does not replace these normal measurements.
These compiler-time tradeoffs remain behind the generated-code improvement.
Receipts, baseline/candidate hashes, failed setup attempts and comparisons are
retained in `target/codegen-bench/evm-rewrite-candidate/lower-alloc-effect-workflow-20260908/`.

## Measured changes

Literal caching versus frozen `0e61860e` shrinks eight matched UI objects:
Gas creation/runtime totals each fall 977 bytes, with no growth; Size is exact.
Aave saves 12 gas on 19 hot labels and 24 on two. The entry-order witness saves
940 bytes per object and 15/33 gas, but retains 20 bytes of Gas debt versus sealed.
Its quiet compiler-time comparison is +1.16%, RSS -0.61%; that cost is retained.
See [literal-cache evidence][literal] for exact inputs, producers and samples.

Size tail grouping versus frozen `7d598404` shrinks 56 of 5,036 matched UI objects:
creation/runtime totals each fall 559 bytes, with no growth. All 1,648 IDs and
825 source hashes match. The initial trial's seven growing objects are preserved
as rejected evidence; the final reserve restores each to baseline bytes.
ERC20Mock saves 11 bytes in both objects with unchanged hot call gas.
Alias Size runtime falls 179→166 bytes; three labels cost 11 more gas than the
prior candidate while remaining below sealed. Its Size byte debt remains 13.

The final full workflow preserves all 24 IDs, 175 ordered gas labels, 139
observations and complete Gas outputs, including all nine heavy projects.
The Size supplement retains 15 IDs/175 labels/139 observations. Quiet full-run
compiler-time geometric mean is -1.06%, RSS +0.30%; this is not a causal speedup
claim. Concurrent Size timing is not used. [Tail-group evidence][group] retains
all commands, individual deltas, exact output joins and source/binary pins.

## Verification and reviewed expectations

The earlier tail-group workspace had 11,747 UI passes, one original alias assertion
failure, 1,557 other passes and two skips. All 36 Foundry projects pass
(772 compiler tests, 765 solc tests), with reported gas and sizes unchanged.
Clippy, formatting and typos pass. The larger tail fixture passes 24 fresh calls
and saves 16 Size bytes with unchanged gas; the alias has bounded solsymdiff
agreement against pinned solc under the recorded Size input/world limits.

Metadata/debug requests preserve measured bytecode across 92 paired captures.
All 75,054 source-map projections match emitted origins. Shared tails can merge
or lose ambiguous checkpoints; optimized-away forwarding entries can lose events.
Those existing policies and changed immutable/reference locations are explicit
in the independent review, not described as complete event preservation.

The cold-call test retains the original Solidity and None checks. Reviewed
optimized checks bind both selectors, predicates, cold helper paths, arguments
and shared returns instead of requiring the former physical fallthrough shape.
All three revisions and 30 exact call/revert checks pass. Current Gas objects
are 194/177 bytes versus sealed 214/197; Size is 193/176 versus 214/197.
All 15 matched labels use less gas except nonpayable rejection, which is equal.
[Cold-call evidence][cold] preserves failed directive setup and normalization
attempts. No original test was removed to close that assertion.

## Current experiments

The validated-word cleanup trial is rejected: despite UI size wins, Fractional,
Maple ERC20 and Governor grow 35/34/17 bytes in both modes, worsening sealed debt.
Runtime results and gas remain exact. Its MIR patch is reversed. The isolated
owner-equality tail fix is also rejected: alias overflow costs 218 gas versus
sealed 215. The writer-bank milestone (`e34bb6e6`) accepts profitable eleven-home bitmaps
and prices Phi writer protection by gas first in Gas mode. It saves 596,175
combined bytes across 277 of 3,344 heavy objects, with no growth. UI creation
and runtime each save 205 bytes, and all 175 hot-gas labels remain exact in both
modes. Foundry preserves all 1,537 test records and gas values; bounded symbolic
comparison agrees over nine paths and fourteen queries. The threshold-only
trials remain rejected evidence. These alias trials are not accepted milestones.

Main `becd2143` is merged in `e40b84f0`, retaining the rewrite backend while
adopting the moved MIR interfaces, call-analysis caching, debug APIs and solx
benchmark support. `503dcfc8` separately updates reviewed debug expectations.
All 5,048 matched UI bytecode objects, 3,344 heavy objects, 175 hot-gas labels
in both modes and 1,537 Foundry test records remain exact against the accepted
writer milestone. Source-map changes are restricted to removing internal-return
markers from external STOP/RETURN instructions; source ranges are preserved.
Raw full compiler time is -0.90%, RSS +1.05%, with the reference run's recorded
parser overlap and no controlled interleaved timing claim. The new workflow's
79 Python tests pass. No solx measurements were collected in this comparison.

The conditional-only medium-tail experiment is deferred: all 5,048 matched UI
bytecode objects remain identical in both modes, so it adds no measured source
benefit. The production patch is reverted; no tracked test was installed or
removed. Its focused activation and independent bytecode audit remain in
`target/codegen-bench/evm-rewrite-candidate/conditional-medium-tail-workflow-20260908/`.
The broader proposal remains held for overlap with known bytecode regressions.

## Physical unit-carry milestone

A six-instruction unsigned increment carry test becomes
`push 1; add; dup1; iszero` in physical EVM IR. Both literal/copy orders preserve
the opaque prefix, sum and carry; each match saves two bytes and six gas while
lowering peak stack depth. Existing observer and metadata guards apply. MIR
scheduling trials, memory homes and the assembler are unchanged. The earlier MIR
version is rejected because 313 UI objects grew despite aggregate savings.

The final frozen producer is `4eb07937` and its reference is `82012fcd`.
Across 1,656 matched UI IDs, 829 source hashes and 5,052 objects, 42 Gas objects
shrink, none grow, and all Size objects remain exact. Gas creation/runtime totals
fall 50/48 bytes. All 3,344 heavy objects remain exact through complete output
fingerprints and retained raw captures. Full and Size workflows preserve all
175 call labels and 139 observations; Foundry preserves all 36 project/settings
IDs, 1,537 test records and 244 size fields. The final workspace has 11,757 UI
passes, the original alias failure, 1,557 other passes and two skips. Clippy,
formatting and typos pass. No original source or FileCheck assertion changes;
two debug goldens have separate instruction/source-map/event reviews.

Focused execution totals 318 exact calls without gas increases on the original
peephole producer. The final equivalent matcher independently repeats the UI,
full, Size and Foundry gates; it is not relabeled as that earlier producer.
Bounded symbolic comparison agrees with solc 0.8.36 for one checked increment,
within the retained input, query, path and time limits. This is not exhaustive
proof. Foundry's historical report lacks per-project input-content hashes; its
path/settings join and current source-stability receipts retain that limitation.

A quiet six-leg compiler-time repeat puts final mean v4/Solmate times
4.03%/2.30% below the accepted reference, but 0.59%/2.23% above the earlier
peephole form. Two samples per build and project establish no causal speedup;
earlier positive deltas remain preserved. Concurrent full-run timings are unused.
[Unit-carry evidence][unit] retains exact commands, producers, unchanged IDs,
raw samples, reviews and rejected trials.

The prior-art follow-up identifies a concrete information loss: the native
`toOrders` return remains an explicit allocation until `lower-alloc` rewrites it
to `mload(64)`. Ordinary and diagnostic outputs match completely. Retaining that
local identity alone does not export its lifetime/range across calls or prove
private-home separation. The terminal-anchor trial also remains deferred:
current source Size output already shares its return, so the isolated rule
would not activate. [Research](evm-stack-scheduling-research.md) links the pinned
primary mechanisms, native boundaries, controls and remaining proof obligations.

## Remaining acceptance work

`global_stack_calldata_alias.sol` remains the original failing assertion.
The original readback failures now pass in every mode. General source-memory
ownership remains an open contract; bounded shared-input sweeps pass. The accepted heavy ledger
still has 31,831,619 positive bytes of sealed size debt;
this is a sum of regressions, not net corpus growth. Compiler-time debts remain.
The backend has 16,974 physical lines in 46 files, 17,664 fewer than the deletion
inventory. Excluding trailing test modules leaves 15,669 physical lines. A
retained count-only baseline reports 29,006 production-section lines, giving a
conditional reduction of 13,337; that file lacks a revision/hash link to the
sealed archive. These counts include comments and are not strict production SLOC.

Eager contraction removes avoidable spills and saves bytecode without corpus
growth; full Gas/Size outputs and Foundry gas remain exact. It adds 188 physical
MIR Rust lines. Quiet repeats retain +1.64% OpenZeppelin and +3.11% Morpho compile
costs. The consumer-tracking simplification (`ce593973`) removes five Rust
lines, halves its consumer entry to four bytes, and preserves all measured
outputs and gas. Its full timing (+4.22%, RSS +0.10%) overlaps another task
on the host and has unequal baseline sample counts; it establishes no speed
claim. The writer milestone records +4.18% compiler time and -0.59% RSS,
with baseline contention and a measured parser overlap; no causal timing claim
follows. Next, restore alias-codegen size and complete the remaining runtime/size
gates. Keep correctness and runtime
gas ahead of size, then compiler time and memory. These local milestones do not
establish complete functionality, green remote CI or a successful remote push.

[pr]: https://github.com/paradigmxyz/solar/pull/1388
[literal]: ../target/codegen-bench/evm-rewrite-candidate/literal-cache-workflow-20260908/
[cold]: ../target/codegen-bench/evm-rewrite-candidate/cold-expectation-migration-20260908/
[group]: ../target/codegen-bench/evm-rewrite-candidate/size-long-tail-group-workflow-20260908/

[unit]: ../target/codegen-bench/evm-rewrite-candidate/unit-carry-physical-workflow-20260908/
