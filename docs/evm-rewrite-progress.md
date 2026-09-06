# EVM rewrite progress

The rewrite is incomplete. Correctness is broadly restored, but remaining UI
assertions and individual generated-code gas/size regressions block acceptance.
The draft is [PR #1388](https://github.com/paradigmxyz/solar/pull/1388).
Detailed experiments, rejected trials and commit evidence remain in
[checkpoint history](evm-rewrite-checkpoints.md), with the original contract in
[the handoff](evm-rewrite-plan.md).

## Scope and architecture

Deletion `e5ba34f2` matched all 40 agreed files; nothing outside scope required
restoration. No deleted implementation was read or recovered. The fresh
unsupported API milestone compiled the workspace before functionality returned.

The backend separates private stack scheduling and frame/spill planning,
MIR instruction selection, physical block IR transforms, deployment construction
and primitive assembly. The assembler writes fixed bytes directly into one
buffer with sparse relocations and computes the least fixed point of label
positions and PUSH widths. There is no Atom stream or assembly-level CFG
optimization. MIR semantics remain in their retained layers. No legacy backend
or temporary unsupported rewrite fallback is used.

At the gas-only writer milestone, the fresh scope has 12,114 Rust lines across
35 files, versus 34,638 deleted raw lines: 22,524 fewer (65.0%). These counts
include comments, blanks and local tests. A historical production-only count was not retained and is
not reconstructed from forbidden source.

## Verification checkpoints

The latest full workspace run (`gas-writer-protection/`) has 1,358 passes,
one failing UI aggregate and two skips. Its UI lane has 10,911 passes,
78 failures and 806 filtered revisions. All 78 failures reproduce with the
pre-writer executable, including three Standard JSON expectations. Foundry and
workspace/all-target Clippy finish successfully; existing warnings remain.
The two new writer warnings were fixed in an output-equivalent cleanup.
The focused helper lane passes 99 tests.

The audited writer screens retain 719 successful UI sources per optimization
mode and the same eight known failures. The cleanup screen adds the bitmap
fixture (720 successes); it separately passes all four UI revisions before
and after. Both hot reports retain all 15
runtime cases and 175 ordered gas labels. Nine heavy compile captures complete
the original 24-case corpus, matching 1,672 contracts and 3,344 artifact fields.
The independent ledger is `gas-writer-protection/heavy-audit/`. The sealed
quality gate still fails: 776/563 UI artifacts, 20/19 hot artifacts, 24/30 hot
gas labels (gas/size), and 1,102 heavy artifacts remain larger or more costly.

An earlier writer/call/recursion checkpoint covers 531 compiled stress calls.
The new contiguous fixture adds 1,032 replays and the gas-only sparse fixture
396 before/after/solc executions, alongside independent stack/memory models. The internal-call stack-return differential reaches bounded
agreement with pinned solc in both modes. Reviewed ABI/termination cases retain
513 concrete checks and eight bounded agreements. New tail/terminal-sharing
observer regressions pass 76 concrete executions and their focused UI tests.
Bounds, initial incomplete runs and extra layout-dependent probe failures remain
explicit in the evidence; none is counted as a passing unrestricted proof.

## Compiler time

These are separate sequential debug-compiler Seaport comparisons with identical
inputs and all 432 generated contracts checked. They are not additive speedups
or a final sealed-baseline comparison.

| Isolated change | Wall time evidence | Memory evidence |
| --- | --- | --- |
| Cache outline stack-height prefixes | 132.95→125.65 s; 132.64→126.69 s (median −5.0%) | RSS +2.0–2.6% |
| Scalar literal cost plans | 137.52→129.18 s; 134.17→124.91 s (median −6.5%) | RSS effectively flat |
| Lazy verifier diagnostic strings | 74.85→68.60 s; 74.61→69.13 s (median −7.8%) | RSS +4.8% / −1.5% |
| Reuse call-entry costs | 70.38→68.63 s (one pair, provisional) | RSS −4.9% |
| Borrow successor targets | 68.37→68.61 s (flat; no speedup claim) | RSS +6.0%, one pair |
| Resize unknown stack outputs | 67.63→66.61; 67.34→67.13 s (−1.5%/−0.3%) | Modest; RSS +5.6%/+0.1% |
| Memoize recursion reachability | 66.10→60.10; 65.60→55.62 s (−9.1%/−15.2%) | RSS +8.7%/−2.7% |
| Stop completed stack counts | 54.83→52.07; 55.84→51.59 s (−5.0%/−7.6%) | RSS +3.1%/+5.5% |

A direct sealed/current checkpoint pair on the same archived input is
54.62→67.63 seconds (+23.8%), with peak RSS 822,828→632,296 KiB
(804→617 MiB). All 432 contract IDs and 348 nonempty creation artifacts match,
with no compile errors. This historical pair established debt at that checkpoint; final matched
sealed timing remains required.
`sealed-time-checkpoint/` retains it separately from the isolated wins above.

Validation remains enabled. Routine comparisons use the debug compiler in this
checkout. Exact commands, source/executable hashes, profiles and measurements
are retained under `target/codegen-bench/evm-rewrite-candidate/`.

## Output quality still owed

The strict ledger at `gas-writer-protection/heavy-audit/` joins original
IDs and ordered call labels. Comment-only source amendments have exact bytecode
proofs and derived reports; sealed reports and the archive remain untouched.

| Matched corpus | Creation-byte delta | Runtime-byte delta | Call-gas delta |
| --- | ---: | ---: | ---: |
| UI, gas (694 original successes) | +2,435 | +4,199 | — |
| UI, size (694 original successes) | −29,791 | −25,273 | — |
| Hot, gas (15 cases) | +18,803 | +19,240 | −27,313 |
| Hot, size (15 cases) | +25,741 | +26,113 | −92,067 |
| Heavy projects, original settings (9 cases) | +19,125,818 | +15,783,586 | — |

Aggregates do not pass acceptance: 1,339 individual UI artifacts remain larger;
24 gas-mode and 30 size-mode hot labels remain higher. Current hot size checks
also have 20/19 larger creation-or-runtime artifacts in gas/size. The required
tail observer fix exposes additional size debt rather than retaining unsafe
sharing. New UI sources are reported separately from the baseline. The heavy
corpus has 1,102 larger artifacts: total runtime output grows
23,499,815→39,283,401 bytes. Router runtime is now 25,978 versus sealed 9,822
bytes, down from the preceding rewrite's 48,812. These compile-only captures
establish size debt, not runtime correctness for arbitrary heavy contracts.

## Active work and remaining gates

Work is committed in small chunks. Recent changes remove allocation costs,
retain safe writer operands, rotate protected results in bounded chunks, and
repair code/gas-observation guards. Reviewed corrections save 121 Nitro bytes
per mode and 15 more UI size bytes; all individual comparisons are retained.
The five-byte enum debt remains unresolved in gas mode. Immutable-read rematerialization
and moving size-only tail merging later were rejected after strict comparison:
the former enlarged 89 existing UI artifact debts and created 18; the latter
enlarged 64 and created two. Rematerialization also worsened five Maple gas
labels in both modes and created four ENS gas debts in size mode. All runtime
observations and identities matched. Sources were restored from fresh trial
snapshots; `nullary-tail-sealed-review/` retains every individual comparison.
A standalone additional argument order retains all previous winning schedules
and skips identical load orders. It saves 54 creation/runtime UI bytes per mode,
36/37 hot creation/runtime bytes in gas/size, and 99 hot-call gas in each mode,
with no individual increases. The new nested-call regression passes all four
matrix revisions; 576 broader differential calls also match. The first matched
Seaport pair is flat (66.35→66.61 seconds); no compile-time win is claimed.
`call-materialization-order/` and `resize-call-order-review/` retain proof.

Tail merging now matches equivalent conditional targets through one empty,
unannotated forwarding hop, preserving pass order and existing observer guards.
It saves 813 creation/runtime UI bytes in size mode with no individual increases;
gas-mode output and both hot lanes are exact. Enum size runtime falls 101→94
bytes (sealed 96); its few local opcode-gas increases remain below sealed gas.
All 32 tail-merge UI revisions, 102 enum checks and 480 raw execution checks
pass. `enum-local-tail/` retains every pair, including PC/GAS and cycle cases.

A fresh frozen-runner checkpoint reports 61 failed and 3,031 passed revisions.
Two failures were executable-name differences in CLI help and pass with a
`solar` symlink; one size timing snapshot now omits the analysis skipped by the
new tail observation guard. All six focused help/timing revisions pass, leaving
58 other failures to investigate at that checkpoint. A subsequent review repairs
three constructor/memory-offset snapshots after 216 deployment observations,
126 calls and two bounded symbolic agreements; all optimized per-fixture gas
and sizes are nonincreasing versus sealed. Evidence is in `snapshot-three-review/`.
`snapshot-five-review/` retains exact comment-only bytecode proofs and derived
source-fingerprint reports. The subsequent `snapshot-dealloc-review/` report
extends that proof chain and is the sealed derived report for future joins.
Two immutable snapshots are also reviewed: 276 executions verify exact values,
constructor failures, placeholder ranges and complete runtime patch templates.
Only the relative order of immutable IDs 4/5 is unconstrained. Current output
improves over the previously reviewed snapshots, but sealed creation debt and
Widths' +1 size byte and +4/+7 read gas remain open. No symbolic agreement is
claimed for immutable constructors, which that differential runner cannot execute.
All six focused revisions across these five fixtures pass. The deallocation
fixture is also repaired after verified stack-only recursive calls; it retains
continuation, addition and return assertions and excludes memory loads. Its
optimized output improves over sealed. This leaves 52 prior failures to investigate. Details are in `immutable-current-review/`.

A four-line per-analysis cache avoids repeating the same physical graph query.
All 716 UI cases per mode, 15 runtime cases and 175 hot labels preserve exact
outputs. Independent checks retain 26 exact diagnostic pairs and 46 passing UI
revisions per leg; 94 helper tests pass. Both Seaport timing pairs improve with
all 432 contract outputs exact. Cache lifetime and keys depend only on the
immutable graph; validation/context widening remain unchanged. Evidence is in
`verifier-recursion-memo/`.

Terminal memory reads now participate in the global free-memory-pointer
initialization check. Assembly return/revert previously returned zero when they
read slot 64 directly; the fix matches the preserved compiler and pinned solc.
The frozen replay retains 288 calls plus 180 isolated calls, and all four new
UI revisions and 94 helper tests pass. Both hot reports remain byte/gas exact.
The 717-success UI screen adds 127 creation and 109 runtime bytes per mode:
82 larger artifacts, including 51 enlarged sealed debts and one new sealed debt
(`TryErrorCatch` size creation 612 versus sealed 608). These correctness costs
remain explicit and were recovered in part by the definite-write change below.
`fmp-terminal-reads/` retains failures, corrected test directives, isolated
return/revert and disjoint cases, source hashes and independent comparisons.

A separate same-block definite-write query now removes redundant initialization
when constant MSTORE/MSTORE8 operations define every FMP byte before a terminal
read. It retains the instruction/MSIZE scan and stops coverage at internal calls.
The isolated screen saves 88 creation and 70 runtime bytes per mode with no
individual increases; both hot reports are exact. All four new UI revisions,
300 isolated and 72 fixture calls match their oracles; the combined helper lane
passes 97 tests. The partial-write and call-barrier controls remain byte-exact.
`fmp-definite-writes/` retains the independent proof, final-source amendments
and corrected comparisons. This recovers most of the preceding correctness cost.

A three-line identical-stack shortcut was rejected: output was exact, but its
isolated compiler-time pair worsened 65.36→67.13 seconds (+2.7%). The one-line
verifier resize replacement retains exact diagnostics, all UI bytecode and hot
gas across both modes. `scheduler-identical/` and `stack-effect-resize/` retain
all source snapshots and comparisons; the second resize timing pair is under
`scheduler-identical-common/`.

Gas-mode writer protection now saves only the two initialized aligned homes
that an arbitrary MSTORE can overlap. A contiguous range or a bounded bitmap
selects those homes; holes and protocol words retain ordinary protection.
Size mode keeps ordinary backups: the ungated candidates increased one fixture
by 24 and then 14 bytes, so those variants were rejected. The gas-only change
has no individual increase against its immediate predecessor across UI, hot
and all nine heavy projects. Heavy creation shrinks 5,878,394 bytes and runtime
4,669,881 bytes across 302 artifacts; Nitro shrinks 3,557 bytes with hot gas
unchanged. This is progress toward, not passage of, the sealed baseline gate.

A quiet Seaport pair takes 53.34→55.09 seconds (+3.3%) and 630,724→605,956 KiB
RSS (−3.9%); no compiler-time win is claimed. A fresh CPU profile identifies
scheduler count scans as a candidate for a separate output-neutral trial.
During profiling, three older candidate files (command, compiler output and
recorder log) were accidentally overwritten. Their original bytes are lost;
the older profile/analysis and sealed baseline archive remain intact. The
fresh run and explicit provenance limitation are retained separately in
`current-cpu-profile/gas-writer-20260906/`. New capture directories must be
created exclusively before opening outputs.

`targeted-writer-protection/`, `bitmap-writer-protection/` and
`gas-writer-protection/` retain all rejected variants, exact mode-boundary
comparisons and independent memory/stack execution. The sparse regression
exposes wrong checksums in the sealed compiler; pinned solc and before/after
rewrite executions agree. Those sealed discrepancies are retained explicitly,
with valid gas-comparison denominators kept separate.

The four-line scheduler count shortcut is accepted at `9f4c880e`. Excess-value
removal makes equal total lengths sufficient to stop duplication counting.
All nine project outputs, UI bytecode, both hot lanes, 26 retained diagnostic
pairs and 99 helper tests remain exact. One strict UI stderr comparison found
warning reordering: an independent replay observes five orders from the same
before executable, identical complete diagnostic multisets, and byte-identical
single-thread output. The original failure remains preserved beside this
supplemental proof in `scheduler-count-completion-20260906/`.

Size-mode exact-stride switches now subtract the minimum and rotate rather
than hash and equality-check every leaf (`c22f10da`). All 510 concrete calls,
three bounded symbolic agreements, three UI revisions and 99 helpers pass.
The 721-source size screen saves 120 creation/runtime bytes (36 from an
existing source), with no increases; gas-mode output and both hot lanes are
exact. Seventeen local opcode-gas increases remain below sealed gas, while
unchanged fallback debts remain explicit. Forced perfect-size output improves
152→118 bytes but still exceeds sealed 105, so its snapshot remains untouched.
`stride-switch-20260906-1/` retains first incomplete symbolic attempts,
subsequent bounded agreements and the exact per-case comparisons.

The common switch snapshot alone is reviewed at `b2587a3a`: 39 measured cases
save 11–42 opcode gas, all four contracts shrink versus sealed, existing
FileChecks are unchanged, and the focused UI test passes. This resolves one
of the 78 failures at the preceding full checkpoint.

Exclusive literal branch arms now fold to arithmetic selection (`33f58353`).
Existing CFG counts and observer guards establish safe removal and equal stack
peaks. The 721-source screen saves 117 creation/runtime bytes in gas mode and
179 in size, with no increases; both hot lanes and all 1,672 heavy contracts
are byte-exact. Twelve new UI revisions, 280 raw executions, 192 Solidity
comparisons and 99 helpers pass. Spill-store size runtime falls 110→105 versus
sealed 109; its gas-mode 14-byte debt stays open. A bounded arithmetic
symbolic run is incomplete, with no replayed counterexample. Evidence is in
`literal-diamond-draft-20260906/` and `literal-diamond-heavy-20260906/`.

An initial three-member short-tail sharing trial was rejected after an
adversarial wide-label program grew 337→339 bytes. Its 760 execution checks
retain correct results, but correctness does not waive the size failure.
`multiway-tail-draft-20260906/verification/width-retry/` preserves the
counterexample. A stronger transfer reserve is being tested separately.

Finish all supported behavior and investigate every remaining assertion before
updating it. Repeat full workspace/UI, Foundry, differential, both size corpora,
all 24 project compilations and both identical-label hot-gas lanes on the final
state. Require no per-case creation/runtime size or call-gas increases under
-Ogas or -Osize, then compare compiler time/RSS sequentially. Finalize LOC and
archive candidate evidence. Simpler or faster compilation cannot waive output
regressions.

The original archive SHA-256 remains
`7ddbbe60c1305e0fbb411afdc2a7652ee55ac2b2ad71c668995695bd86bde5db`.
All 529 baseline evidence checksums and 2,376 source fingerprints were verified.
