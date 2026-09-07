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
