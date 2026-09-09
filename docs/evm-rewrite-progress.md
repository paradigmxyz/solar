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

## Shared-call experiment and integration (September 9)

A fresh fetch finds main `2632e43b` already merged. The shared-call experiment
is rejected and its two production files and two existing goldens are restored
from our own checksummed pre-experiment copies. The rebuilt compiler is exactly
`0ab6248b3a93b1d1be9b1ba58e51e717849f19a2ad353bde8044a36d129a346d`,
the accepted executable. No production lines change in this checkpoint.

The experiment reused storage planning's existing immutable call summaries in
scheduler alias queries, adding eight physical Rust lines without another
fixed-point computation. The UI screen preserves 1,668 IDs, 835 source hashes
and 5,076 bytecode objects: 24 objects shrink, saving 892 creation and 892 runtime
bytes in each optimization mode, with no growth. The official full and Size
workflows retain all 175 hot-gas labels and 139 observations exactly; their 15
runtime cases have identical full output fingerprints. Pinned solc corpus rows
are inherited from the baseline; the focused tests below execute solc freshly.
Full compiler-time median geometric mean is +0.284%, Size -0.136%. Seaport and
Solady have only one sample per producer, and OpenZeppelin has two baseline
versus one candidate; the other full rows and all Size rows have five.

The complete heavy comparison prevents acceptance. Across the same 3,344
objects, the net saving is 59,761 bytes, but `MyGovernor` grows by 18 bytes in
both creation and runtime code. Removing protocol backups reduces the relative
stack peak available to compact literals: three address masks and one byte mask
grow by 76 bytes, outweighing 58 bytes of removed protocol/shuffle code. A second
trial preserving initial spill planning still grows by 18 bytes. Neither trial
ships. A follow-up must account for literal construction across the complete
call region without weakening the caller-prefix capacity proof. Both candidate
executables, complete inputs/outputs and the rejection witness remain preserved.

Three new runtime owners retain eighteen computed values across memory-clean,
unknown-pointer-writing and multi-result recursive calls. Their six exact
oracles pass all five UI revisions. Another 36 fresh calls compare the accepted
compiler, the first candidate and pinned solc in Gas and Size; all pass. The
candidate saves 219 gas in the clean-call cases, while the two controls retain
identical gas and bytecode. The candidate-only clean-call emission check stays
in the experiment archive; committed tests retain the runtime oracles, MIR
snapshots and physical checks for unknown-writer protection and hidden result
loads. Removing either protected operation fails its corresponding check. No
existing test or oracle is removed or weakened. The candidate also reports
bounded symbolic agreement for the existing resident-argument fixture's
`first(uint256)` entry point; its default differential settings produce different
bytecode from the UI lane, so this is separate evidence.

Native attribution of the original two 13-home Reference protection banks finds
all thirteen values mandatory across calls, including five arguments and five
Phi values. This experiment does not remove those banks or resolve general
source-memory ownership. The complete selected Reference contract output stays
exact. A diagnostic raw-dump assertion initially failed on absolute source-span
offsets; independent reconciliation confirms identical canonical MIR,
disassembly and complete output JSON. Both the failure and reconciliation are
retained under
`target/codegen-bench/evm-rewrite-candidate/reference-bank-residence-20260909/`,
together with the rejected candidates, measurements and independent reviews.

## Bounded clean-call emission (2026-09-09)

`530099f5` reuses existing memory-effect summaries to identify clean calls,
without changing frame layouts, alias provenance or spill planning. The ordinary
owner emission runs first; an owner with an eligible internal call may try an alternate
without clean-call backups. Complete physical blocks must preserve topology,
respect private entry-stack bounds and improve the existing literal-cost query.
Multi-result calls and nested Phi trials remain conservative. A shared owner
snapshot preserves appended continuations, metadata and switch budgets when a
trial is rejected. This adds 145 physical backend Rust lines; the assembler and
MIR layer boundaries remain unchanged.

The reduced recursive-call fixture in `97917400` preserves compact address-mask
construction and executes both successful calls and an exact empty revert in all
five UI revisions. The new and existing focused fixtures pass 54 fresh calls
against the baseline, candidate and pinned solc in Gas and Size. Clean-call
vectors save 219 gas; the reduced recursive vector saves 36 gas. Unknown-writer
and multi-result controls retain exact gas and bytecode. All three changed
existing snapshots were independently reviewed from paired outputs; surviving
source maps match, and no source, check or runtime oracle was removed.

The fresh official workflow preserves all 24 full test IDs, 15 Size IDs, 175 gas
records and 139 observations in each lane. All 15 runtime output fingerprints
remain exact. The extended UI comparison explicitly joins the original 1,670
rows with two rows for the new fixture: 1,672 rows and 5,092 bytecode objects,
with 32 shrinking objects and no growth or equal-length bytecode changes.
Creation and runtime totals each fall by 1,042 bytes in each mode. All 18 known
failed capture rows remain; one stderr difference is only warning ordering.

The nine heavy projects retain all 1,672 contracts and 3,344 objects. Five fresh
complete-output captures and four exact fingerprint reuses find 191 shrinking
objects, no growth and 57,652 fewer bytes. MyGovernor shrinks by 28 bytes in both
creation and runtime code; this resolves the preceding experiment's regression.
Positive sealed size debt falls by 57,488 bytes. The selected outputs establish
profitability on these corpora; they do not identify which cost guard rejected
each discarded alternative or prove final layout costs universally.

Full compiler-time median geometric mean changes by +0.047%, Size by +0.967%;
RSS changes by +0.100% and -0.037%. Full sample counts are 112 baseline and 109
candidate, including unequal OpenZeppelin counts and one-sample heavy rows.
Flashloan and SignatureChecker have disjoint slower five-sample ranges. There
is no compiler-speed claim. Pinned solc corpus rows are inherited exactly; the
focused runtime and bounded symbolic runs execute solc freshly. The latter
reports agreement for `first(uint256)` under 128 paths and 256 queries, with no
counterexample; it does not cover the new constructor fixture.

All 1,562 workspace tests, nightly Clippy and formatting pass. Baselines,
compiler/source pins, complete outputs, failure reconciliation, independent
reviews and per-object ledgers remain under
`target/codegen-bench/evm-rewrite-candidate/call-region-cost-20260909/`.

## Rejected store-pair experiment and integration checkpoint

A disjoint adjacent-MSTORE rewrite passed focused stack, memory and source-map
checks, but failed the complete UI size screen. The matched 1,674 rows and 5,096
bytecode objects retain all inputs, contract IDs and compiler statuses. Gas
creation/runtime totals fall by 289/287 bytes and Size by 361/332 bytes, yet 34
individual objects grow across 18 contract rows. Aggregate savings do not meet
the acceptance gate. The candidate workspace run passes 1,561 tests and fails
the UI runner on 22 executable-output snapshots; these existing expectations
were not blessed. Full hot-gas and heavy-corpus acceptance were not attempted.

The rejected production edit and its new physical fixtures are archived with
checksums under `target/codegen-bench/evm-rewrite-candidate/getter-terminal-20260909/store-pair/integration/`.
The accepted implementation is restored from the preserved before-copy, and
all previously tracked tests remain unchanged. The independent Solidity getter
fixture is retained: absent and initialized mapping entries return four exact
ABI values in all five codegen revisions. Only this new fixture's snapshots
were generated for the restored compiler. Eighteen independent focused calls
against the baseline, candidate and pinned solc also pass; their local gas
savings do not override the corpus rejection. No compiler-speed claim follows.

Main was fetched again at `2632e43b` and was already merged. The pushed
`01e589d5` checkpoint has 17 successful checks, one expected skip and one neutral
CodSpeed analysis. The restored workspace, including the retained getter
fixture, passes all 1,562 tests with the same two existing skips. This
integration does not change production code or close any of the remaining
performance debts below.

## Late-DCE store-pair milestone

The rejected adjacent-store rule is now confined to the existing late-DCE
traversal. Earlier physical cleanup and all five peephole call sites used by
scheduling or pass adapters have explicit phase permissions; both scheduling
query calls disable the rule. The exact disjoint, nonwrapping word-range proof,
stack interface and source/event handling remain unchanged. This adds 64
physical Rust lines across two existing files, with no new traversal or analysis.

Paired phase captures preserve complete outputs between the default and explicit
staged pipelines. The two reduced prior-growth examples remain byte-exact at
every stage. Fractional first differs at late-DCE: one exchange is removed by
reversing its independent stores; its initial runtime IR stays exact. Constructor
changes are exactly the two shortened runtime-length constants. This establishes
phase isolation for these cases, not a universal model of downstream layout.

The fresh official Full and Size runs retain all 24/15 test IDs, all 175 gas
records and all 139 observations in each lane. Seven Fractional getter calls
save 9 gas each in both modes; the other 168 records remain exact. Its creation
and runtime code each shrink by three bytes. The UI comparison retains 1,674
rows, 838 source hashes and 5,096 objects: 181 shrink, none grow and no equal-size
byte changes occur. Gas creation/runtime totals each fall by 90 bytes; Size
falls by 257/234 bytes. The 18 existing failed capture rows remain present.

Six fresh heavy-project captures and three complete-fingerprint reuses preserve
all nine projects, 1,672 contracts and 3,344 objects. There are 296 shrinking
objects, no growth and 8,604 fewer bytes. Positive sealed debt falls by 8,550
bytes to 31,677,987 across the same 1,039 positive objects; the remaining 54
saved bytes were outside that positive-debt sum. The full and Size compiler-time
median geometric means increase by 0.745% and 1.458%, respectively, with 108
and 75 samples per leg. RSS increases by 0.335% and 0.698%. These measurements
establish no compiler-speed improvement.

All 1,562 workspace tests and nightly Clippy pass. Seven snapshot changes were
reviewed as eight disjoint-store rotations and four immutable offset updates.
Two assertions were added to the shared-tail FileCheck to check both reordered
stores; all original runtime bodies, directives and oracles remain unchanged.
Fresh paired UI captures after that comment edit preserve every object. Eight new physical fixture revisions and one glue case cover early refusal,
late activation, overlap, wrapping addresses, glue validation and debug events. Fresh plain/debug assembly
captures are byte-identical per compiler in Osaka and Amsterdam. Bounded
`solsymdiff` runs for `ICallFallbacks.multi(uint256)` agree with pinned solc in
Gas and Size under 128 paths and 256 solver queries; the selected successful
return path contains the transformed pair. This is bounded evidence, not an
unrestricted equivalence proof.

Baselines, frozen compilers, per-object and per-label comparisons, rejected
outputs, phase captures and independent reviews remain under
`target/codegen-bench/evm-rewrite-candidate/getter-terminal-20260909/late-store-pair-20260909/`.

## Returning-memory milestone

The MIR call summary now distinguishes writes on paths that can resume a caller
from all-path memory effects. One reverse CFG walk finds returning blocks; the
existing monotone call fixpoint propagates the new fact. Void internal `stop`,
tail calls, incomplete bodies and multi-result publication remain conservative.
Only the backend caller-save decision consumes this fact; alias analysis and
frame planning retain their all-path effects. This removes spill backup and
restore operations around callees whose memory writes occur only while aborting.

The first candidate was rejected: Morpho grew 243 bytes, amplified across 40
heavy objects. A shared panic block exceeded the verifier's label-context limit
and erased useful bounds on caller continuations it could never resume. Under
Morpho's Paris target, 35 lost uint128 and two lost address-mask constructions
cost 415 bytes, offset by 172 other saved bytes. The final verifier forgets label
identities before deduplicating physically halting contexts without embedded
jumps, preserving exact heights and the first raw recursion prototype. Capacity
checks and returning or embedded-jump contexts remain intact. Independent review
caught the prototype requirement before the final build. No context limit was
raised. The final Morpho creation/runtime sizes are 16,167/15,745 bytes, each
329 bytes below the baseline.

Fresh official Full/Size runs preserve all 24/15 ordered IDs, all 175 gas records
and all 139 execution observations in each lane. Gas and outputs are unchanged;
Governor creation alone shrinks eight bytes in both modes. The UI screen joins
1,676 rows and 5,104 objects, including the new fixture: 14 shrink, none grow and
no equal-size objects change. All 18 prior failed rows remain. Five fresh heavy
captures and four complete-fingerprint reuses preserve nine projects, 1,672
contracts and 3,344 objects: 267 shrink, none grow and no equal-size objects
change. The total falls by 631,439 bytes. Positive sealed debt falls by 631,275
to 31,046,712 bytes across the same 1,039 objects.

The geometric means of per-case arithmetic compiler-time mean ratios fall by
0.236%/0.112% in Full/Size, with 108/75 samples per leg; RSS falls by
0.317%/0.196%. These small aggregate differences do
not establish a general speedup. Full Aave and Maple means increase by
3.012%/2.995%, with disjoint five-sample ranges; OpenZeppelin increases 1.141%
with only one sample per leg. Every per-case increase and sample is retained.

All 1,562 workspace tests and nightly Clippy pass, with the same two existing
skips. Two existing snapshots change only the reviewed caller-save backups and
argument shuffling; no existing source, runtime oracle or test is removed.
The new Solidity fixture covers 11 successful and five exact failing calls in
five revisions. Paired baseline, candidate and pinned-solc runs pass all 96
calls across Gas and Size. Its positive successful calls save 267 gas; writer
controls remain exact. Three EVM IR fixtures exercise halting-context merging,
embedded and returning jumps, and preservation of the 1,024-word capacity limit.
Bounded `solsymdiff` agrees for a separate linear returning-call witness in both
modes; the arithmetic fraction witness remains incomplete in both modes, with
no replayed counterexample. Incomplete results are not agreement.

Pinned Venom, Sonatina and solx LLVM passages were checked for terminal stack
liveness and the distinction between internal return and external halt. They
support that distinction, not a claim that they implement this exact memory
summary. The implementation adds 68 physical production Rust lines: 50 in the
MIR analysis and 18 in the existing EVM verifier, with no new production file.
Baselines, rejected captures, primary-source links, source and executable pins,
per-object and per-label audits, differential limits and independent reviews
remain under
`target/codegen-bench/evm-rewrite-candidate/returning-memory-20260909/`.

## Shared bitmap selector experiment (rejected)

The September 9 experiment reused one shifted membership mask for both words
protected around a source store. The raw templates tied at 42 instructions and
130 static gas, and independent arithmetic and memory-trace checks agreed.
Final normalization nevertheless removed three instructions from the existing
template and only two from the candidate. All six calls in the new sparse-bank
fixture therefore cost three additional gas under `-Ogas`: the first-word case
rose from 1,986 to 1,989 execution gas. The candidate is rejected and the
immediately preceding rewritten source is restored. No deleted implementation
was retrieved. No production changes from this experiment are retained.

The official quiet Full/Size workflow preserved 24/15 ordered IDs and all 175
gas labels and 139 execution observations per mode. That corpus missed the
focused regression. Nitro creation/runtime each shrank 118 bytes; Size outputs
were exact. The expanded UI screen joined 1,678 rows and 5,108 objects, with ten
shrinking, none growing and all 18 existing failed rows retained. The heavy
join retained all 3,344 objects: 266 shrank, none grew, 43 changed at equal size
and 3,035 remained byte-exact. Its 520,911-byte reduction is rejected along with
the candidate, and does not reduce the accepted size-debt ledger.

Compiler-time geometric means of per-case arithmetic sample-mean ratios rose
0.520%/1.035% in Full/Size, with 108/75 samples per leg; peak RSS rose
0.267%/1.146%. Individual increases and sample ranges are retained. These
measurements establish no compiler-speed improvement.

The six fixture oracles pass against the baseline, candidate and pinned solc
in both modes, covering 36 concrete calls. Both bounded symbolic attempts timed
out and remain incomplete. An additional proposed unaligned-hole oracle assumed
zero preexisting memory incorrectly: its final byte was not overwritten and
already contained 12. That failed experiment remains in the evidence; the
incorrect oracle was never added. All 1,562 workspace tests pass after restoring
the accepted implementation, with the same two existing skips. The retained
fixture checks the accepted
sparse writer, including wrapped relative indices, word crossings, distinct
readback and live-value preservation. No existing test or oracle is removed.

Baseline and rejected candidate binaries, source copies, workflow results,
per-object joins, gas traces, symbolic limits and independent reviews remain in
`target/codegen-bench/evm-rewrite-candidate/nitro-size-20260909/`. Fetching and
merging `origin/main` at `2632e43b` reported already up to date. The backend
remains at 17,517 physical Rust lines across 48 files; accepted performance
and the remaining debts below are unchanged.

## Bitmap address scheduling

The revised shared-mask selector computes the second protected address before
the first, consuming its intermediate values before either load. The two loads
and three stores keep their original order. It remains in the existing writer
helper at the MIR-to-EVM boundary; the contiguous selector, restore footer and
all initialized-home, protocol-word, capacity and profitability guards remain.
No planner, compiler trial or fallback is added.

Unlike the rejected proposal above, this schedule removes one raw operation:
41 instructions and 127 gas instead of 42 and 130. The executed focused selector
and all 54 Nitro bitmap sites normalize to 39 instructions and 121 gas, tying
the accepted incumbents. The six existing fixture oracles pass in both modes
with unchanged gas across 12 fresh candidate calls. Retained baseline objects
are joined exactly before reusing their concrete traces; the earlier pinned-solc
leg remains reference evidence, not a fresh solc execution. Gas fixture
creation/runtime each shrink four bytes, while Size bytecode is identical.

The fresh official Full/Size workflows preserve all 24/15 ordered IDs, all 175
gas labels and 139 execution observations in each lane. All gas records and
execution results are unchanged. Nitro creation/runtime each shrink 172 bytes;
every Size output, saved MIR and Standard JSON input remains exact. The final-source
UI comparison joins 1,678 rows and 5,108 objects: ten shrink, none grow, and no
equal-size objects change. Gas creation/runtime totals each fall 25 bytes;
Size is exact and all 18 existing failed rows remain.

Seven fresh heavy captures and two exact complete-fingerprint reuses preserve
all nine projects, 1,672 contracts and 3,344 objects. There are 309 shrinking
objects, 3,035 byte-exact objects, no growing objects and no equal-size changes.
Total size falls by 655,111 bytes. Positive sealed debt falls by 655,108 bytes
to 30,391,604 across the same 1,039 objects. The three-byte difference is a
Solady creation object that was already below its sealed baseline.

Compiler-time geometric means of per-case arithmetic sample-mean ratios rise
1.454%/1.897% in Full/Size, with 108/75 samples per leg. Median-ratio geometric
means rise 1.896%/0.945%; peak RSS changes by +0.156%/-0.191%. Disjoint five-sample
slow ranges include Full Solady Signature (+1.727%) and PRB (+0.977%), and Size
OpenZeppelin ERC20 (+6.116%), Vesting (+4.470%), Nitro (+7.781%), Aave (+8.931%),
Flash (+2.733%) and Maple (+3.546%). These increases remain in the evidence;
this change does not establish compiler-speed neutrality or improvement.

All 1,562 workspace tests and nightly Clippy pass, with the same two existing
skips. Only the reviewed selector FileCheck and two physical expectations
change; test bodies, all six runtime oracles and the MIR golden are unchanged.
Both bounded symbolic attempts remain incomplete after their 45-second limits,
so neither establishes agreement. Independent byte-memory models, exact
formatted-source extraction, executed traces and the complete Nitro selector
census support the local transformation. Pinned solx, Venom and Sonatina
scheduling and cost models informed the review; the selector algebra is an
independent design. The step adds 35 physical production Rust lines and no
production file. Evidence, source and compiler pins, per-label/per-object
comparisons and limits remain under
`target/codegen-bench/evm-rewrite-candidate/bitmap-stack-20260909/`.

## Rejected CSE prefix recovery (2026-09-09)

Merged main `8d3d877e` as `bfb5b367` and pushed it. All 1,575 workspace tests
passed with two existing skips; the merged head has 17 successful CI checks,
one expected skip and one neutral check. Its official runtime benchmark keeps
all 24 case IDs, 175 gas records, 139 observations, 30 bytecode files and nine
heavy fingerprints identical to the prior accepted run.

A fresh merged-head baseline preceded two local CSE trials. They preserve
known stack values through accesses below the tracked suffix using fresh
opaque identities. Review found that newly exposed reuse can replace a
two-gas environment read with a three-gas DUP, so the first trial added a
structural profitability guard. The guard preserves value identities but
changes the early DUP forms used by later stack normalization. One calldata
reference contract grows by one byte in each mode. Across the same 5,108 UI
objects, 82 shrink, four grow and 16 change without changing size.

The second trial additionally removes permutations of equal tracked values.
It fixes those four objects, but two other contracts each grow by one byte
under Size. The joined counts are 355 shrinking, four growing and eight
same-size changed objects. The existing `literal_duplicate_run` Size fixture
still gains a SWAP; its golden was never blessed. Stage captures identify
normalization, producer ordering and outlining interactions, rather than
address-width cliffs. A proposed selective key-invalidation workaround has a
repeated-shuffle counterexample and was not implemented.

Both trials are rejected. All 42 second-trial focused runtime calls passed
with nonincreasing gas and unchanged observed stack peaks; fresh Gas and Size
`solsymdiff` runs for `MultiReturnForward.first(uint256)` found bounded
agreement with solc. The generated harness runtimes match the measured UI
runtimes plus an explicit 14-byte metadata suffix. These correctness results
do not override the cost failures. Full timed, hot-gas and heavy-object
candidate runs were not started after the failed size screens.

The accepted production source and every pre-existing test and expectation
are restored exactly. Five new fixture sources and eleven new goldens,
candidate patches, frozen compilers, failed runs, paired outputs, traces and
reviews remain under
`target/codegen-bench/evm-rewrite-candidate/cse-prefix-20260909/`, including
`restoration.json` and `rejected-tests/`. This experiment contributes zero
accepted production LOC and no performance improvement. The independent
five-instruction duplicate-store opportunity remains a separate proposal.

## Remaining acceptance work

The alias assertion migration passes; its generated-code size debt remains.
The original readback failures now pass in every mode. General source-memory
ownership remains an open contract; bounded shared-input sweeps pass. The complete heavy join now has 30,391,604 positive bytes of sealed size debt
across 1,039 objects. This includes creation/runtime and embedded-child
amplification; it is a sum of regressions, not net corpus growth. The historical
writer count was 31,831,619. Terminal returns removed 20 positive bytes before
the resident trial, which removes another 87,574. All 3,344 sealed object IDs,
sizes and hashes match; the reconciliation is retained under the resident
candidate's `pr-ledger/`; the bounded clean-call increment removes another
57,488 positive bytes in its `candidate1/independent/` ledger. Late-DCE store
reordering removes another 8,550 positive bytes; returning-memory analysis and
halting-context verification remove another 631,275. Bitmap address scheduling
removes another 655,108. Against current main, 19 of 175 gas labels regress,
down from 24: Maple's five approve regressions are gone. OZ mint and Flash fee
costs improve to +3; the remaining getter debts persist. Fractional's seven getter calls now cost
one additional gas each, down from ten. Compiler-time debts
remain.
The backend has 17,552 physical lines in 48 files, 17,086 fewer than the deletion
inventory. Excluding trailing test modules leaves 16,247 physical lines. A
retained count-only baseline reports 29,006 production-section lines, giving a
conditional reduction of 12,759; that file lacks a revision/hash link to the
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
establish complete functionality. Remote CI is tracked for each pushed head; passing CI does
not close the performance acceptance debts above.

[pr]: https://github.com/paradigmxyz/solar/pull/1388
[literal]: ../target/codegen-bench/evm-rewrite-candidate/literal-cache-workflow-20260908/
[cold]: ../target/codegen-bench/evm-rewrite-candidate/cold-expectation-migration-20260908/
[group]: ../target/codegen-bench/evm-rewrite-candidate/size-long-tail-group-workflow-20260908/

[unit]: ../target/codegen-bench/evm-rewrite-candidate/unit-carry-physical-workflow-20260908/
