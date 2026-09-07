# EVM rewrite progress

The rewrite is incomplete. Remaining UI assertions and individual generated-code
gas/size regressions block acceptance. The draft is
[PR #1388](https://github.com/paradigmxyz/solar/pull/1388). The
[handoff](evm-rewrite-plan.md) defines the contract;
[checkpoint history](evm-rewrite-checkpoints.md) and small commits retain earlier
milestones. Detailed evidence below lives under
`target/codegen-bench/evm-rewrite-candidate/`.

## Main integration and scheduling research

The latest main integration targets `8d553ca1` (27 incoming commits). The fresh
backend was preserved while retained MIR/CLI interfaces and tests were merged.
Debug-output transport is implemented and the workspace compiles without
warnings. The first merged CI-profile run has 1,388 passes, one UI aggregate
failure and two skips; expanded UI has 11,227 passes, 153 failures and 849
filtered revisions. CI repair is in progress. The verification ledger below
describes the pre-merge compiler, not acceptance of this merge.

[Stack scheduling research](evm-stack-scheduling-research.md) compares pinned
solx, Venom and Sonatina source. The isolated 17-word entry trial remains in
`stack-entry-window-trial-20260906/`, with its frozen compiler and source patch.
It saves size in two nested-storage fixtures, but focused size-mode calls include
gas increases and GAS/MSIZE observations change. It is not accepted or included
in the merge. The first symbolic attempts omitted the required stateful flag and
are recorded as incomplete, not agreement.

The independent pre-merge test audit is in
`test-harness-integrity-audit-20260906/`. It found zero standalone fixture or
snapshot deletions/moves, no removed/narrowed original runtime calls, revisions,
compiler flags or ignores, and no tracked harness changes. There are 149 added
source fixtures and 209 added snapshots. All 28 modified original source files
were reviewed; 26 change comments only, and two equivalent structural-branch
rewrites have retained no-pass byte-identity evidence.

This does not mean no tests were removed: the authorized backend deletion
removed 204 old embedded tests, replaced so far by 36 fresh backend helpers.
The 63 other codegen tests remain, explaining the net 168-test workspace drop.
The audit does not certify one-to-one requirement replacement. Eleven early
pattern-expectation changes have static/unchanged-input evidence rather than
individually located historical runtime certificates. These limits and all 119
modified snapshot hashes are retained; no failing tests are silently waived.

## Scope and architecture

Deletion `e5ba34f2d3676b493aed6dc120c90a1b16605c0a` matched all 40 agreed files; nothing outside scope required
restoration. No deleted implementation was read or recovered. The fresh
unsupported API milestone compiled before functionality returned.

The backend separates private stack scheduling and frame/spill planning,
MIR instruction selection, physical block IR transforms, deployment construction
and primitive assembly. The assembler writes fixed bytes into one buffer with
sparse relocations and computes the least fixed point of label positions and
PUSH widths. There is no Atom stream or assembly-level CFG optimization. MIR
semantics remain in their retained layers. No legacy backend or temporary
unsupported rewrite fallback is used.

The accepted scope has 12,983 Rust lines across 37 files, versus 34,638 deleted
raw lines: 21,655 fewer (62.5%). Counts include comments, blanks and local tests.
A historical production-only count was not retained and is not reconstructed
from forbidden source.

## Current verification

The current workspace run has 1,358 passes, one failing UI aggregate and two
skips. Expanded UI has 11,034 passes, 41 remaining failures and 806 filtered
revisions. All 99 codegen helpers, Foundry and ordinary workspace/all-target
Clippy pass. The twelve source-memory readback revisions pass, including
unoptimized execution. The nullary-read assertion is now restored; no new
failure ID remains after the narrowly reviewed expectation updates.

The current original-artifact ledger is
`private-terminal-acceptance-review-20260906/`, backed by the separate UI,
heavy and hot audits. The latest terminal-sharing milestone reduces bytecode
without any individual increase; both hot modes preserve gas and observations. All 694
original successful UI IDs and eight known failures match in each mode; 35 added
successes remain separate. Both hot lanes retain 15 runtime cases and 175 ordered
gas labels. Nine heavy captures match the original inputs/settings, 1,672 contract
IDs and 3,344 artifacts, including 1,002 empty outputs and 14 linked-placeholder
artifacts; all 540 reference sites preserve identity, width and content. Five
metadata dictionaries have reviewed, valid offset changes.

Recent expectations were reviewed with actual execution before updating:

| Reviewed group | Evidence | Result |
| --- | --- | --- |
| Embedded children and dump output (`c062ba84aba96d632bb7ee5d6e193bb909449b56`) | 36 compiles, 90 executions, 32 optimized size and 20 gas comparisons | All oracles agree; no optimized increase |
| Code-object copies (`ed1b467f`) and Paris memory copy (`ffdc9c7d`) | 27 compiles, 99 executions; exact data/padding and label reconstruction | All oracles agree; no optimized increase |
| Equal immediates (`ce287764`) | 9 compiles, 135 executions; six FileCheck replays | Optimized output saves two bytes and valid calls save five opcode gas |
| Mapping storage (`fc1154ff`) | 54 executions; exact hashed-field storage traces | Optimized output saves two bytes and successful calls save 21 gas |

Evidence is retained in `writer-snapshot-runtime-review-20260906/`,
`code-object-mcopy-review-20260906/` and `equal-immediate-review-20260906/`.
Unoptimized gas increases remain explicit. Four Solar comment-amendment pairs
are byte-exact for each changed source. The code-object solc pairs differ only
inside independently parsed IPFS metadata digests; the initial failed equality
assertion and complete CBOR audit remain retained. The latest derived sealed
source-join report is
`absolute-range-root-expectations-20260906/sealed-ui-derived-for-future.json`;
its 1,404 original rows and totals are unchanged. The mapping proof initially
rejected different counts of comment-only blank lines before any mutation; the
corrected noncomment-line comparison and four exact bytecode pairs are retained.

Replay-confirmed bounded differentials cover internal stack returns, selected
ABI/termination cases, exact-stride switches and initializer controls. Writer,
SF and arithmetic runs that hit their bounds remain incomplete. Concrete
execution and independent models supplement those bounds; they do not turn an
incomplete symbolic run into a proof. Known sealed sparse-writer miscompilations
remain separately recorded rather than adopted as an oracle.

## Compiler time

These sequential debug-compiler measurements use the same archived Seaport input
and check all 432 contract outputs. Isolated speedups are not additive.

| Isolated change | Paired wall time | Sampled maximum RSS |
| --- | --- | --- |
| Memoize recursion reachability | 66.10→60.10; 65.60→55.62 s | +8.7% / −2.7% |
| Stop completed stack counts | 54.83→52.07; 55.84→51.59 s | +3.1% / +5.5% |
| Count missing copies once | 52.98→52.16; 53.09→52.28 s | −2.7% / +0.9% |
| Bound futile perfect shifts | 52.61→52.20; 52.31→51.87 s | +2.2% / +0.6% |
| Bitmap writer word offsets | 52.87→52.81; 52.71→52.89 s (flat) | +0.2% / −7.8% |

At the bounded-search checkpoint, the uncontended sealed/current pair is
54.71→52.59 seconds (−3.9%), with
sampled maximum RSS 823,252→601,332 KiB (−27.0%). Another pair is 55.85→52.55
seconds, but its current leg overlapped a roughly half-second Python model and
is not treated as isolated. Raw captures and the concurrency limitation remain
in `bounded-search-final-20260906/`. Future quiet windows exclude CPU-heavy models.

Combining return classification scans was rejected and reverted despite exact
UI/hot/heavy output: paired times were 54.91→54.32 and 52.61→54.19 seconds
(−1.1% / +3.0%), providing no reliable speed benefit. The six-line reduction does
not justify that uncertainty. `return-classification-trial-20260906/` retains all
checks, measurements, original source snapshots and the rejection decision.

A fresh writer-delta profile attributes 29.2% of sampled stacks to verification
and 15.5% to scheduling (inclusive scopes overlap). It predates the two latest
speed changes. Its complete output and warning multiset match the quiet capture.
Open `current-cpu-profile/writer-delta-refresh-20260906/profile.json.gz` with samply.
Validation remains enabled; measurements use the debug compiler in this checkout.

## Output quality still owed

The strict ledger joins original IDs and ordered call labels. Aggregate wins
cannot waive individual regressions.

| Matched corpus | Creation-byte delta | Runtime-byte delta | Call-gas delta |
| --- | ---: | ---: | ---: |
| UI, gas (694 original successes) | −7,630 | −5,864 | — |
| UI, size (694 original successes) | −33,537 | −28,989 | — |
| Hot, gas (15 cases) | +17,572 | +18,009 | −27,793 |
| Hot, size (15 cases) | +25,654 | +26,026 | −92,547 |
| Heavy projects, original settings (9 cases) | +17,823,285 | +14,689,773 | — |

There remain 704/507 larger UI artifacts, 19/19 larger hot artifacts and 24/30
higher hot gas labels in gas/size mode, plus 1,079 larger heavy artifacts.
SeaportRouter runtime is 24,648 versus sealed 9,822 bytes, down from an earlier
rewrite's 48,812. Heavy captures establish size debt, not arbitrary runtime
correctness. Required observer and arbitrary-memory correctness guards remain.

Selected-home protection and address-delta reuse removed substantial writer
code without individual increases; their measured compiler-time costs remain
explicit in `gas-writer-protection/`, `writer-delta-heavy-20260906/` and
`router-writer-delta-20260906/`. Size mode keeps ordinary backups because all-mode
trials displaced profitable outlines. A cap16 automatic switch trial saved size
and aggregate gas but was rejected: a default path cost 167 versus sealed136.
Wide indexed encoding, perfect scratch reuse and canonical append shortcuts were
also rejected for measured size or timing failures. Their artifacts remain;
none is in production.

Bitmap writer selection now keeps word offsets until final address conversion
(`8e3ccbd1`). All 2,616 focused calls match the required oracles and historical
sealed exclusions. The 130 successful sparse gas labels each save three gas;
contiguous, none and size output stay exact. All 108 unique raw boundary pairs
pass, including 22 successful exact-1024 cases and 27 required 1025 failures.
The first harness lacked LT for its unchanged contiguous control; its failure
is retained. A fresh sparse symbolic run times out at 60 seconds, with no
agreement or confirmed mismatch; size mode was not run within that shared cap.

The UI screen saves 13 creation/runtime bytes in gas mode (11 from original
sources), Nitro saves 162, and nine projects save 245,594 creation and 200,414
runtime bytes. There are no individual increases. All hot gas labels and 26
diagnostic pairs remain exact. The independent heavy audit verifies all 540
library/immutable sites, including 19 relocated tables, and seven projects'
exact code-size warning changes. Evidence is in `writer-bitmap-words-20260906/`,
`writer-word-index-{trial,heavy,review}-20260906/` and the current sealed ledger.

The frozen packed-table count screen covers every count2..128, with exact
count33 calibration and 21,209 candidate label checks. None closes the retained
selector-gas debt, so it supplies no production policy. Its initial implicit-STOP
oracle failure is retained in `packed-bucket-count-screen-20260906/`.

A constant opcode stack-effect table was rejected and reverted. All 256 effects
match in compiled Rust and all 432 timed contract outputs stay exact, but times
53.02→52.98 and 53.02→53.21 seconds show no reliable benefit. Its 12 added lines
and 768-byte static table are absent. The initial pure-harness assertion confused
SELFDESTRUCT with INVALID; the corrected exhaustive check and original failure
are retained in `opcode-stack-table-trial-20260906/`. Broader suites were not run
after the timing rejection.

Successor-aware preparation for acyclic comparisons was rejected and reverted:
LValueEvaluationOrder grows from 530 to 531 runtime bytes, already above sealed
428. Four removed SWAP2 bytes are outweighed by five net label-immediate bytes
when empty-edge removal changes layout. Its three calls remain correct and save
6/6/0 gas; aggregate gains do not waive the size failure. The targeted wrapper
never activates the new rule because its canonical edge order is unchanged.
All helpers, Clippy and Foundry pass, and the workspace retains the same 62 UI
failures. Heavy and timing lanes were not run after the decisive size rejection.
Evidence is in `comparison-branch-order-trial-20260906/`,
`comparison-branch-order-runtime-review-20260906/` and
`branch-order-lvalue-attribution-20260906/`.

A global terminal-owner guard was also rejected: requiring an existing
JUMPDEST avoids a one-gas fallthrough cost but retains duplicate bodies and
creates 16 new or worsened sealed size debts. The original pass explicitly
permits that observer-free tradeoff. Both conservative and complete-addressability
trials, their 32 raw control executions and the withheld regression fixture are
retained in `terminal-owner-{label-fix,addressability}-20260906/` and
`private-terminal-redirection-tests-20260906/`. The refined heavy lane matched
three complete projects before encountering changed OpenZeppelin output; later
projects and timing were not run. The proposed private-control extension must
preserve old observer-free decisions and prove any forwarded-gas restriction
with execution evidence before changing them.

Allowing live MSTORE operands within the existing sixteen-word window was
rejected and reverted despite 700 correct focused calls and smaller UI output.
The nested-struct audit's 48 calls all return correctly, but memory copying costs
14 more gas under -Ogas and 243 more under -Osize, worsening sealed debts.
All nine heavy projects compile with identical IDs and valid reference tables;
eight artifacts grow despite aggregate savings of 631,797 creation and 538,576
runtime bytes. Router falls from 24,648 to 24,313 runtime bytes, legitimately
removing one 24,576-byte warning. The first strict audit rejected that missing
warning; its corrected threshold proof and all eight increases remain explicit.
No quiet timing was run. All helpers, Clippy and Foundry pass; workspace retains
exactly 62 UI failures. Evidence is in `live-mstore-residents-trial-20260906/`,
`live-mstore-residents-heavy-review-20260906/`,
`live-mstore-residents-study-20260906/` and `live-mstore-nested-replay-20260906/`.

The committed terminal fix (`ee916cdb`) requires an existing owner label when
the module forwards gas. On the false path, the old added JUMPDEST changed a
child's returned GAS from 75,170 to 75,169 at a 100,000-gas limit. The actual fix
restores the expected result in all 18 CALL replays; 40 observer-free controls
remain byte/gas/outcome exact. The regression fixture fails the old pass and
passes the fix. All UI output, both 15-case/175-label hot lanes, 26 diagnostic
pairs and nine complete project JSON outputs remain exact. Evidence is in
`terminal-forwarded-owner-fix-20260906/` and
`terminal-forwarded-owner-final-replay-20260906/`. Quiet times increase
53.07→57.30 and 53.20→54.88 seconds (+8.0% / +3.2%); sampled maximum RSS
falls 614,808→585,260 and 633,120→600,100 KiB. Full 432-contract output stays
exact. The followup defers the observer scan until an actual duplicate needs it;
the measured slowdown was investigated rather than waived. The accepted lazy
followup (`728f8df7`) queries observers only for an actual unlabelled duplicate,
with one cached answer. Its 45,242-case model preserves the exact redirect map;
all corpus/diagnostic output and six focused disassemblies remain exact.
Three-way timing retains both original/eager/lazy orders: 54.82/60.36/53.48 and
53.01/54.91/53.91 seconds. Lazy improves both eager comparisons; against the
original writer the mixed −2.4%/+1.7% result shows no consistent remaining
slowdown in these captures. Evidence is in `terminal-observer-lazy-20260906/`,
`lazy-terminal-forwarded-review-20260906/` and
`terminal-observer-lazy-focused-20260906/`.

An early four-instruction literal-orientation rule was rejected and reverted.
It saves one byte in AcyclicStackPhi and aggregate UI bytes, but 16 artifacts
grow and 12 worsen sealed debts. FunctionPointerDirtyBits grows ten bytes:
reordering consumes a high-bit mask before later CSE can reuse it, requiring a
second compact mask at each of two sites. Both hot lanes pass. No heavy or quiet
timing lane followed the size failure. The exact local rule and a separate late
block-IR placement remain under review in `literal-orientation-trial-20260906/`
and `acyclic-literal-orientation-draft-20260906/`.

The late placement is accepted in `9c1b7639`. The final named pass preserves
earlier constant reuse and rejects observers, unproved control and indexed
jumps. Its 244 focused executions and two fresh bounded solsymdiff runs agree.
No UI or heavy artifact grows: creation/runtime each shrink 562 bytes in gas
mode and 698 in size mode; heavy totals shrink 902/650. Hot arithmetic calls
save 30/150/300 gas per mode, with every other ordered label unchanged.
The function-pointer regression is byte/gas exact. All 540 heavy reference
sites and measured warning values validate. Quiet times rise 53.06→53.18 and
52.79→53.11 seconds (+0.2% / +0.6%); sampled maximum RSS rises 0.7% / 0.5%.
That small compiler cost is retained for the output-quality gain. Evidence is
in `late-literal-orientation-{trial,runtime,heavy-review}-20260906/` and
`late-literal-symbolic-20260906/`; the tracked fixtures cover both old and modern
forks, metadata, observers and control exclusions.

The committed compiler-time change (`9897cc1a`) shares normalized stack bounds and unknown-jump
status between local-pass callers. Raw encoding validation remains separate,
including concrete overflow rejection beside an unknown edge. All UI/hot/heavy
outputs and 53 focused exit/stdout/stderr pairs are exact; workspace retains the
same 62 failures. Its initial focused harness missed eight inline error
annotations; the compiler results were already identical and the static
classification correction is retained. Quiet paired times improve 53.22→53.01
and 53.71→52.71 seconds (−0.4% / −1.9%); sampled maximum RSS rises
582,352→634,640 and 581,912→634,880 KiB (+9.0% / +9.1%). This memory
tradeoff remains explicit in `local-stack-facts-trial-20260906/`.

Reordering tail-merge observer guards was rejected and reverted. Its complete
432-contract timed outputs are exact, but paired compile times rise
52.71→54.02 and 53.70→54.12 seconds (+2.5% / +0.8%). No broader test lane
followed this compile-time rejection. Evidence is retained in
`tail-guard-order-trial-20260906/`.

An FMP-provenance screen found no eligible runtime root under the retained
no-reset analysis, so no broader memory-disjointness assumption was introduced.
A followup checked Router's 42 FMP stores: none restores an exact saved SSA
value, and only five allocation ends have complete carry/upper-bound guards.
Those guards do not prove a lower bound after an arbitrary source FMP reset.
Covering repeated writers needs loop ranges and successful-return effects beyond
the retained facts; no speculative backend analysis was added. Static evidence
is in `fmp-checked-restoration-study-20260906/`.
Current work examines private-control-aware duplicate exit removal. Further
residency changes require evidence that their added shuffles pay for themselves. The acyclic phi's
five-byte debt includes a removable four-byte duplicate revert; its FMP
initializer already saves one byte. The library wrapper remains two gas above
sealed; loop short paths and acyclic phi size debts remain withheld in
`library-phi-review-20260906/`. Finish supported functionality,
resolve every expectation, and repeat full workspace/UI, Foundry, differential,
both size corpora, all project compilations and identical-label hot lanes on the
final state. Require no per-case size or gas regression under -Ogas or -Osize,
then finalize compiler time/RSS, LOC and the candidate evidence archive.

The byte-store selected-home extension was rejected for compiler cost, with no
measured semantic defect. It saves 108 creation/runtime bytes and 145 opcode gas
per exercised path in an added rewrite fixture; a new sparse fixture saves 156
bytes. The common sealed UI, hot and heavy inventories remain byte/gas exact.
Quiet times rise 53.07→53.60 and 53.01→53.71 seconds (+1.0% / +1.3%), confirming
an earlier +1.3% / +1.5% pair. An output-identical loop variant is slower again
at 54.30 / 54.37 seconds. Both variants were removed using fresh rewrite
snapshots, preserving all evidence in `writer-byte-{store,loop}-trial-20260906/`.
The 1,257 focused executions agree; only a fixed-destination symbolic probe
reaches bounded agreement in both modes. Sparse arbitrary writers have concrete
coverage, not a complete symbolic proof. The new sparse regression is retained
in `925db056`; all four matrix revisions pass with both the accepted compiler
and byte-store trial, with the same MIR snapshot. Full workspace coverage before
that new fixture retains the same 62 failures. No sealed debt was removed.

Raw validation reuse is accepted in `a904c535`. Assembly reuses concrete bounds
only when indexed lowering borrows the original immutable graph; every owned
attempt gets fresh analysis. Public validation and optimization bounds remain
unchanged. The 63 focused exact-output pairs include a proved borrowed-to-owned
width retry and concrete overflow alongside unknown control. All 725 UI sources
per mode, both hot lanes (15 cases / 175 ordered labels each), and 3,344 heavy
artifacts are byte-exact. All 540 reference sites and 193 diagnostic blocks are
unchanged; three projects only reorder their warnings. Helpers, ordinary Clippy
and Foundry pass. Workspace has 1,358 passes, one UI aggregate failure and two
skips; expanded UI has 10,983 passes and the same 62 failures. Quiet times improve
53.17→53.11 and 53.42→53.16 seconds (−0.1% / −0.5%); sampled peak RSS falls
633,540→597,128 and 598,176→581,824 KiB (−5.7% / −2.7%). These small compiler
improvements remove no sealed output debt. Evidence is in
`assembly-raw-facts-{trial,focused,heavy-review}-20260906/`.

Disjoint OR-mask absorption is accepted in `2c93d841` (+9 production lines).
Existing metadata, unknown-control and whole-module observer guards protect
`(x | A) & B -> x & B` when `A & B == 0`. Only FunctionPointerDirtyBits changes
in the UI size corpus: gas creation/runtime shrink 39/38 bytes, size shrinks
24/24. Its two functions each save 18 gas in gas mode and 21 in size mode.
All 162 focused executions agree, including full-stack, overlap and observer
controls; four original pinned-solc calls agree. Two separate pure-mask symbolic
runs reach bounded agreement, but their before/after bytecode is identical, so
they support the algebra rather than demonstrate backend activation. The exact
function-pointer source observes its own address and is outside that symbolic
lane's supported scope. All hot and heavy output remains exact. Quiet times are
mixed: 52.86→53.36 and 53.18→52.97 seconds (+0.9% / −0.4%); sampled RSS falls
634,988→582,336 and 633,580→617,676 KiB (−8.3% / −2.5%). No compiler-speed win
is claimed. Evidence lives in `disjoint-mask-{trial,focused,differential,heavy-review}-20260906/`.
Four sealed UI artifact debts are removed; 758 gas-mode and 548 size-mode
artifact debts remain, along with all 1,101 heavy debts and hot gas gaps.

Absolute unknown-length ranges are narrowed in `dea1a0df` (+10 raw Rust lines,
including six helper assertions). Only an absolute start at or beyond a checked
protected-word end proves disjointness; relative and unresolved starts remain
conservative. All optimized UI/hot/heavy outputs are exact. The 1,694 focused
executions include boundary, overflow, memory-size and dynamic-frame controls;
a pressure-copy case saves 93 optimized runtime bytes and 141 opcode gas.
Symbolic RETURN-size probes remain incomplete. Quiet times are mixed:
53.01→53.18 and 53.01→52.81 seconds (+0.3% / −0.4%), with sampled RSS
585,840→604,316 and 597,448→581,584 KiB (+3.2% / −2.7%). No compiler-speed
win is claimed. Reviewed expectations in `18c8437f` reconstruct all 14 embedded
objects and exact padding. Full workspace UI failures fall from 62 to 44,
including two Standard JSON fixtures restored without blessing. Evidence lives
in `absolute-unknown-range-{trial,focused,heavy-review,json-focused}-20260906/`
and `absolute-range-{new-ui-review,root-expectations}-20260906/`.

Two additional supported-functionality gaps now have concrete witnesses.
`writer-readback-probe-20260906/reduced18/` shows compiler spill restoration
replacing an unannotated assembly write before a later source read: current and
sealed return 1 and 2 instead of pinned solc's `0xdeadbeef`. This is not adopted
as an expectation. `recursive-copy-depth-review-20260906/` isolates a
15-argument recursive copy rejected with `InaccessibleDepth`; sealed and solc
execute 24 reference calls successfully. Pressure planning omitted saved
protocol words. Both defects predate the absolute-range change. The scheduler gap is now
fixed below; broader memory visibility remains a completion blocker alongside
the measured gas/size debts.

Protocol-aware pressure planning is accepted in `d5655a62` (+131 raw Rust lines),
with regression coverage in `6ed236ff`. The existing checked scheduler includes
possible saved control words; a single recheck handles the first dynamic frame
created by spills. Absolute-only pre-layout overlap checks preserve the existing
13-argument DUP16 boundary while the previously rejected 15-argument recursive
copy now executes. An initial maximum-only variant was rejected for +580 runtime
bytes and up to +2,999 gas on the supported boundary case; all evidence remains.
The refinement has 324 focused oracle passes, 72 comparable gas rows and 36
artifact rows exact. Both modes reach bounded solc agreement on an activated
fixed-depth/copy subset; the initial path-limit failure is retained. Two mixed
static-parent/dynamic-callee controls have 48 candidate/sealed passes with traced
frame transitions; pinned solc rejects them as stack-too-deep. All UI/hot/heavy
outputs and 540 reference sites remain exact, with the same 44 workspace UI
failures. Quiet times improve 53.21→52.58 and 52.81→52.51 seconds (−1.2% / −0.6%);
sampled RSS is mixed, 609,460→596,980 and 594,712→599,264 KiB (−2.0% / +0.8%).
The measured binary precedes a same-order nested-if style cleanup; the final
binary has three complete focused compiler-output pairs exact, four UI revisions
and ordinary Clippy passing. Evidence is in `protocol-pressure-focused-20260906/`,
`protocol-overlap-pressure-{trial,heavy-review-v2}-20260906/` and
`protocol-pressure-final-style-20260906/`. The fresh scope recount corrects a
one-line understatement in the prior progress total. The next isolated trial
rematerializes immutable calldata reads to avoid their source-visible spill homes;
that is a targeted repair, not a general unsafe-assembly memory solution.

The broad calldata-rematerialization trial is rejected. It repairs all fourteen
original Solidity readback calls, with actual source-write/later-MLOAD traces;
all 68 candidate calls and 14 pinned-solc calls meet their oracles. Seven previous
candidate rows and nine sealed rows retain their known wrong results. However,
it creates or worsens 330 sealed UI artifact debts (195 gas / 135 size), including
632 immediate size increases. The gas hot lane preserves all 175 call results and
gas values but grows ten artifacts across five contracts. Heavy, size-hot and
quiet timing were not run after that rejection. Repeated ABI-head reads replace
stack reuse, growing a representative sequence 18→19 bytes at equal static gas.
Evidence is in `calldata-rematerialization-{trial,sealed-ui-review}-20260906/`
and `calldata-remat-small-growth-review-20260906/`. The next scope considers only
values the ordinary planner already homes, preserving its layout and protocol.


Calldata-home rematerialization is accepted in `0436448d` (+109 raw Rust lines),
with three source readback fixtures in `e4e733c0`. Recipes replace only ordinary
spill homes after frame planning; reserved words and the original spill protocol
remain fixed. Gas mode selects only entirely eligible home sets, preserving mixed
compact writer banks. Other modes select each eligible immutable read. There is
no additional planning pass. The intermediate partial-home variant grew Navigator
creation/runtime by 322 bytes: removing one home disabled eleven compact writer
templates. An all-mode uniform variant then lost two none/mir readback checks.
Both rejected candidates and exact attribution remain preserved.

The final shape has 68 focused candidate and 14 pinned-solc passes, twelve new
UI revisions passing, and fourteen traced source-write/later-MLOAD pairs without
intervening overlapping writes. Seven previous and nine sealed wrong results
remain explicit; ten additional pinned-solc calls supplement the original-source
oracles. Fresh symbolic checks in both modes remain incomplete (solver unknown),
with identical source/settings fingerprints; no agreement is claimed. All 4,000
original UI artifacts and 3,344 heavy artifacts remain byte-exact. Added-source
creation/runtime totals shrink 1,652/1,648 gas bytes and 2,213/2,209 size bytes;
there are no individual increases. Both hot lanes preserve outputs and gas;
size-mode Nitro alone shrinks 24 creation/runtime bytes. Quiet compiler pairs
are 59.53→52.48 and 53.02→52.51 seconds; the unusually slow first baseline makes
its larger gain uncertain. Sampled RSS is 634,688→633,576 and 634,884→635,056 KiB,
essentially flat. Every timed JSON matches its reviewed 432-contract capture.
Evidence is in `bank-preserving-calldata-{trial,heavy-review,sealed-ui-review,
timing}-20260906/` and `homed-calldata-navigator-review-20260906/`.

This remains a targeted repair: other private homes and protocol words can still
interfere with unannotated assembly. Unchanged frame reservation does not prove
source-memory, MSIZE or FMP invariance. That defect, the remaining 44 UI assertions
and individual sealed gas/size debts continue to block completion.

Two embedded-child MIR snapshots were subsequently reviewed with 69 successful
runtime calls, including complete 228-byte producer/wrapper revert payloads
matched to solc and reconstructed immutable targets. The LongReturn object
remains 243 bytes; RevertingProducer grows 76→81 bytes to initialize FMP128,
fixing the old payload's zero at byte 95. All seven embedded children match their
standalone objects; only the reviewed literals and derived lengths are refreshed.
Eight matrix revisions pass. The last full-workspace count above predates this
narrow refresh. Evidence: `embedded-child-{mir-review,root-refresh}-20260906/`.

`computed-writer-readback-witness-20260906/` confirms the remaining interference
with an exact reduced18 variant using `xor(value,256)`: current gas output at
address 192 is `(257,4779)` instead of solc's `(0xdeadbeef,4779)`. The 4096 control
passes both. Traces show the source MSTORE followed by a compiler restore of 257
and a source MLOAD of 257; this result is not promoted to an expected value.

Stable nullary reads are restored in None mode by `cdac80a0` (+64 raw Rust
lines). Planning, residency and emission share one classifier; NUMBER, mutable
reads, noncanonical effects and unavailable opcodes retain ordinary evaluation.
Optimized modes keep their previous scheduling. The original all-mode variant
was rejected for individual gas/size regressions. Two None-only drafts cost
4.3–4.9% and 1.5–1.6% compiler time; a single value classification reduces that
cost to 1.013% and 0.948% (52.363→52.893 and 52.319→52.815 seconds). This is an
explicit functionality tradeoff, not a compiler-speed win. Sampled RSS is
592,864→597,304 and 586,348→636,404 KiB. None-corpus successful-case time improves
1.653% and 0.354%; its 22 growing objects remain recorded alongside aggregate
creation/runtime reductions of 5,927/5,842 bytes.

All optimized UI and heavy objects and both 175-label hot lanes remain exact.
The focused suite passes 48 calls; a fresh activated caller-reuse symbolic case
reports bounded agreement, with its limits retained. Snapshot commits
`756a00a8`, `250dfa28` and `b222c9dd` follow 26 embedded-child reviews, 54 observer
calls and six real deployments with 42 boundary calls. The single source oracle
changes 276→277 because runtime grows 116→117 bytes while FMP stays 160. Six
compiler/mode pairs prove that comment edit leaves complete artifacts unchanged.
All 35 reviewed IDs and 147 selected/sibling revisions pass without blessing;
the subsequent full workspace leaves exactly 41 old failures. Evidence is in
`nullary-single-match-{trial,acceptance,timing,refresh-plan}-20260906/` and the
independent `nullary-single-match-*-review-20260906/` directories. The authoritative
timing audit is `nullary-single-match-timing-review-20260906/corrected-v3/`;
initial harness and audit failures remain preserved.

Private terminal redirection is accepted in `0ae64fbe` (+18 raw Rust lines).
Private return-label targets stay protected; shared owners must already be
addressable, with gas-observer and unknown-control checks before mutation.
Across all 729 UI successes, runtime shrinks 9,203 gas bytes and 2,681 size bytes.
Heavy runtime shrinks 9,595 bytes; hot creation/runtime shrink 145 gas bytes and
56 size bytes. No individual artifact or gas label increases. Remaining sealed
artifact debts fall to 704/507 UI, 19/19 hot and 1,079 heavy. All 540 relocation
sites retain valid identity, width and contents, including five offset changes.

The independent focused suite passes 216 calls over 54 public-driver captures;
return targets and forwarded-child gas remain intact. Nine real deployments and
117 mapping/storage boundary calls justify the sole snapshot update `18e3944e`.
Four fresh bounded symbolic agreements use two internal-call signatures in both
modes; exact same-input captures prove the candidate symbolic bytes and actual
20-byte runtime reductions. An initial cross-input UI identity assumption and
pretty-versus-canonical JSON hash preflight failure remain preserved separately.
Quiet pairs are 52.469→52.598 and 59.400→52.410 seconds: the first is essentially
flat (+0.246%); the unusually slow second baseline does not establish a large
speedup. Sampled RSS is 599,932→582,184 and 634,540→585,108 KiB. All timed outputs
match their 432-contract reviewed captures. The full workspace returns to the
same 41 old failures. Evidence is in `private-terminal-{trial,focused,
mapping-oracle,root-acceptance,timing,timing-review}-20260906/` and the independent
`private-terminal-acceptance-review-20260906/` ledger.

A proposed FMP-interval certificate was rejected before implementation. The
current Router census identifies 33 potentially interesting protected stores,
but unchecked allocation bumps and 89 unresolved writer destinations leave zero
proved removals. No annotation or blanket heap-disjointness assumption was added.
The general assembly-memory interference defect remains open. See
`router-reservation-census-20260906/` and `fmp-interval-adversarial-review-20260906/`.

## Evidence provenance

The sealed archive SHA-256 remains
`7ddbbe60c1305e0fbb411afdc2a7652ee55ac2b2ad71c668995695bd86bde5db`.
All 529 baseline evidence checksums and 2,376 source fingerprints were verified.
An earlier profile capture overwrote three candidate files; their original bytes
are lost. The old profile/analysis and sealed archive remain intact, with the
limitation recorded in
`current-cpu-profile/gas-writer-20260906/artifact-provenance-note.md`.
New artifact directories are created exclusively. Prior progress text is retained
in `equal-review-final-20260906/progress-before-condense.md` and earlier commits.

### Main CI repair: gas reserve scheduling

Merged main `8d553ca1` in `2d5f077f` and pushed the merge to draft PR #1388.
The first CI run reached all jobs; runtime benchmarks, docs, feature builds and
WASM passed. Forty full snapshots were independently recaptured and changed
only for canonical `icall`/`!metadata` spelling. No test inputs or execution
oracles changed. Subsequent publication of local fixes is currently rejected
by automatic approval review despite the existing push authorization.

Pre-EIP-150 lowering now prepares call operands and spill backups before the
adjacent `GAS; SUB; CALL` sequence. Semantic adjacency metadata survives parsing
and blocks rewrites that would insert work inside that reserve. Frozen compiler
`8f88f1f26b6e98c1e65e100478bc47e09687559aed17cb07eb32e74215376e3f`
passed 1,388 workspace tests; the UI aggregate has 95 failures, down from 153
(11,285 UI cases passed, 849 filtered). All 15 previously reported Homestead
out-of-gas failures disappeared. Remaining snapshots can mask later runtime
checks, so this is not complete runtime admission. Evidence is retained under
`main-ci-reserve-20260906/`; compiler sources stayed fixed through the build
and workspace run. CI-style Clippy exposed additional MSRV lint findings;
those remain a separate repair. Full rewrite performance acceptance is open.

### Main CI repair: provenance and test admission

Source/debug origins now survive copy packing, physical rewrites, constructor
exits and lowered invocation boundaries. Exiting stack cycles terminate
validation without weakening strict optimization facts. All 4,776 UI objects
remain identical across those fixes. The full workspace now passes 1,389 tests
with only the UI aggregate failing: 11,346 UI cases pass and 35 fail, with
849 filtered. Clippy with warnings denied and nightly formatting pass locally.

Reviewed snapshot/check changes retain executable sources and their runtime
oracles. Admission includes 224 child calls, 28 debug-setting checks, eight
library calls, and 261 switch deployments/calls; these counts describe separate
suites, not unique programs. Eleven debug and thirteen lowering mutations
confirm retained assertions reject incorrect output. Switch flags still force
all five algorithms, and the shared constructor/runtime growth budget remains
checked. The switch commit follows the full run and resolves twelve of its
failing IDs in focused checks; a new full run is pending.

Tail sharing now accepts private physical branch pairs and proves no gas
observer can follow an added transfer. Size mode retains its conservative
guard after the unrestricted trial grew PrecompileBuiltins by ten bytes and
increased seventeen Aave call labels. The restricted candidate preserves all
4,776 UI objects and thirty hot compiler outputs exactly, transferring the
existing fifteen-case/175-label-per-mode runtime certificate by full object and
input identity. No new execution is claimed for that transfer. Evidence is in
`main-ci-{provenance-neutrality,standard-json-review,standard-json-check-migration,
switch-review,tail-gas-independent}-20260906/`.

Main is merged and draft PR #1388 exists. Automatic approval review still blocks
publishing the subsequent local commits despite the prior push authorization.
The explicit destination approval remains pending. CI is not green yet, and
the original sealed gas/size debt and computed-memory interference bug remain
open. The current cheap-environment-copy and clone-sharing trials are not
accepted performance results.


### Main CI repair: late environment reads and remaining sharing

The accepted `environment-copies` pass runs after literal orientation. It
replaces legacy DUPs of known, stable two-gas environment values with fresh
reads, retaining ordinary CSE identities until stack normalization finishes.
It adds 117 raw production lines. All 4,776 UI objects and 3,344 heavy-project
objects retain their lengths; only 990 UI DUP/read opcode positions change.
Both hot modes retain all fifteen cases and 175 ordered call labels exactly.
Sixty-four focused calls pass, with thirty-two paired traces retaining every
PC, stack and memory record; operation costs fall by one or two gas per call.
Four bounded symbolic comparisons agree. Two reversed-order Seaport timing
pairs are 0.78% slower; sampled RSS is mixed, so this is not a compile-time win.
The rejected early-CSE trial and its one-byte regression remain preserved.
Evidence: `late-environment-{independent,heavy-review-v2,timing}-20260906/`
and `main-ci-late-read-trial-20260906/`.

The subsequent full workspace run passes 1,389 tests, with only the UI
aggregate failing: 11,420 UI cases pass, fourteen fail, and 851 are filtered.
This run includes an uncommitted whole-block coalescing trial. Its UI screen
shrinks 81 objects without increases, and both hot modes retain all bytes and
ordered gas labels. Eighty focused calls pass. Three gas-mode calls acquire
one JUMPDEST and cost one more gas than the preceding rewrite; they remain
seventeen, five and six gas below the sealed original respectively. The
trial's fixture is still three bytes larger than the original. Heavy-object
review and quiet compiler timing remain pending, so the pass is not accepted.

The remaining CI failures retain assertions for shared return/arithmetic
suffixes, short-message helpers and cold continuations. Those requirements
are being implemented; their absence is not being blessed. Reviewed phi,
ternary, library and data snapshots retain executable sources and runtime
oracles, including thirty additional current/solc storage panic checks.
Publication remains blocked by automatic approval review. The original
performance debt and computed-memory interference bug remain open.


### Main CI repair: block coalescing and cold annotations

Whole-block coalescing is committed with a corrected marker-cost guard. The
adversarial case exposed a retained fallthrough clone acquiring an additional
JUMPDEST; the corrected gas pass declines that case. Its final compiler keeps
all 4,780 UI objects and 3,344 heavy objects byte-identical to the measured
trial, and both hot modes retain fifteen cases and 175 labels. Relative to the
preceding accepted compiler, 81 UI objects shrink and none grow. Four timing
legs of the pre-correction trial show 0.03% and 1.29% lower compiler time;
these are not measurements of the final guard. Evidence is in
`block-guard-{ui-independent,heavy-identity}-20260906/` and
`block-dedup-adversarial-20260906/`.

Cold-path classification now runs after final layout and only annotates IR.
All 4,780 UI objects remain byte-identical. An earlier placement was rejected
because it enlarged 2,302 objects; that trial remains archived. Fifty-two
snapshot migrations change only cold labels, thirteen FileCheck sources keep
their label identities and instruction assertions, and two timing snapshots
add the actual new pass rows. Separate library and cold-call checks preserve
branch polarity, complete failure paths and return behavior. Negative mutations
reject incorrect labels, terminators and control flow. A focused run passes
99 of 100 revisions; the remaining size-mode cold-call requirement is still
unimplemented. Full workspace CI is not green.

A new return-sharing experiment improves sizes but worsens existing sealed
gas debts on tuple and Aave calls, so it is not enabled or committed. Its new
source and 39 new fixture files are preserved with checksums outside active
UI discovery in `return-sharing-held-source-20260906/`; no existing tracked
test was moved or removed. A smaller terminal-word compaction is undergoing
final compiler timing after passing corpus size and actual runtime checks.
The original rewrite performance debts and computed-memory interference bug
remain open. Publishing local commits is still blocked by automatic approval
review despite the earlier user authorization.


### Main CI repair: terminal words and reviewed snapshots

Terminal single-word returns now use scratch offset zero when the immediately
preceding full-word store proves the exact returned value. Canonical effects,
glue, bounded offsets and code-observer exclusions remain checked. This adds
51 net raw production lines. All original 1,582 UI rows and 4,780 objects keep
their success inventory: gas creation/runtime shrink 3,305/3,261 bytes, size
shrinks 2,008/1,976 bytes, and no object grows. Across nine heavy projects,
214 objects shrink and 323 change at equal size; creation/runtime totals fall
1,538/1,520 bytes. ABI, source maps and all 541 logical reference sites per
leg remain valid. Both hot modes retain fifteen cases and 175 ordered labels,
with 94 less gas and no individual increase. The 103-call focused suite has
no gas increases; four bounded symbolic comparisons agree on an extracted
pure subset. The full symbolic fixture is explicitly incomplete because solc
rejects its unrelated MSIZE function under Yul optimization.

The final lint correction preserves selection exactly; four final-compiler
Seaport outputs reproduce the separately audited captures. Reversed timing
pairs are 3.05% faster and 1.90% slower, so there is no reliable speedup claim.
Sampled RSS falls 0.99% and 1.51%; these are 100ms samples, not true peaks.
All 32 new regression revisions pass, as do lint, formatting and typo checks.
Evidence is retained in `terminal-word-{ui-independent,heavy-review,timing,
tests}-20260906/` and `main-ci-terminal-word-{final,symbolic}-20260906/`.

Snapshot commits separately account for terminal offsets, structural jump and
table relocations, deployment lengths and debug instruction offsets. Existing
assertions and executable sources remain intact. A separate array-copy check
migration binds each original selector to its actual decoder, allocation and
copy path, while allowing the getter's direct return. Old/current/solc agree
on 58 calls plus 58 persistent getter reads per leg; 25 negative mutations
reject incorrect paths and memory operations. The frozen final UI run now has
nine failures, 3,578 passes and 113 filtered cases. The earlier full workspace
run passed 1,389 tests; its UI aggregate failed before these reviewed snapshot
updates. CI is still not green and local commits remain unpublished.

A two-word compaction is an uncommitted trial. Unconditional late MemoryDse
was not enabled: its tuple improvement exposes broader gas-observation and
pipeline-cost questions. The original sealed performance debts and computed
memory interference bug remain open.


### Main refresh and benchmark workflow

Fetched and merged main `933bc1e2` as `2c29596e`; no conflicts or incoming
backend implementation changes. All 36 in-progress pair candidate files retain
their hashes. The merged compiler builds, all 76 benchmark-tool tests pass,
formatting and warnings-denied Clippy pass, and the workspace passes 1,395
tests. Its UI aggregate has eight failures, 11,502 passes and 851 filtered
cases. The assembler fixture now retains exact shared body identities and
complete scratch returns: creation/runtime are 110/93 bytes versus sealed
111/94, and all thirteen measured calls use no more gas. This explicitly
replaces the historical cross-value return-tail policy; dedicated tail-merge
tests and executable source remain intact.

The prior pair timing run was interrupted for this requested merge and is not
acceptance evidence. Further benchmarks use the new runtime/compile-time loop
and official `benchmark-compare.py`, retaining samples, artifacts and per-case
comparisons. The first new-workflow pair run uses the retained before/after
debug executables built in this checkout before the merge, isolating the pair
change from main's LSP changes. Compiler hashes and build records certify the
named hard links under the existing `target/debug`; no source implementation
was retrieved to reconstruct the baseline. That comparison is still pending.
The pair candidate remains uncommitted, and full rewrite completion remains
open.


### Two-word admission under the new workflow

Committed `fad8a4f6`: bounded terminal pair compaction adds 71 net raw
production lines. The original 1,582 UI rows and 4,780 objects retain their
success inventory. Gas creation/runtime shrink 316/310 bytes; size shrinks
246/240 bytes, with no object increases. All nine heavy projects retain
1,672 contracts and 3,344 objects: 52 shrink and four change at equal size,
for 108 fewer creation and runtime bytes. ABI, source maps and all 541
library/immutable reference tables, including offsets, remain exact.
Both optimization modes keep fifteen hot cases and 175 call labels, with
four less gas and no increase. The focused 103-call suite agrees with the
sealed compiler and solc on observations; tuple swap saves eleven gas and
multi saves two. Thirty new revisions, fifteen mutation controls and four
bounded symbolic comparisons pass. Eight redundant fixture EOF newlines
were removed with a hash mapping, then all thirty revisions passed again.

The official runtime/compile-time workflow and independent audit retain all
24 case IDs, both compiler labels and 240 artifact pairs. The two changed
Solady MIR files are exact bijective helper renames; executable artifacts
are identical. The Aave change is exactly the pair addresses plus required
constructor length and immutable patch relocations. Compiler time measures
2.84% lower overall, with last-process peak RSS 0.79% higher. PRB Math's
initial 3.95% slowdown does not reproduce in a reversed five-sample repeat
(1.99% faster); neither result establishes a stable compiler-speed change.
Whole-run wall time is not comparable because the long-compile cutoff
changes the actual number of samples. Evidence remains in
`terminal-pair-workflow-20260907/`,
`terminal-pair-official-independent-review-20260907/`, and the retained
pair UI, heavy, focused-runtime and symbolic directories.

Main `933bc1e2` remains merged. The latest complete workspace run passes
1,395 tests but its UI aggregate still has eight original failures. No
original test was removed or ignored. Local commits remain unpublished
because automatic approval review rejected the earlier push. The sealed
performance debts and computed-memory interference witness remain open.


### Terminal arguments and final benchmark workflow

Committed `1b41b7dc`: eligible fixed-range revert arguments now stay on the
physical stack and enter the body's canonical layout directly. Frame
reservations remain fixed. Returning activations, spills, dynamic frames and
code observers decline. The change adds 193 net raw production lines. Four
separate test commits retain ten fixture families: 58 revisions, 216 runtime
executions, 18 FileChecks and 28 negative controls. A further 111-call replay
and the original 43 calls per mode preserve exact observations. Eight short
revert calls save 39 gas each; their runtime shrinks 35 bytes.

An earlier broader candidate was rejected: three expanded literals made
TestERC20 eight bytes larger and propagated into 72 growing objects. The
existing returning-activation fact excludes that case without new analysis;
all 72 objects are restored byte-for-byte. Final Gas UI totals shrink 438
creation and 376 runtime bytes across 35 objects. Sixty-six heavy objects
shrink 914 bytes in each total. No object grows; Size outputs remain exact.
All nine heavy projects retain 1,672 contracts, 3,344 objects and 541 library
or immutable reference tables. The eleven changed source maps preserve all
3,412 known source/event records, with 52 checked label relocations.

The first implementation enlarged the debug compiler's main lowering
function and repeated timings remained slower. Extracting terminal setup
into the call-entry module reduces that function below its original code
and frame sizes. Final full-workflow medians improve across all 24 cases,
with a 4.11% equal-weight geometric mean; last-process peak RSS is 0.38%
higher. A fresh
three-case reversed repeat measures 1.77–4.52% faster with retained RSS costs.
These measurements do not establish a universal speedup. Total workflow
wall time rises because the ten-second cutoff changes actual sample counts.
Both modes retain fifteen hot cases, 175 labels and 139 observations per
compiler, with unchanged gas and deployed bytes. Fingerprint-checked solc
records are reused; they are not new solc timings. All final physical
artifacts bridge exactly through the helper extraction. Raw runs, initial
failures, timing repeats and independent audits remain under
`tail-entry-workflow-20260907/` and the adjacent review directories.

Two original CHECK migrations explicitly replace older sharing policies:
short errors keep one fixed encoder while shifts become local; packed
calldata keeps inline Gas hash/return tails. Runtime bodies and directives
remain exact, with full goldens and 15/27 negative controls retained.
Seven existing packed control-path gas debts per mode remain open. Original
R's two sealed library-value disagreements are independently confirmed
against solc and retained outside comparable gas improvements.

The final workspace run passes 1,395 tests; its UI aggregate has six original
failures, 11,562 passes and 851 filtered cases. Three redundant EOF newlines
in new fixtures have a recorded hash mapping and pass that full rerun. No
retained UI test was deleted or ignored. Current backend files total 15,216
physical lines versus the recorded 34,638 deleted lines: 19,422 fewer
(56.1%). This raw file count includes comments and inline tests. The sealed
performance debts and computed-memory interference witness remain open.
Main `933bc1e2` is merged; local commits remain unpublished after the earlier
automatic push rejection.


### Terminal exchanges and storage expectation migration

Commits `80f702b5` and `281441ee` let terminal two-word return compaction
recognize literal addresses beneath canonical non-top stack exchanges.
The bounded backward scan adds twelve net raw production lines and keeps
instruction order and stack effects intact. Six new EVM IR revisions retain
positive and guard snapshots; eleven negative controls reject. Sixty valid
stack executions preserve complete results, while twelve overflow cases
preserve the failure point. Forty-six paired runtime labels have no gas
increase. No original test is removed or ignored.

The final compiler passes Clippy and the full benchmark workflow from main:
24 identical case IDs, fifteen runtime cases, 175 gas labels and 139
observations per compiler. Gas and Size runtime corpus outputs and gas are
exact. UI Gas totals shrink 26 creation/runtime bytes; Size shrinks 22 each,
with 22 objects smaller per mode and none larger. The nine-project heavy
capture retains 1,672 contracts, 3,344 objects and all 541 reference sites;
six objects shrink two bytes each, four change at equal size, and the rest
are exact. Physical artifacts are compared, including explicit generated
helper-name mappings where needed. Solc records are fingerprint-checked
reuse, not fresh solc timing measurements.

Compiler-time geometric mean is nearly flat (-0.11%), but Seaport remains
0.83% and 1.00% slower in opposite run orders. Fractional is also slightly
slower in both. Those are observed tradeoffs, not an unqualified compile
speed win; their cause is not established. RSS changes do not consistently
repeat. Seaport has one sample per leg because of the ten-second cutoff.
The reversed four-case run measures time only and reuses no solc records.
All samples, artifacts and independent reviews remain under
`legacy-call-boolean-workflow-20260907/`,
`terminal-exchange-reversed-timing-20260907/` and adjacent review directories.

A separate trial classified four legacy CALL results as Boolean in MIR.
Although the facts are sound, a new four-word GAS-observer case increased
total execution gas by six in Gas and fifteen in Size after an initial
return-address issue was fixed. The MIR trial was rejected and restored
from the retained current file; no Boolean facts or expectation changes
from it were committed. Drafts, binaries, runtime traces and negative
controls are retained. The terminal exchange fix above stands independently.

Commit `1fa2328b` refreshes only the original storage-bytes test's comments
and full golden. It explicitly accepts compact inline getter returns in
place of the historical shared len/at return. Solidity tokens and flags are
unchanged; full before/after ABI, bytecode and EVM IR are exact. All 528
stateful calls agree, 224 neighboring-slot checks preserve storage, and 21
negative controls reject. All 74 successful getter labels beat the sealed
gas baseline. Six existing Size malformed-header gas debts remain explicit.
A bounded symbolic getter probe agrees after fixed state preparation; it
does not establish arbitrary-state equivalence. Installation evidence is in
`storage-bytes-installed-20260907/` and its independent review directory.

The latest full workspace run has 1,395 passing tests and one failing UI
aggregate: 11,569 UI cases pass, five original cases fail, and 851 are
filtered. The failures are cold-call fallthrough (Size), global calldata
alias, low-level calldata calls, unloaded spill stores and tuple assignment.
Current backend Rust files total 15,228 raw lines, 19,410 fewer than the
recorded deleted 34,638 (56.0%); this includes comments and inline tests.
The sealed performance debts and computed-memory interference remain open.
A fresh fetch confirms main `933bc1e2`; merging reports already up to date.
Future comparisons use main's full benchmark and artifact workflow. Local
commits remain unpublished after automatic approval review rejected push.


### Computed immutable spill recipes

Commits `d88041fa` and `649033b0` repair a source-memory readback corrupted
by restoring a compiler spill over the source's `mstore`. At the stack
scheduling boundary, a bounded whole bank of immutable calldata expressions
becomes cached recipes. ADD, SUB, AND, OR and XOR over a fixed calldata read
and literal retain operand order; canonical constant ADDs can complete the
bank. Only closed same-block producers are suppressed, with original
liveness cleanup retained. Reservations, protocol choices and assembler
behavior stay fixed. Internal-call artifacts, dynamic frames, returning
owners, oversized functions and noncanonical banks decline this extension.
This is a targeted correctness repair, not a general solution to compiler
storage interfering with source memory.

The original eighteen-value witness now returns the source's `0xdeadbeef`
instead of the restored spill value 257. Runtime size drops 408 to 255 bytes
in both optimized modes; executed opcode gas drops 790 to 526 at address
192 and 1,161 to 924 at 4,096. These are trace opcode costs, not transaction
receipt gas. Three new standard-matrix fixtures pass twelve UI revisions
and 48 focused runtime calls, repairing eighteen baseline failures across
None/Gas/Size. They cover unaligned writes, all five recipe operations,
shared producers and actual mixed-bank/internal-call controls. Thirteen
unique negative FileCheck controls reject. Original tests remain intact.

The required `solsymdiff` tool confirms the old mismatch through a fixed
concrete prefix on a mutability-only derivative of the constrained array
witness. The reported symbolic suffix is not itself the failing call.
Candidate exploration and the earlier array/scalar attempts timed out;
none establishes symbolic equivalence. Generated runtime bodies bridge to
concrete before/candidate/solc replays. Complete attempts and the independent
scope audit remain in `computed-rematerialization-symbolic-20260907/`.

Commit `74381ecf` separately extracts recipe emission and moves suppression
cleanup to opcode lowering without changing selected or emitted code. The
debug `load_value` helper shrinks 2,863 to 1,826 native bytes and its frame
1,888 to 976 bytes. Against the unextracted fix, the full 24-case compiler
time geometric mean improves 0.95%, with final-process peak RSS 0.50%
higher. Against the pre-fix baseline, full time is 0.31% lower overall,
but Solarray remains 2.24% slower and 2.85% slower in a reversed repeat.
This repeated per-project cost remains open. Official unknown-profile
exclusions are preserved; raw full-run timing is explicitly supplemental
with dev-build provenance. The reversed run uses verified debug aliases.

The new workflow retains all 24 case IDs and, in each hot mode, fifteen
runtime cases, 175 gas labels and 139 observations per compiler. Gas,
creation and runtime bytes are exact; only two MIR helper-name changes
need explicit bijections. Solc records and artifacts are fingerprint-checked
reuse. All 4,908 UI objects and 3,344 heavy objects bridge exactly through
the extraction, including the three added sources as a separate inventory
extension. Sealed UI/heavy debts remain unchanged. Raw runs and reviews
are under `computed-rematerialization-workflow-20260907/` and adjacent
`computed-rematerialization-*` review directories.

Clippy passes. The full workspace has 1,395 passing tests and one failing
UI aggregate: 11,581 cases pass, the same five original cases fail, and
851 are filtered. That aggregate includes the Standard JSON and upstream
Solc modes; no separate rerun is claimed. Current backend Rust files total
15,465 raw lines versus 34,638 deleted: 19,173 fewer (55.4%), including
comments and inline tests. Full functionality, sealed performance parity
and CI remain unfinished. Main `933bc1e2` is merged; publication remains
blocked by the earlier automatic push rejection.


### Empty suppression check and explicit Foundry run

Commit `49d5e21b` replaces two debug-build helper calls per opcode with a
direct borrowed optional-set check. Selection and emitted instructions are
unchanged. The full new workflow measures 0.85% lower compiler time than
`74381ecf`, with 0.07% higher final-process peak RSS. Solarray improves 2.24%
and matches the pre-fix median. Two reversed microchecks retain overlapping
positive gaps of 0.24 ms and 0.30 ms; no universal speedup is claimed.
Both hot modes retain exact fifteen-case, 175-label, 139-observation joins,
and all 4,908 UI/3,344 heavy objects remain exact. Raw evidence and all
repeats are under `computed-rematerialization-workflow-20260907/guard-trial/`.

The explicit Foundry run executes all 36 projects: 772 compiler tests and
765 solc tests pass, with identical ordered IDs in all 35 paired projects.
The existing stack-depth project supplies seven compiler-only tests.
Reports pin the source-equivalent workspace rebuild separately from the
frozen benchmark binary; no launch-time binary hash was captured. Foundry
uses each project's solc selection, including two 0.8.12 projects, rather
than the runtime benchmark's universal 0.8.36 pin. The full workspace still
has the same five UI failures. Clippy and typos pass; `ef17e019` fixes the
only nightly-format failure by rewrapping two documentation lines.

A broad terminal-dedup/tail-merge pipeline probe is rejected despite smaller
objects: seventeen original calldata-alias calls increase opcode gas by
1–24. All 360 focused calls and actual transaction receipts are retained;
calldata floors conceal some opcode differences. The unloaded-spill case
has a promising branch-local alternative, with explicit layout and stack
proofs required before implementation can be accepted. Original fixture
bodies and expectations remain unchanged during this investigation.


### Direct literal arms and one restored UI case

`c66f69c5` selects exclusive literal arms arithmetically in the default Gas
pipeline. Matching, costing and emission live in the focused EVM IR `diamond`
module; the assembler and MIR layers are unchanged. Fresh return blocks use
the existing unique-label allocator. Size is excluded after the first trial
increased one corpus object and two executed paths. The rejected trial and
exact duplicate-label round-trip failure remain in the evidence.

`e05112be` adds five fixtures and six reviewed goldens: nine revisions pass,
including 52 runtime calls. `0dda007c` separately migrates the original spill
checks from frame-slot reloads to one shared stack-based addition, overflow
check and return. Its program and flags are unchanged; fresh same-path Gas
and Size full outputs are byte-identical before and after the comment edit.
Its Gas runtime is 102 bytes versus 119 before and 109 sealed, saving 4–5
opcode gas on success/addition-overflow paths. All fourteen focused labels
remain below sealed gas. Size remains 103 bytes.

The new workflow retains exact 24-case Gas and 15-case Size joins, each with
175 ordered gas labels and 139 observations per compiler. Runtime gas and
physical artifacts are unchanged in both hot corpora. Two MIR captures have
proved bijective helper-name changes. All 3,344 heavy objects and metadata
remain exact. The UI corpus has 808 sources and 4,912 objects: original Gas
Branch/spill objects shrink by 9/17 bytes, the new runtime source shrinks by
17 bytes per artifact, and Size is exact. Source-only supplements preserve
the original denominator through fixture/comment edits; no new or worsened
sealed debt is hidden. The Size audit rejects an intermediate runs=200 solc
artifact copy and traces the correct 105 reused files to sealed runs=1 data.

Full compiler time rises 1.42% (21/24 cases), with peak RSS down 0.20%.
Emission extraction and cheap rejection reduced the initial Solarray cost;
its final full-run delta is +0.27%. Fresh outlier repeats give Counter −2.25%,
Aave +0.96% and Solmate +1.28%; they do not replace the full-run tradeoff.
Clippy, nightly formatting and typos pass. All 36 Foundry projects pass,
with 772 compiler and 765 solc tests and unchanged recorded gas/size values.
Solc versions are project-specific, including two 0.8.12 pins. Symbolic
truthiness has bounded agreement; checked multiplication remains incomplete
under the hard-arithmetic heuristic, with concrete runtime checks passing.

The final workspace has 1,395 passing tests and one failing UI aggregate:
11,591 UI cases pass, four original cases fail, and 851 are filtered. Cold
call fallthrough (Size), global calldata aliasing, low-level calldata calls
and tuple assignment remain open. Backend Rust totals 15,725 raw lines in
44 files, 18,913 fewer (54.6%) than the deleted scope; counts include comments
and inline tests. Main `933bc1e2` was freshly fetched and is already merged.
Publication remains blocked by the earlier automatic push rejection.

Raw runs, hashes, failures, strict joins and timing reports are retained in
`target/codegen-bench/evm-rewrite-candidate/direct-literal-arms-workflow-20260907/`,
with adjacent test, spill-migration and Foundry/symbolic reviews. A redundant
new-fixture dump flag exposed a matrix integration failure; all failed runs
are retained and all runtime directives survived the correction.

An independent follow-up census attributes 8,764 Router runtime bytes to
137 spill-preservation fragments. Selective SSA residence needs real mixed
stack/home Phi edges; relaxing mandatory Phi homes alone is invalid. Separate
replays confirm mutable-memory readback errors in both the current and sealed
compilers, so this is pre-existing semantic debt, not a sealed-correct
regression. All 48 concrete calls and replay-confirmed fixed-prefix
`solsymdiff` mismatches are retained under
`arbitrary-memory-spill-correctness-20260907/`. Neither fewer spill homes nor
small immutable recipes establish a general repair. Full functionality,
sealed performance parity and passing CI remain unfinished.


### Selective Phi trial: correctness screen, rejected cost

The first selective-residency draft (`2503101c…`) uses one eight-word,
interval-ranked proposal after ordinary allocation and rematerialization.
It preserves reservations and entry layouts, excludes dynamic frames and
incoming internal-call closures, and emits simultaneous mixed stack/home
Phi transfers. Failed real lowering restores owned blocks, debug metadata,
appended block IDs, both switch budgets and the original allocation.
This is an uncommitted trial, not an accepted performance milestone.

All 114 focused candidate calls pass; 29 improve opcode gas and 85 are
unchanged. Adjacent-edge traces prove old-home snapshots survive writes and
dying resident-to-home transfers. Same-edge mixed-header coverage and an
actual failed-emission rollback remain open. Thirty-six independent control
calls pass with exact paired output/gas, including an internal-call target
whose tail descendant has nineteen Phis. Symbolic exploration is incomplete;
it is not evidence of unrestricted agreement.

The unchanged UI inventory has 807 compilation inputs per mode and 4,912
objects. Gas creation/runtime each shrink by 741 bytes across three
contracts; Size is byte-identical, with the same eighteen diagnostic rows.
Nine full projects retain 1,672 contract IDs and 3,344 objects. Aggregate
creation/runtime shrink by 100,006/59,394 bytes, but fourteen artifacts grow.
The new workflow's hot runtime screen retains all fifteen IDs, 175 ordered
gas labels and 139 observations unchanged. Nitro creation/runtime grow by
43 bytes and deployment costs 9,307 more gas. These regressions reject the
draft. Concurrent one-sample timing is not an acceptance measurement.

Independent attribution finds compact-writer fragmentation in Nitro and
Seaport. Retiring one home can turn a twelve-home bitmap into eleven ordinary
backups, or a contiguous run into a larger bitmap. A proposed repair checks
the actual initialized/live/overlapping writer bank and declines promotion
when remaining protection costs more in either bytes or static gas. Global
scheduling and outlining still require measured acceptance afterward.

The workspace remains at 1,395 passing tests plus the same failing UI
aggregate: 11,591 UI cases pass and four original cases fail. No tracked
fixtures or expectations changed. Frozen binaries, source diffs, exact
input joins and rejected results are in
`target/codegen-bench/evm-rewrite-candidate/selective-spills-workflow-20260907/`;
adjacent regression reviews retain binary-matched writer-region evidence.


### Selective Phi residence: accepted bounded milestone

`54199124` keeps selected Phi values and their inputs on the stack after
ordinary allocation, with mixed transfers in the private machine lowering
module. One interval-ranked proposal preserves entry layouts and reserved
home addresses. Failed actual emission restores owned blocks, debug data,
bindings and switch budgets. Dynamic frames, returning/hidden-prefix owners
and rematerialization recipes remain outside admission. This is a bounded
scheduler improvement, not a general private-memory repair or optimal search.

The writer guard fixes the rejected Nitro/Seaport bank fragmentation by
comparing the actual live, initialized, overlapping original and proposed
banks. A separate narrow filter keeps single-use arithmetic stores that
already target an unpromoted Phi's home. Draft3 incorrectly excluded canonical
Pure metadata; draft4 corrected that and removed the remaining eight +2-byte
artifacts. Final draft5 removes only a redundant selection flag. All rejected
outputs and failed analyses remain retained.

Frozen final `04887ff5` preserves all original 808 source hashes, 807 UI
compilation IDs per mode, 4,912 objects and eighteen diagnostic rows. Gas
creation/runtime each shrink by 557 bytes; Size is byte-identical. Nine full
projects retain 1,672 contract IDs and 3,344 objects: creation/runtime shrink
by 40,112/39,845 bytes across 154 objects, with no individual growth or
worsened sealed debt. Final contract outputs and metadata exactly match
passing draft4; three projects differ only in diagnostic ordering, with exact
multisets. Origin-policy review finds no new transport defect: body operations
retain MIR origins, and generated mixed edges remain explicitly unknown.

The freshly fetched main `933bc1e2` is already merged. The new benchmark and
comparison workflow records all 24 Gas IDs and fifteen Size IDs. Each runtime
leg retains 175 ordered gas labels and 139 observations, exact solc reference
records, and unchanged physical artifacts, deployment gas and runtime gas.
Only two captured MIR files have proved bijective helper-symbol renames.
Full compiler time is +1.483% and peak RSS +0.064%. Twenty-two cases have five
samples per leg; Seaport and Solady have one because of the ten-second cutoff.
Reversed five-repeat outlier runs give Nitro +0.76%, signature checker -1.36%
and Solmate +0.25%; these do not replace the primary full-run cost. The Size
one-sample run is a correctness/gas supplement, not an acceptance timing claim.

`52a30c23` adds four standard-matrix fixtures and four MIR snapshots, preserving
all 46 runtime directives. Selective checking passes sixteen revisions and
184 calls, including MIR's ordinary None execution. Fresh same-installed-path
baseline/candidate captures preserve ABI and physical bytecode; complete
requested outputs match prepared captures after explicit path substitution.
The separate 138-call three-mode trace ledger has no gas increases. Gas
runtime savings are 121, 70 and 73 bytes in transfers, cycles and mixed writer;
the returning-call control and every None/Size object are unchanged. Actual
mixed-header traces capture an old home before overwriting it on the same edge.
No existing test or expectation changed.

Clippy, nightly formatting and spelling checks pass. All 36 Foundry projects
pass with exact ordered IDs, gas and sizes: 772 compiler and 765 solc tests.
The seven compiler-only stack-deep cases remain unchanged. The first successful
Foundry run omitted report output; a second run retains all 36 reports and
exact final binary hashes before/after execution. Project-specific solc pins
remain in effect; the reports do not record every actual solc executable.
Fresh paired symbolic runs on the installed transfer fixture use depth 2048,
64 paths and 64 queries. Both reach the solver-query limit, so neither supplies
agreement or a counterexample. Identical requested settings/source are retained;
the derived bytecode-holding bound differs 734 to 613.

The final workspace has 1,395 passing tests and the same failing UI aggregate:
11,607 UI revisions pass, four original cases fail, and 851 are filtered.
Cold call fallthrough (Size), global calldata aliasing, low-level calldata
calls and tuple assignment remain CI blockers. General mutable-memory defects
and sealed performance debt remain open. Backend Rust totals 16,274 raw lines
in 45 files, 18,364 fewer (53.0%) than the recorded deletion scope, including
comments and inline tests. This milestone adds 549 production-file lines.
Publication remains blocked by the earlier automatic push rejection.

Evidence is retained under `selective-spills-workflow-20260907/`, with adjacent
independent corpus, metadata, installed-test and rollback reviews. The focused
symbolic projects and traces remain under `target/selective-spills-tests-20260907/`.
A read-only follow-up design identifies one existing late MemoryDse invocation
as a possible tuple transport fix; no pipeline change has yet been made, and
shared-return obligations remain separate.


### Late memory cleanup: rejected isolated experiment

A two-line trial added the existing MemoryDse after LowerEvmShaped and before
final Dce. Frozen `5e6b5a71` improves tuple creation/runtime by 13 bytes and
multi execution from 125 to 89 gas. Fresh before, candidate and solc lanes each
pass all 103 focused calls; sealed traces are reused only after exact artifact
and program/directive checks. The other 102 gas labels and all 36 external-call
contexts are unchanged. Three swap labels remain seven gas below sealed;
twelve viaNamed labels still cost sixteen more than sealed.

The complete size screen rejects this broad invocation. Identical 811 UI IDs
per mode and 1,232 contracts retain all statuses, but 445 contract/mode entries
grow. Gas creation/runtime increase 34,408/30,990 bytes; Size increases
22,140/20,080. Nine full projects retain all IDs and have 516 individual object
increases. The workspace shows 25 unchanged-expectation failures (21 additional
to the four original failures); none were blessed. No full hot-gas or controlled
timing acceptance run is warranted for this rejected screen.

Independent review also reproduces an existing constant-store-map bug:
`mstore 128,1; log0 128,32; mstore 129,0; mstore 128,1` can lose its required
last store. A semantic frame-store control preserves that store before frame
lowering, then loses it with the new late invocation. Both frozen compilers
reproduce the explicit-pass defect. The two trial lines were removed and the
accepted pipeline's exact hash restored. The correctness repair is a separate
next change; no rejected pipeline or expectation update remains in production.
Evidence is retained in `late-memory-dse-workflow-20260907/` and
`target/late-memory-dse-tests-20260907/`.


### Overlapping constant words: correctness repair

The constant-store cache now invalidates every overlapping 32-byte word,
including writes with unknown values at known addresses. The old exact-key
update could delete a required repair after an unaligned store; concrete
execution and replay-confirmed differentials reproduced the wrong returned
word or hash. No extra late MemoryDse invocation was retained.

A bounded local proof preserves repeated mapping-seed elimination: the next
instruction must overwrite the dirty intersection, and a still-live equal
seed within eight preceding instruction positions must establish the residual
bytes. Existing alias analysis must prove intervening writes disjoint from
that residual. Its region is explicitly Unknown, including intervals crossing
the scratch/heap boundary. The proof adds no persistent byte-range state,
never treats a deleted seed as live, and leaves whole-word cache facts
unchanged when deleting a partially redundant store. Production-file delta
is +71 lines in one MIR pass; the backend and pipeline are unchanged.

Committed as `0e5ab53b`, frozen `1913b139` retains all 4,928 prior UI bytecode
objects exactly. Across nine archived projects and 1,672 complete contract outputs, the voting
contracts recover all bytes and metadata lost by the overlap-only trial.
Only ERC721Test grows: 15 creation and 15 runtime bytes restore the required
seed store. The reduced returned-hash contract demonstrates the old wrong
result and now agrees with solc, at 15 additional opcode gas in Gas and Size.
All surviving source-map entries remain exact; five restored instructions
point to the seed store and references relocate correctly. The earlier
18-static-gas attribution was an arithmetic error, preserved and corrected
in the evidence. Strict size-debt checks still flag this required restoration;
this is not a claim that the rewrite's performance gates are complete.

The new official workflow retains 24 full-run IDs and 15 Size-run IDs, with
175 ordered gas labels and 139 observations per compiler in each runtime
lane. Gas, deployment gas, runtime bytecode and all physical artifacts match
the prior candidate; solc reuse is exact. Two MIR helper-name changes are
proved bijective renames. Full compiler-time geomean is -0.2349%, RSS -0.1401%;
22 cases have five samples, Seaport and Solady one due the ten-second cutoff.
The one-sample Size supplement makes no compiler-time claim.
A candidate-first repeat reduces Solarray/OpenZeppelin slowdowns from
2.606%/1.158% to 0.616%/0.226%, with overlapping five-sample ranges. Its
two-case geomean is +0.421%; the primary full result remains unchanged.
This compiler-only repeat has no gas measurements or reused solc records.

Six new fixtures and ten reviewed snapshots retain the word-boundary,
semantic/physical phase, partial-overwrite, observer, alias-region, window,
dead-seed and overflow obligations. All 16 new UI revisions pass, executing
28 run-call assertions per run. An initial two-output directive parser error
was corrected without changing any expected bytes; failed captures remain.
The artifact-only harness also passes 16 MIR revisions and 21 concrete calls;
three symbolic projects report bounded agreement with unchanged inputs,
settings and bounds. No general equivalence proof is claimed.

The final workspace has 1,395 passing tests, one failing UI aggregate and
two skipped tests. UI revisions: 11,623 pass, the same four originals fail,
and 851 are filtered. All 36 Foundry projects pass; their ordered 772/765
compiler/solc tests, gas and reported bytecode sizes are unchanged.
Clippy, nightly formatting and typos pass. No existing test or expectation was changed.
Broader arbitrary-memory defects and the
rewrite's sealed performance debts remain unresolved.

Evidence is retained under `memory-dse-word-overlap-workflow-20260907/`,
`memory-dse-residual-candidate-independent-20260907/`, and
`target/memory-dse-partial-overwrite-tests-20260907/`.


### Main refresh and merged dependency verification

A final HTTPS fetch advanced main from `933bc1e2` to `6059f0c0`. Merge
`53075d0e` includes the dependency, CI-action, npm and LSP-test updates without
conflicts. The merged compiler is frozen as `eb7d8a74`; all 145 codegen source
hashes are unchanged, and the new lockfile is pinned separately. The prior
binaries and benchmark evidence remain intact.

The new official full and Size workflows both pass. All 24 full-run output
fingerprints match `1913b139`, with exact physical runtime artifacts, 175 gas
labels and 139 observations per compiler. Solc reuse is exact; two MIR helper
name changes are complete bijections. Both UI corpus legs use the same final
814 source hashes, 813 IDs per mode and 1,234 contracts per mode. All 4,936
objects are byte-identical. One warning stream differs only in the order of
ten complete diagnostic blocks, with identical contents and multiplicity.

Merged compiler-time geomean is +0.7063%, RSS -0.5015%. Seaport and Solady have
one sample per leg due the ten-second cutoff; the other 22 have five.
Nitro's +1.5621% becomes +1.3048% in a five-sample candidate-first repeat,
whose ranges overlap; this remains a measured slowdown. Forge-std's +0.9647%
has disjoint sample ranges and is not repeated. No speed or noise claim is
made. The one-sample Size run remains a runtime supplement, and the Nitro
repeat measures no gas. These merge costs do not replace the repair's prior
measurement or establish final rewrite acceptance.

The merged workspace retains 1,395 passing tests and the same failing UI
aggregate: 11,623 revisions pass, four original failures remain, 851 are
filtered. Clippy, nightly formatting and typos pass. No existing test or
expectation was changed. Evidence is under
`target/codegen-bench/evm-rewrite-candidate/main-6059f0c0-merge-20260907/`.


### Prior-art refresh and rejected argument cache

Solx, Venom and Sonatina were refreshed at the same documented pins. Their
selective spill and rematerialization policies continue to motivate bounded
home selection at the scheduling boundary. The independent review identifies
mandatory Phi inputs/results and existing writer/call floors that a first
failure-directed proposal must retain; it makes no measured performance claim.

The local external-argument cache trial is removed. The original calldata-alias
fixture reached its replay with equal input/net/peak stack usage, but normalized
cost was 27 gas versus 24, with both sequences 11 bytes. None/Gas/Size focused
outputs therefore remained exact. Raw shuffle cleanup cannot overcome that
measured lower bound. No new test was installed and no existing expectation was
changed. Reverse patches and source-hash checks establish exact restoration of
the accepted scheduler. The unaccepted binaries and diagnostics remain under
`argument-residence-workflow-20260907/`; the debug build must be rebuilt before
it is used as the accepted compiler. A narrow MIR carry-comparison experiment
and the broader spill-floor investigation remain work in progress.


### Rejected MIR carry canonicalization

The unsigned identity `lt(add(x, k), x) = lt(add(x, k), k)` was trialed for
one-byte constants in the existing instruction simplifier. Independent review
confirmed its word semantics and termination guard. The motivating calldata
fixture shrank from 215 to 208 runtime bytes, but the complete unchanged UI
corpus rejected it: 1,384 of 4,936 objects changed and 955 grew. Gas creation and
runtime totals increased 227/190 bytes; Size increased 472/446 bytes. All 813
IDs per mode and their 804 successful/nine diagnostic outcomes were retained.

Two inspected regressions explain why local arithmetic cost is insufficient.
The cross-block nullary fixture grows 82 bytes in Size because the new carry
shapes prevent two literal arms from sharing one arithmetic body. The do-while
fixture gains 14 jumps and markers after branch reversal, growing 57 bytes in
Gas and 37 in Size. Exact captures and accounting remain in
`small-carry-workflow-20260907/`. No expectation changed; the sole production
patch was reversed and its original hash verified. The failed size screen
precludes an acceptance claim, so no full hot-gas or compiler-time run was made.
Work continues on bounded retirement of unnecessary homes, using the existing
emission transaction and preserving the accepted Phi allocation path.


### Bounded retirement of single-use homes

Commit `a2014fe5` adds an ordered twenty-value storage/writer regression test;
`5b64cf34` reuses the existing pressure scheduler and emission checkpoint to
retire optional single-use homes. At most eight scans restore an old home from
a failed site's conservative identity pool. The no-Phi, 256-value admission
bound leaves mandatory homes, original residents, entry layout and reserved
memory intact. The existing Phi proposal and assembler remain unchanged.
Production changes add 136 lines across three files; the backend now contains
16,410 raw Rust lines in 45 files, 18,228 fewer than the recorded deleted scope
(52.6%). These are physical lines including comments and inline tests, not a
strict production-SLOC census.

Broader drafts were rejected for real regressions: mutable-bank execution
increased 132 opcode gas despite fewer bytes, and multi-use checked locals grew
86 bytes through rotations. Final `a5dffa7b` reuses last-use operand preparation
and restricts new residents to one static use. Against accepted `eb7d8a74`,
mutable-bank runtime shrinks 403 to 271 bytes and saves 204 opcode gas;
ordered-storage shrinks 369 to 293 and saves 169 gas; the mixed tail entry
shrinks 252 to 184, with fast gas unchanged and blocked gas down 108.
Dynamic-writer runtime shrinks 349 to 215 and normal receipt gas falls 153.
At the retained 86,856-gas boundary its GAS-dependent boolean changes from zero
to one, explained by each leg's twenty recorded GAS words; this is explicitly
preserved, not reported as exact numerical GAS behavior. Size is unchanged.

Final UI captures use the same 815 source hashes, 814 IDs per mode, 805
successful and nine diagnostic outcomes, and 1,235 contracts per mode. Eight
of 4,940 objects shrink, none grow: Gas creation/runtime totals fall 411/410
bytes, and all 2,470 Size objects are exact. Ten warnings retain their complete
contents and multiplicity with only ordering changed. The tail-entry check and
two snapshots were updated after exact-runtime review and eight negative
mutants; argument store576 and the shared revert protocol remain checked.
No runtime directive was removed or weakened.

Official full/Size workflows retain 24/15 IDs and 175 gas labels with 139
observations per compiler. Runtime gas, deployment gas and physical artifacts
are exact to the preceding candidate; two MIR helper-name changes are proved
bijections. Complete project-output fingerprints bridge all 1,672 contracts,
3,344 objects and 541 reference sites to retained raw outputs. All 1,061
objects larger than the original sealed baseline remain unchanged. Mutable
Gas still costs three bytes/36 opcode gas more than sealed, and its Size debt
of 135 bytes/240 opcode gas remains. Mixed-tail Size debt also remains.

Primary compiler-time geomean is +3.4615%, RSS +0.2043%. The four-case reversed
repeat is +0.5129% time, +0.5213% RSS; two cases improve and two slow down.
The primary result and unrepeated sum-array slowdown are retained. No noise
or compiler-speed win is claimed. The one-sample Size lane has no timing claim.

Final workspace results are 1,395 passes, one failing UI aggregate and two
skips: 11,627 UI revisions pass, the four original failures remain and 851 are
filtered. All 36 Foundry projects pass with exact ordered 772/765 compiler/solc
tests, gas and reported sizes against the previous retained reports. Clippy,
nightly formatting and typos pass. The first symbolic attempt is incomplete
on both legs at the same 25-second timeout; the guarded mutable-bank source
still activates all eight retirements. Both longer attempts also
time out at 180 seconds after compilation; neither establishes agreement. The scalar-formal probe also timed out on both legs at 90 seconds after
compilation, with identical calldata layout and the same retirement activation.
All six attempts remain incomplete; no differential agreement is claimed.
Arbitrary-memory correctness, exact retry-boundary coverage and final sealed
performance acceptance remain open.

Evidence: `failure-directed-homes-workflow-20260907/`,
`failure-directed-homes-runtime-20260907/`,
`failure-directed-homes-tests-20260907/`, and
`failure-directed-homes-reversed-timing-20260907/` beneath
`target/codegen-bench/evm-rewrite-candidate/`. The first two production drafts,
failed checks and sealed comparisons are retained alongside the final results.


### Adjacent call Boolean normalization

Commit `c9130ac8` removes adjacent double ISZERO after the four legacy call
opcodes in physical EVM IR. It retains the original call and fixes a scanner
stall when adjacency metadata refuses the rewrite. Five new UI revisions
cover the call families, metadata, raw-word refusals and the stalled case.
The corrected `b4a1695e` binary passes them; the earlier `64bdcfc5` draft and
its failing glue fixture remain preserved. Production adds six physical lines;
the backend has 16,416 lines in 45 files, including comments and inline tests.

Against accepted `a5dffa7b`, the identical UI corpus has 48 shrinking objects
and no growth: Gas creation/runtime totals fall 32/32 bytes and Size 28/28.
All 264 focused calls across both candidates and pinned solc pass. Each of
24 low-level forwarding labels per mode saves six execution gas. A forwarded
GAS observer saves 12 gas while its later offered and target gas rise by six;
the direct observer stays exact. Trace evidence records these numerical changes.
The low-level fixture is still six Gas bytes above the original sealed baseline
(315/297 creation/runtime versus 309/291), and 17 Size bytes below it.

Official full/Size workflows preserve 24/15 IDs, 175 ordered gas labels and
139 observations per compiler. Runtime gas and physical output fingerprints
are exact to the preceding candidate. Nine project fingerprints bridge all
1,672 contracts and 3,344 objects; all 1,061 objects larger than sealed remain
unchanged. The separately recorded Foundry suite passes 36 projects with
identical ordered 772/765 compiler/solc tests, gas and reported sizes.

Primary compiler-time geomean is +5.8486%, RSS +0.1649%. Candidate-first
outlier repeats give +0.0836% time and +0.4187% RSS across four cases in two
runs. Nitro and Forge have five samples per leg; Seaport and Solady have one
under the ten-second cutoff. The large primary Nitro and Solady slowdowns did
not recur. Both results remain recorded; neither their cause nor a compiler
speed improvement is established. The blocked earlier draft's +0.4006% is a
separate result. The one-sample Size lane has no controlled timing claim.

Workspace results are 1,395 passes, one failing UI aggregate and two skips:
11,632 UI revisions pass, four original failures remain, and 851 are filtered.
Clippy, formatting and targeted typos pass. No existing test or expectation
changed. Original arbitrary-memory correctness and sealed performance gates
remain open. A subsequent empty-revert CFG experiment will address part of
the low-level size debt; its shared-return assertion remains an explicit
optimization obligation pending measured review.

Evidence is retained under `adjacent-call-boolean-workflow-20260907/`,
`adjacent-call-boolean-tests-20260907/`, `adjacent-call-boolean-replay-20260907/`
and `adjacent-call-boolean-independent-20260907/` beneath the candidate
evidence directory. A separate artifact-only pure-consumer scheduling draft
was withheld after review found a two-run instruction-order instability;
see `writer-observer-scheduling-design-20260907/ADVERSARIAL_REVIEW.md`.


### Existing empty-revert owners

Commit `c511964c` extends physical CFG terminal redirection to canonical empty
reverts around GAS observations, subject to an already-taken, unpushed owner,
a nonempty retained entry and no indexed control. Transfer and destination
costs stay unchanged; every retained label stays nonzero. No assembler or MIR
logic changed. Production adds 28 physical lines; the backend now has 16,444
lines in 45 files versus 34,638 in the deletion census, a reduction of 18,194
(52.53%). These are physical lines including comments and inline tests, not a
strict production-SLOC comparison.

The frozen `775676aa` candidate versus accepted `b4a1695e` has 316 shrinking
UI objects and no growth among 4,940 matched objects. Gas creation/runtime
totals fall 618/618 bytes and Size 897/895. The corpus retains 814 IDs per mode,
805 successes and nine diagnostic outcomes, and 1,235 contracts per mode.
One diagnostic differs only in ordering of its complete warning blocks.
After the call-test matrix migration, matched captures and source-body checks
bridge every affected object to the full capture; compiler inputs match
between the baseline and candidate legs.

All 24 full-workflow and 15 Size-workflow IDs retain 175 ordered gas labels
and 139 observations per compiler. Runtime gas is unchanged. The Size Aave
L2 encoder's deployment gas falls 776,314 to 766,790 (9,524 saved); its
creation and runtime each shrink 44 bytes;
other runtime objects are exact. Four changed project outputs were recaptured,
and five unchanged fingerprints bridge retained outputs. Across all 1,672
project contracts and 3,344 objects, 66 objects shrink by 3,075 creation and
3,075 runtime bytes, with no growth. Immutable/link identities, widths and
contents remain valid. Seventeen source-map changes were reviewed against
retained instructions, operand relocation and metadata merge rules.

Original sealed debt remains substantial: 1,059 project objects are still
larger, down from 1,061. Their positive size deltas total 33,272,046 bytes;
that is a sum of regressions, not a net corpus delta. Existing mutable-bank,
calldata-alias and mixed-tail debt also remains. The low-level forwarding
fixture now has 295/277 Gas creation/runtime bytes versus sealed 309/291,
and 280/262 Size bytes versus sealed 309/291. All 24 valid forwarding labels
per mode retain preceding-candidate execution gas, saving 43/54 gas for
Call/Delegate in Gas and six in Size against sealed. Raw malformed-caller
traces preserve empty reverts and execute no external call, but retain sealed
debts of 15 gas for selector rejection, 18 for call value and two for the
Delegate head/address checks. These are not hidden by the hot-path savings.

Eight new EVM IR revisions cover positive cases and conservative refusals.
Nine existing snapshots changed only after their original FileChecks passed
and all retained instructions were audited. Commit `09d6f173` explicitly
replaces the old forwarding fixture's shared-return layout policy with complete
per-wrapper copy/call/Boolean-return and shared-decoder checks. The executable
source is unchanged; 15 check mutants fail. Four precompile controls and 29
exact rejection directives run across five revisions, replacing the default
revision with a named IR revision plus the standard matrix. The 165 directive
executions are established by all five passing revisions; the UI log does
not contain 165 separate receipts. This is a documented test-policy/revision
migration, not a claim that original revision IDs remained identical.

Final pinned UI results are 11,645 passes, three original failures and 851
filtered revisions. The remaining failures are `cold_call_fallthrough` Size,
`global_stack_calldata_alias` and `tuple_assignment`. The workspace unit run
has 1,395 passes, one failing UI aggregate and two skips. All 36 Foundry
projects pass with 772/765 compiler/solc tests and unchanged exclusions. Two
TupleTernary tests save 875 gas each; two Unifap router size reports shrink
four bytes each. All other reported gas and sizes are exact. Clippy, formatting
and typos pass. No full-suite or final rewrite acceptance is claimed.

A final binary guard found Cargo had selected a different executable despite
identical current source and lockfile hashes. Both executables and their stale
embedded Git metadata were preserved; the cache-selection cause is unproved.
The frozen measured binary was installed atomically, then the existing UI and
Foundry runners were executed directly without Cargo. Its hash matched before
and after, with the final results above. Prior logs remain preserved.

Primary compiler-time geomean improves 7.8905%, RSS 0.1129%, against a baseline
with recorded outliers; this is not a broad compiler-speed claim. Fractional
slows 9.3186% in that run. A candidate-first, five-sample repeat of Fractional
and PRB improves 2.8421% and 2.4916% respectively, with combined time down
2.6670% and RSS up 0.4822%. Both primary and repeat evidence are retained.
The Size lane has one sample and no controlled timing claim.

The matched symbolic probe terminates with exit 2 on both legs because GAS is
not modeled; it establishes no symbolic agreement. Concrete cold-call traces
cover all removed exit families. Previous symbolic timeouts, arbitrary-memory
correctness, retry-boundary coverage and sealed performance gates remain open.

Evidence is under `empty-revert-redirection-workflow-20260907/`,
`empty-revert-runtime-20260907/`, `empty-revert-lowlevel-rejections-20260907/`,
`empty-revert-redirect-tests-20260907/`,
`empty-revert-original-expectations-20260907/` and
`empty-revert-lowlevel-migration-20260907/` beneath the candidate directory.
An unapplied FMP-placement draft and a separately labeled bytecode-relocation
witness are in `fmp-common-frontier-study-20260907/`. The draft is held because
190 added lines of narrowly constrained interprocedural analysis do not yet
justify its demonstrated local benefit. The independent physical witness has
48 unchanged hot pairs, 26 early-rejection pairs saving 18 gas and 32 unchanged
decoder-failure pairs; it does not validate the Rust query or later optimization.
Neither artifact is an accepted compiler optimization. The refreshed
solx/Venom/Sonatina memory audit is recorded in the scheduling research document.


### Literal frame forwarding

Commit `2a55250a` extends the existing MIR MemoryDse pass to forward known
literal words through semantic frame slots in acyclic functions. Frame facts
retain physical base/offset but use the unknown alias region, so raw accesses
cannot evade invalidation through a frame-region tag. Each word write kills
old facts before admitting a literal; existing raw-load forwarding remains
unchanged. Admission is computed once per pass fixpoint, and cycle analysis
is requested only for functions containing word-frame operations. No extra
pass, backend representation or assembler logic was added. This adds 81
physical MIR lines; the backend census remains 16,444 lines in 45 files.

The broader drafts were rejected and retained. Precise frame regions hid raw
aliases; unrestricted forwarding grew two Size objects by 20 bytes each;
literal forwarding in loops regressed 31 of 66 focused calls by 3–87 gas.
The final acyclic restriction restores that loop's complete creation/runtime
objects to the preceding candidate. The sealed compiler is wrong on 34 of
those loop inputs, so its gas on those inputs is not a correctness-equivalent
baseline. A failed edit attempt is separately recorded as an unchanged capture,
not a new candidate.

Frozen `04007f97` versus `775676aa` preserves all 815 UI source hashes,
1,628 ordered compilation IDs and 4,940 objects. Fifty-eight objects shrink,
none grow or change bytes at equal size. Gas creation/runtime totals fall
315/107 bytes and Size 299/91. The official full 24-ID and Size 15-ID reports
retain all 175 ordered gas labels and 139 observations per compiler, with
identical runtime/deployment gas and physical runtime artifacts. Across nine
archived projects, 1,672 contracts and 3,344 objects, 19 objects shrink by
132 creation and 126 runtime bytes, with no growth. Complete output hashes
bridge retained raw captures; eight source-map changes and immutable/link
relocations passed independent review.

The original tuple fixture shrinks from 247/230 creation/runtime bytes to
239/222 in both modes, versus sealed 250/233. Its `multi` call falls from
125 to 98 gas; the other 15 labels retain preceding-candidate gas. All outputs
match sealed and solc. The named-call paths still cost ten more gas than sealed
in Gas and seven in Size. Their original shared-return CHECK has not been
changed in this milestone. A separate matched scalar `solsymdiff` probe reports
bounded agreement for both compiler legs; its runtime shrinks 83 to 70 bytes.
This is not a full-memory or all-input equivalence proof.

The installed 20-case MIR fixture has raw and optimized revisions; 25 negative
FileCheck mutations fail. Final pinned UI has 11,647 passes, the same three
original failures and 851 filtered revisions. Workspace tests have 1,395
passes, one failing UI aggregate and two skips. All 36 Foundry projects pass;
772/765 compiler/solc tests and 123/121 size reports match the preceding
candidate exactly. Clippy, formatting, typos and diff checks pass. No original
fixture or expectation changed in this milestone.

Primary compiler-time geomean increases 2.1467%, with RSS down 0.2451%.
A reversed-order five-sample repeat of four cases gives time down 10.0940%
and RSS down 0.0131%, but retained Nitro, LibString and Forge outliers make
that repeat unstable. It does not erase the primary slowdown or establish
a compiler-speed improvement. The Size lane has one sample and no controlled
timing claim.

Original acceptance remains open: 1,059 project objects have positive size
deltas totaling 33,271,788 bytes, the three original UI failures remain, and
arbitrary-memory ownership and other recorded coverage gaps are unresolved.
Evidence, rejected drafts, exact commands and independent reviews are in
`semantic-frame-forwarding-workflow-20260907/` and
`frame-word-forwarding-proposal-20260907/` beneath the candidate directory.
The preceding empty-revert entry also corrects Aave Size deployment gas using
its unchanged raw report: 776,314 to 766,790, rather than unchanged deployment.


### Rejected equal-identity SWAP trial

A one-condition scheduler trial omitted swaps between equal private identities.
It preserved every modeled stack state and retained reach checks; ten scheduler
unit tests passed. The independent model covered 377,980 reconciliation
settings plus preparation and call controls. No global GAS-equivalence claim
was made: deleting a swap changes subsequent gas observations.

The compiled `5ccb95d4` trial nevertheless grew 29 contract/mode outputs by one
creation and one runtime byte each. Across identical 1,628 UI IDs and 4,940
objects, 168 objects shrank, 58 grew and 206 changed at equal size; aggregate
creation/runtime savings of 79/71 bytes do not override that growth. In the
tuple fixture, later normalization replaced three legacy swaps with four.
Its local analysis starts with distinct incoming identities, losing the equal
zero relationship available to the scheduler. This is a downstream interaction,
not an incorrect exchange cost table or a wrong private permutation.

The trial and its experimental helper test were removed using the pretrial
current-source backup. All 145 codegen source hashes match the accepted
`04007f97` baseline, whose executable is repinned. Original tests are unchanged.
No heavy or hot-gas acceptance was attempted after the failed UI size gate.
Evidence and the independent explanation remain under
`top-first-permutation-study-20260907/` beneath the candidate directory.


### Tuple return test policy and runtime matrix

Commit `25d0fcae` migrates the original tuple fixture from its shared
second-word-store/return policy to complete compact direct returns. Its
executable tokens and all four function bodies are unchanged. This deliberately
retires the `PAIR_RETURN` sharing obligation; it is not printer normalization.
The measured justification is 239/222 creation/runtime bytes versus sealed
250/233 in both modes, with swaps at 156 versus 163 gas and `multi` at 98
versus 139. Named calls still cost ten more gas than sealed Gas and seven
more than sealed Size; this test change does not resolve that debt.

The former full stdout becomes the named IR revision's full golden, and a full
MIR golden is added. The standard matrix plus IR revision retains strict
physical output rather than normalizing it away. Selector/decoder edges,
swapped full-word order, literals, calldata-copy/CALL data flow and exact return
base/length are checked. Independent review reran the positive check and 31
distinct negative mutations. Fourteen runtime directives cover tuples,
precompile success/failure and malformed/rejected calls across five revisions.
The artifact runner established 28 fresh calls plus 42 complete-object bridges;
all five revisions then passed the official UI runner. These are separate
receipts, not 140 distinct fresh EVM calls.

Final pinned UI has 11,652 passes, two original failures and 851 filtered
revisions. The remaining failures are `cold_call_fallthrough` Size and
`global_stack_calldata_alias`. The former has a real successful-fallthrough
layout defect; the latter still lacks the asserted argument reuse and retains
size debt. Neither assertion was weakened. The compiler remained frozen at
`04007f97` throughout the UI run. No production change or new whole-rewrite
acceptance is claimed. Evidence and installation hashes are in
`tuple-final6-migration-20260907/` and
`tuple-final6-migration-independent-20260907/` beneath the candidate directory.


### Cold-owner proposal held; large-object priority

The uncompiled directed cold-owner proposal is held. Its 121 added lines have
an accidental dependency on cold hints that the default pipeline produces later.
A broader occupancy audit of retained full/Size IR found 35 distinct matching
pairs per mode, all excluded by the required observer/control guards. Only the
focused cold-call witness survives. That ten-byte witness does not justify the
new helper or resolve successful fallthrough and sealed gas debt. No production
patch or original expectation changed; the proposal, model and limitations are
retained in `cold-directed-owner-proposal-20260907/`.

The latest SeaportRouter census instead verifies runtime 23,977 versus sealed
9,822 bytes on the identical 386-source input. Its 137 bitmap writer-protection
fragments occupy 8,764 bytes, or 8,627 beyond their source stores: 60.95% of the
14,155-byte debt. All terminal revert blocks together occupy only 357 bytes.
Ninety-one adjacent spill-store/reload sequences account for 728 encoded bytes;
that is an investigation target, not removable-byte or gas savings. Current SSA
eligibility, backup shuffles and later normalization still require proof. The
exact inputs, objects, disassembly and census are retained in
`seaport-router-final040-review-20260907/` beneath the candidate directory.


### Writer-address experiment under measurement

An exact current Router MIR/EVM capture ruled out alignment specialization:
none of 137 protected writers is proved aligned (127 mapped owners have unknown
alignment, ten remain unmapped), and none of the thirteen reserved-allocation
writes owns a protection template. The experiment instead
keeps an immediately produced address on the stack while preserving its home
and the complete writer protocol. It follows the prior-art distinction between
having a spill home and needing to reload it.

Frozen draft `5a32ced0` activates 91 Router boundaries in 50 blocks, reducing
creation/runtime by 182 bytes each. MIR is exact; all retained opcode origins,
new-copy origins and relocated immutable references are independently checked.
The identical-input UI screen has ten smaller Gas objects, creation/runtime
totals each down twelve bytes, no growth and all Size objects exact. Runtime,
heavy size and quiet compiler-time gates remain pending. No original expectation
has changed. Evidence remains under `writer-operand-cache-workflow-20260907/`;
this candidate is uncommitted and the broader rewrite remains incomplete.
