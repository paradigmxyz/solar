# EVM rewrite progress

The rewrite is incomplete. Remaining UI assertions and individual generated-code
gas/size regressions block acceptance. The draft is
[PR #1388](https://github.com/paradigmxyz/solar/pull/1388). The
[handoff](evm-rewrite-plan.md) defines the contract;
[checkpoint history](evm-rewrite-checkpoints.md) and small commits retain earlier
milestones. Detailed evidence below lives under
`target/codegen-bench/evm-rewrite-candidate/`.

## Scope and architecture

Deletion `e5ba34f2` matched all 40 agreed files; nothing outside scope required
restoration. No deleted implementation was read or recovered. The fresh
unsupported API milestone compiled before functionality returned.

The backend separates private stack scheduling and frame/spill planning,
MIR instruction selection, physical block IR transforms, deployment construction
and primitive assembly. The assembler writes fixed bytes into one buffer with
sparse relocations and computes the least fixed point of label positions and
PUSH widths. There is no Atom stream or assembly-level CFG optimization. MIR
semantics remain in their retained layers. No legacy backend or temporary
unsupported rewrite fallback is used.

The accepted scope has 12,792 Rust lines across 36 files, versus 34,638 deleted
raw lines: 21,846 fewer (63.1%). Counts include comments, blanks and local tests.
A historical production-only count was not retained and is not reconstructed
from forbidden source.

## Current verification

The current workspace run has 1,358 passes, one failing UI aggregate and two
skips. Expanded UI has 11,015 passes, 44 remaining failures and 806 filtered
revisions. All 99 codegen helpers, Foundry and ordinary workspace/all-target
Clippy pass. Thirty targeted revisions pass after reviewing ten changed
snapshots and one comment-only FileCheck update; there are no new failures. The
new recursive-writer fixture separately passes all four matrix revisions.

The current quality ledger is `disjoint-mask-sealed-ledger-20260906/`;
`absolute-unknown-range-trial-20260906/` preserves its optimized outputs exactly.
All 694 original successful UI IDs and eight known failures match in each mode;
31 added successes remain separate in those captures; the recursive-writer
fixture was added afterward and is separately verified. Both hot lanes retain all 15 runtime cases,
175 ordered gas labels and exact observations. Nine heavy captures match the
original inputs/settings, 1,672 contract IDs and 3,344 artifacts, including
1,002 empty outputs and 14 linked-placeholder artifacts. The latest compiler-time
change shares raw bounds during immutable assembly (`a904c535`); its complete
UI/hot/heavy output and 63 focused diagnostic/encoding pairs are exact.

Recent expectations were reviewed with actual execution before updating:

| Reviewed group | Evidence | Result |
| --- | --- | --- |
| Embedded children and dump output (`c062ba84`) | 36 compiles, 90 executions, 32 optimized size and 20 gas comparisons | All oracles agree; no optimized increase |
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
| UI, gas (694 original successes) | +1,522 | +3,287 | — |
| UI, size (694 original successes) | −30,913 | −26,368 | — |
| Hot, gas (15 cases) | +17,717 | +18,154 | −27,793 |
| Hot, size (15 cases) | +25,734 | +26,106 | −92,547 |
| Heavy projects, original settings (9 cases) | +17,838,508 | +14,699,368 | — |

There remain 758/548 larger UI artifacts, 20/19 larger hot artifacts and 24/30
higher hot gas labels in gas/size mode, plus 1,101 larger heavy artifacts.
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
