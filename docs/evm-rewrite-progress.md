# EVM rewrite progress

The rewrite remains incomplete. General memory-ownership evidence and individual
sealed gas/size debts still block final acceptance. The original UI assertion
has been resolved without changing its Solidity body.
This page records accepted milestones and integration through 2026-09-09. The
[handoff](evm-rewrite-plan.md) defines acceptance; [PR #1388][pr] tracks review.
The [checkpoint archive](evm-rewrite-checkpoints.md) preserves the complete
history, baseline hashes, rejected trials and measurement limitations.

## Accepted local state

Main `2632e43b` was integrated by merge `790e3687`. Earlier commits preserve
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

## Calldata carry checkpoint

The repeated-calldata unit-carry rule reuses the incremented sum instead of
loading the same immutable word for the comparison. Size enables it in the
existing final DCE traversal after sharing; applying it earlier grew two
contracts, so that rejected trial remains archived. The current comparison
retains 1,660 IDs and 5,060 objects: 66 shrink and 4,994 remain byte-exact.
Gas creation/runtime totals each fall 39 bytes; Size falls 98/95. The focused
source saves six execution gas and three runtime bytes. The original alias
case now has 163 Size runtime bytes versus sealed 153; its 63-call replay
passes, including overflow at 200 gas versus sealed 215. Its original failing
snapshot remains untouched.

The outlined helper preserves every compared byte of the late matcher.
Current-build runtime/Size/Foundry runs finish successfully. Independent corpus
joins preserve all 175 gas labels, 139 observations and serialized artifacts
per compiler. Foundry preserves 1,537 status/gas records and 244 size fields
across 36 IDs, with the actual rebuilt producer recorded. Workspace testing records
11,775 UI passes and the sole original alias failure, plus 1,557 other passes
and two skips. The CI-scope Clippy command passes on the current stable
toolchain; the initial all-features alias failure is retained. CI uses nightly.
Two timing golden files change only the four final `dce` names to `late-dce`;
raw captures justify each change. Fourteen new fixture files cover positive,
refusal, observation and runtime behavior.

Quiet six-leg means retain a Seaport compiler-time cost of +3.20% versus the
allocation baseline (-1.17% versus the larger late matcher); the earlier
+8.17% trial is preserved. Profiling identifies stack verification and opcode
effect lookup as larger costs than the carry matcher. No speed-neutral claim
follows from two samples. The implementation adds 82 EVM Rust lines; its
output gains and remaining compiler-cost concern are documented separately.
Evidence lives under
`target/codegen-bench/evm-rewrite-candidate/rematerialized-unit-carry-workflow-20260908/`.

## Opcode effect lookup checkpoint

The profiled stack-effect match now indexes a borrowed constant table derived
from the same opcode declarations. Exhaustive comparison preserves all 256
results, including 95 unknown effects. A first by-value table copied 768 bytes
per lookup in debug builds and is rejected; its binary, assembly and complete
timing run remain archived. The final shared reference uses a 16-byte frame
and a 97-byte lookup body, with no table copy.

Against the carry checkpoint, quiet ABBA means fall 2.37% for Seaport, 6.67%
for v4 and 3.81% for Solmate. Both samples per project are below the respective
baseline range; this small batch does not establish isolated causal savings
or erase the earlier allocation-baseline timing concern. All 5,060 UI bytecode
objects are exact and workspace results are unchanged. Full runtime, hot-gas,
Size, Foundry, Clippy and formatting runs finish with the same original alias
failure retained in the workspace suite. Independent joins preserve all 175
gas labels and 139 observations per compiler, all 375 retained artifacts and
1,537 Foundry status/gas records with 244 size fields. The lookup adds eleven physical EVM
Rust lines and changes no generated-code decision. Evidence is retained under
`target/codegen-bench/evm-rewrite-candidate/opcode-stack-table-workflow-20260908/`.

## Main integration and alias contract migration (September 8)

Merged main `54c76a0cce9e28fce531b841af1c8c70faf30686` in `7e7bff72`.
All 46 rewritten backend files were byte-identical across the merge; no
original backend implementation was retrieved. Nightly rustfmt then changed
only import grouping and comment wrapping. The new benchmarking workflow
and its comparison rules are retained from main.

The alias fixture now checks the current physical branch representation and
keeps the original Solidity function body. Its standard matrix runs all 21
retained runtime oracles in four modes. A fifth Gas IR revision repeats one
call so the runner retains its nonempty dump: 85 calls in total. The checked
relationships preserve one cleaned account through the branch chain and read
the independent value from calldata word 36. The old unrevisioned snapshot
is replaced by reviewed IR and MIR snapshots; its original bytes remain in
the evidence archive. All five revisions pass. Five corrupting IR mutants
are rejected by the check patterns. Before/after annotation migration produces
identical complete contract objects in None, Gas and Size.

This is an assertion migration, not restoration of the old eager-load and
shared-tail strategy. Alias runtime sizes remain 208 bytes in Gas and 163 in
Size, versus the sealed 153-byte baseline: +55 and +10 bytes respectively.
Fresh bounded symbolic comparisons agree with pinned solc in both modes;
malformed calldata, dirty addresses and nonpayable calls are covered by the
concrete runtime oracles. The merged workspace run passes all 1,562 tests
with two pre-existing skips, including the complete UI runner and Foundry.
Nightly-feature workspace tests, formatting, Clippy and typo checks pass
locally. Remote checks are
being run on the pushed draft; this checkpoint does not claim remote success.
Evidence is under
`target/codegen-bench/evm-rewrite-candidate/main-integration-20260908/` and
`target/codegen-bench/evm-rewrite-candidate/alias-ci-contract-review-20260908/`.

## Four-word terminal returns and Windows CI integration

The terminal return proof now handles up to four completely overwritten words,
including a final size/DUP1 pair, with the existing memory and code-observer
refusals. It uses four fixed store positions and adds five physical backend
lines. Four EVM IR fixtures and one Solidity fixture add 18 revisions and 36
runtime calls. Two existing snapshots change only the compacted addresses and
resulting deployment lengths; frozen baseline reproduction and the retained
immutable/entry-order runtime oracles justify those updates. No existing test
source or runtime expectation was removed.

Against the previous checkpoint, all 1,660 existing UI IDs retain their status:
310 of 5,060 bytecode objects shrink, 4,750 are exact, and none grow. Gas saves
237 bytes each in creation/runtime output; Size saves 144 each. The additional
fixture is compared separately against the frozen baseline. Full runtime and
Size runs preserve all 175 gas labels and 139 observations per compiler/lane;
Aave saves four bytes per object and twelve v4 objects save two bytes each.
The final compiler is connected to those execution results through identical
complete outputs and 270 freshly captured artifacts, not relabeled fresh gas
runs. Four bounded symbolic comparisons agree with pinned solc. Workspace tests
pass (1,562, two existing skips), as do Clippy, formatting and typo checks.

Quiet ABBA means increase 1.735% for Seaport, 2.735% for v4 and 0.073% for
Solmate. All time ranges overlap with only two samples per compiler/project;
this does not establish a speed win or dismiss the measured cost. RSS means
change -0.275%, +0.604% and +2.000%, respectively. The size improvement is
retained while compiler-time concerns remain open. Independent review and the
complete provenance, rejected fixture attempts, output joins and measurements
are under `target/codegen-bench/evm-rewrite-candidate/terminal-four-word-workflow-20260908/`.

Main `2632e43b` (Windows CI test repairs) is merged after the previous draft
head passed all remote checks. The merge leaves the rewritten backend exactly
unchanged. The merged workspace passes all 1,562 tests with two skips; the
nightly-feature run passes 1,559 with five skips, including its three existing
layout-test exclusions. Formatting also passes. Remote checks completed on
`5db94d77`: 17 succeeded, the full cross-server comparison was intentionally
skipped, and CodSpeed reported neutral. The
aggregate `ci success` check passed.

## September 9 integration checkpoint

A fresh fetch confirms main remains at `2632e43b`, already merged. The resident
operand ordering experiment is rejected and its production changes are removed:
the first trial grows 75 existing Gas contracts in the UI corpus, and the bounded
block trial still grows 25. Both preserve the same 1,662 UI IDs; no expectations were blessed
and no tests were removed. Stage captures identify literal-aware stack
normalization as the first size reversal missed by the scheduler cost model.
The candidate sources, frozen compilers, unchanged baselines, runtime reports
and proposed tests are retained under
`target/codegen-bench/evm-rewrite-candidate/resident-before-literals-20260908/`.
Those proposals remain unaccepted work, separate from the pushed backend.
After restoring the accepted implementation, all 1,562 workspace tests pass
with the same two skips. Formatting and the whitespace check pass.

## Resident operands before literal construction

The fourth resident-operand trial is accepted as an incremental milestone in
`01e526b9`, with additional coverage in `11070cbb`. A
focused scheduler helper prepares already resident operands before pushing an
absent scalar-literal prefix. It runs last, preserves the established chooser's
entry prefix and exact exit layout, and uses the same bounded block eligibility.
The physical-IR cost query checks conservative normalization, literal-aware
normalization and orientation, including the existing literal planner's relative
stack budget. The assembler and MIR remain unchanged. These estimates do not
model every later sharing or caching effect, so complete outputs remain a gate.

The first two candidates above remain rejected. Candidate three removed every
UI growth but grew 18 heavy bytecode objects: earlier DUPs consumed the headroom
needed for short sign-bit constructions, leaving PUSH32 literals. Candidate four
prices that construction headroom without granting additional stack capacity.
The reduced recursive fixture preserves a short construction and shrinks from
172 to 170 runtime bytes; candidate three produced 195 bytes.

Against frozen integrated baseline `f6902c31`, candidate `0ab6248b` preserves all
24 full-workflow IDs, 175 ordered call labels and 139 runtime observations.
Fifty-five calls save gas, 120 are exact and none grow (summed delta -1,578 gas).
The 15 runtime cases save 233 creation and 203 runtime bytes. The complete heavy
ledger preserves nine project IDs, 1,672 contracts and 3,344 objects, including
empty outputs: 1,112 objects shrink, none grow and none change at equal size,
saving 92,009 bytes. Eight captures are fresh; Solarray explicitly reuses an
identical full-output fingerprint. These are comparisons with the preceding
checkpoint, not closure of the original sealed-baseline debts.

The original 1,662 UI IDs retain their inputs and outcomes. The expanded 1,668-ID
comparison saves 1,113 Gas creation bytes and 675 runtime bytes without growth;
all Size bytes remain exact. New fixture rows are explicitly joined to separate
captures from the same frozen producers, not presented as one fresh timing run.
Three UI fixtures execute 111 calls; five Foundry tests check exact event topics,
data, emitter and persistent state. Three fresh bounded symbolic comparisons
agree with pinned solc on status and return data; they do not prove arbitrary
logs, state or recursion. Four existing snapshots were updated only after frozen
baseline reproduction, symbolic stack-effect review and unchanged FileChecks.
No existing test source or runtime oracle was removed. All 1,562 workspace tests
pass with the same two skips; formatting, typo checks and nightly Clippy pass.
Clippy's first nightly process stalled on an exited build script; the two-job
retry completed successfully. The stable all-features alias requires nightly.

The official full-run compiler-time geometric mean increases 0.962%, with RSS
-0.142%; Size time increases 0.183%, RSS 0.199%. Several individual compile-time
ranges are disjoint, including Maple +9.396% and Governor +5.878%. Most rows have
five samples, but adaptive repeats leave Seaport and Solady at one per producer,
and OpenZeppelin at two baseline versus one candidate. These measurements do not
establish a compiler speedup. The quiet five-case ABBA follow-up retains ten time samples per compiler and
lane, with exact inputs and producer-specific output fingerprints. Gas means
increase 4.051% geometrically: Maple +8.081%, Aave +3.468%, Governor +3.538%,
Fractional +3.520%, LibString +1.755%. Maple, Governor and Fractional have
disjoint slower ranges. The inactive Size lane is +0.131%. RSS geometric means
are -0.591% Gas and +0.734% Size, with only two retained readings per compiler
and case. This sustains a Gas compile-cost concern. Output quality justifies
this local milestone while compiler-time debts remain open.

The backend is now 17,290 physical Rust lines across 47 files, an increase of
213 lines for this milestone and a reduction of 17,348 from the deletion
inventory. Excluding trailing test modules leaves 15,985 physical lines. Pinned
solx, Venom and Sonatina sources informed the bounded operand and literal-cost
investigation; no deleted backend implementation was retrieved. Full inputs,
producer hashes, rejected candidates, review receipts and measurements are under
`target/codegen-bench/evm-rewrite-candidate/resident-before-literals-20260908/`.
A fresh fetch and merge again find main `2632e43b` already integrated.

## Remaining acceptance work

The alias assertion migration passes; its generated-code size debt remains.
The original readback failures now pass in every mode. General source-memory
ownership remains an open contract; bounded shared-input sweeps pass. The preceding accepted heavy ledger
has 31,831,619 positive bytes of sealed size debt; the new local savings have not
yet been rejoined to every sealed object, so this historical total is retained;
this is a sum of regressions, not net corpus growth. Compiler-time debts remain.
The backend has 17,290 physical lines in 47 files, 17,348 fewer than the deletion
inventory. Excluding trailing test modules leaves 15,985 physical lines. A
retained count-only baseline reports 29,006 production-section lines, giving a
conditional reduction of 13,021; that file lacks a revision/hash link to the
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
establish complete functionality. Remote CI passed at `5db94d77`; this does
not close the performance acceptance debts above.

[pr]: https://github.com/paradigmxyz/solar/pull/1388
[literal]: ../target/codegen-bench/evm-rewrite-candidate/literal-cache-workflow-20260908/
[cold]: ../target/codegen-bench/evm-rewrite-candidate/cold-expectation-migration-20260908/
[group]: ../target/codegen-bench/evm-rewrite-candidate/size-long-tail-group-workflow-20260908/

[unit]: ../target/codegen-bench/evm-rewrite-candidate/unit-carry-physical-workflow-20260908/
