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

At the scheduler copy-count milestone, the fresh scope has 12,488 Rust lines across
35 files, versus 34,638 deleted raw lines: 22,150 fewer (64.0%). These counts
include comments, blanks and local tests. A historical production-only count was not retained and is
not reconstructed from forbidden source.

## Verification checkpoints

The latest full workspace run (`scheduler-copy-count-20260906/`) has 1,358
passes, one failing UI aggregate and two skips. Its UI lane has 10,950 passes,
74 failures and 806 filtered revisions. The four-line append trial was rejected:
all UI/hot/heavy output is byte-exact with the committed initializer checkpoint,
but timing pairs 51.59→52.36 and 51.71→59.39 seconds show no benefit. The slow
second measurement remains retained; the fresh source is reverted. Workspace/all-target Clippy passes.
All 99 codegen helpers, Foundry and workspace/all-target Clippy pass at the
writer-delta checkpoint; the same 74 failure IDs remain.

The current ledger is `writer-delta-sealed-ledger-20260906/`, with independent
heavy and focused runtime audits.
All 694 original successful UI IDs and eight known failures match in each mode;
30 added successes remain separate. Both hot lanes retain all 15 runtime cases,
175 ordered gas labels and exact observations. Nine heavy captures match the
original inputs/settings, 1,672 contract IDs and 3,344 artifacts, including
1,002 empty outputs and 14 linked-placeholder artifacts. The sealed gate still
fails: 768/555 UI artifacts, 20/19 hot artifacts, 24/30 hot gas labels
(gas/size), and 1,102 heavy artifacts remain larger or more costly.

An earlier writer/call/recursion checkpoint covers 531 compiled stress calls.
The new contiguous fixture adds 1,032 replays and the gas-only sparse fixture
396 before/after/solc executions, alongside independent stack/memory models. The internal-call stack-return differential reaches bounded
agreement with pinned solc in both modes. Reviewed ABI/termination cases retain
513 concrete checks and eight bounded agreements. New tail/terminal-sharing
observer regressions pass 76 concrete executions and their focused UI tests.
Bounds, initial incomplete runs and extra layout-dependent probe failures remain
explicit in the evidence; none is counted as a passing unrestricted proof.

The 74 remaining failures have now been classified from preserved actual output.
All are snapshot mismatches, but replaying existing FileChecks exposes 36 masked
assertion failures; 22 checks pass and 16 have no FileCheck. Three embedded child
creation snapshots reconstruct exactly from one child blob and four derived
length operands apiece. Their optimized artifacts are no larger than sealed;
concrete replay is pending before any expectation update. The complete inventory
is `writer-delta-20260906/failure-triage/`.

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
| Count missing copies once | 52.98→52.16; 53.09→52.28 s (−1.5% both) | RSS −2.7%/+0.9% |

Fresh reversed sealed/current pairs on the same archived Seaport input are
54.58→54.71 and 54.37→52.98 seconds, with peak RSS 867,224→603,696 and
852,504→605,260 KiB. All 432 contract inventories match. These supersede the
older 54.62→67.63 checkpoint debt without claiming a large time improvement.
The isolated initializer measurements are noisy: one pair is 51.81→57.27,
another 52.01→51.68, and the earlier attempt 51.87→51.82 seconds.
No initializer speedup is claimed. The first repeat check rejected reordered
warnings; all contract/source outputs and the complete 90-warning multiset are
exact. Original failure and reviewed rerun remain in
`fmp-entry-frontier-final-20260906/quiet-timing{,-reviewed}/`.

A fresh profile of the committed writer-delta executable on the same 432-contract
input attributes 29.2% of sampled stacks to verification and 15.5% to scheduling
(inclusive scopes overlap). Its generated output and complete warning multiset
match the quiet capture. The profile is retained in
`current-cpu-profile/writer-delta-refresh-20260906/profile.json.gz`; open it with
`samply load target/codegen-bench/evm-rewrite-candidate/current-cpu-profile/writer-delta-refresh-20260906/profile.json.gz`.

Validation remains enabled. Routine comparisons use the debug compiler in this
checkout. Exact commands, source/executable hashes, profiles and measurements
are retained under `target/codegen-bench/evm-rewrite-candidate/`.

## Output quality still owed

The strict ledger at `writer-delta-sealed-ledger-20260906/` joins original
IDs and ordered call labels. Comment-only source amendments have exact bytecode
proofs and derived reports; sealed reports and the archive remain untouched.

| Matched corpus | Creation-byte delta | Runtime-byte delta | Call-gas delta |
| --- | ---: | ---: | ---: |
| UI, gas (694 original successes) | +2,132 | +3,896 | — |
| UI, size (694 original successes) | −30,193 | −25,648 | — |
| Hot, gas (15 cases) | +17,883 | +18,320 | −27,313 |
| Hot, size (15 cases) | +25,740 | +26,112 | −92,067 |
| Heavy projects, original settings (9 cases) | +18,085,004 | +14,900,432 | — |

Aggregates do not pass acceptance: 1,323 individual UI artifacts remain larger;
24 gas-mode and 30 size-mode hot labels remain higher. Current hot size checks
also have 20/19 larger creation-or-runtime artifacts in gas/size. The required
tail observer fix exposes additional size debt rather than retaining unsafe
sharing. New UI sources are reported separately from the baseline. The heavy
corpus has 1,102 larger artifacts: total runtime output grows
23,499,815→38,400,247 bytes. SeaportRouter runtime is now 24,809 versus sealed
9,822 bytes, down from the preceding rewrite's 48,812. These compile-only captures
establish size debt, not runtime correctness for arbitrary heavy contracts.

## Active work and remaining gates

Work is committed in small chunks. Earlier recursion/call scheduling, writer
protection, terminal observer corrections and reviewed constructor/immutable
snapshots remain documented in the preceding commits and checkpoint history.
Their evidence directories are retained under `target/codegen-bench/`.

The gas-only selected writer milestone removes 5,878,394 heavy creation bytes
and 4,669,881 runtime bytes with no individual increase. It protects only the
initialized homes overlapping a primitive store, using a contiguous range or
bounded bitmap. Size mode retains ordinary backups: all-mode trials grew an
existing fixture and were rejected. Arbitrary/wrapping pointers, duplicate
selections and exact stack limits are checked independently; sealed sparse
writer miscompilations are recorded rather than adopted as an oracle.
`gas-writer-protection/`, `bitmap-writer-protection/` and
`targeted-writer-protection/` retain the variants and complete comparisons.

An earlier profile capture accidentally overwrote three candidate files
(command metadata, standard-JSON output and recorder log). Their original bytes
are lost; the old profile/analysis and sealed archive remain intact.
`current-cpu-profile/gas-writer-20260906/artifact-provenance-note.md` records
this limitation. Every new artifact directory is created exclusively.

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
unchanged fallback debts remain explicit. Forced perfect-size output initially improves
152→118 bytes; the subsequent short-tail change below closes its sealed debt.
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
counterexample. The accepted eight-member group reserves five bytes per transfer,
checks every member before rewriting, and leaves the wide-label control exact
(`35c3dbe5`). All 784 raw calls and 38 tail UI revisions pass. The full size
screen changes only the switch fixture, saving 20 creation/runtime bytes; gas
output and both hot suites are exact. Its eight higher local execution-gas
labels remain 21 below sealed. Perfect-size runtime is now 98 bytes versus
sealed 105; that snapshot alone is reviewed at `9e722c60`. Evidence remains in
`multiway-tail-reserve-isolated-20260906/`.

An observer-loop early-exit cleanup was also rejected: two quiet reversed
Seaport pairs measured 51.59→52.84 and 51.84→52.09 seconds, despite exact
outputs. The small source cleanup was reverted; measurements remain in
`literal-diamond-cleanup-20260906/`.

The existing FMP initializer now moves only past a memory-free dispatcher into
one static demanding entry (`7bf717ca`). Reentry, argument setup, multiple
demands and code/gas observation retain global initialization. UI saves 69/69
gas-mode creation/runtime bytes and 177/150 size-mode bytes; heavy saves 10/8,
with no individual increase. Both hot lanes keep all gas labels exact. The SF
getter saves nine gas and is four below sealed. Fifteen actual public-API MIR
compiles and 39 executions preserve explicit FMP4096 across reentry and pin
constructor, nonempty-argument and dynamic-frame behavior. Two positive symbolic
runs reach bounded agreement; SF depth512 runs remain incomplete alongside
324 exact return checks and 252 state transitions.

Five relocation-only snapshots are reviewed at `f35751b1`: sources/FileChecks
stay unchanged, focused before/after revisions pass, and 126 fallback echo
calls have no gas increase. SF's checks now pin stack recursion and fixed
multi-result storage (`6e7bfadf`), with six comment-only bytecode identity
proofs. Recursive 20-argument UI tests separately exercise dynamic allocation.
The SF-amended derived sealed report for future source joins is
`fmp-entry-frontier-20260906/sf-review/sealed-ui-derived-for-future.json`.
Its 1,404 original rows and bytecode totals are unchanged.
The two retained MIR graphs (`db5fd896`) check parsing in CI; their backend
execution evidence is explicitly separate. Evidence lives in
`fmp-entry-frontier-20260906/` and `fmp-relocation-snapshots-20260906/`.

Protected stores now reuse the first aligned address delta for the second
selected home (`9f295e1b`), preserving modular arithmetic, stack peak and exact
memory-access order. All 215 successful focused gas labels save 15 execution
gas. The 724-source screen saves 172 creation/runtime bytes in gas mode; Nitro
saves 920. All none/size output and hot gas labels remain exact. Nine projects
save 1,040,804 creation and 883,146 runtime bytes, with 302 shrinking artifacts
and no individual increase. Six library and 13 immutable metadata fields move
offsets; identity/count/width/placeholder and nonoverlap checks pass.

The focused run retains 2,616 calls, including all 1,962 required
before/after/solc oracle matches and the same 181 known invalid sealed outputs.
All 84 raw boundary pairs and eight UI revisions pass. The symbolic attempt
hits its 60-second cap without a reported counterexample, so remains incomplete.
Quiet Seaport pairs measure 51.81→52.81 and 51.51→53.11 seconds (+1.9%/+3.1%);
RSS is +1.6%/−1.8%. This is an explicit compiler-time tradeoff for measured gas
and size wins, not a compilation speedup. Evidence is in `writer-delta-20260906/`,
`writer-delta-heavy-20260906/` and `router-writer-delta-20260906/`.

A wide indexed-table encoding trial was rejected and reverted. Eleven UI
contracts enlarged 22 creation/runtime artifacts already above or crossing the
sealed baseline. Both hot lanes stayed exact and 1,116 focused executions
preserved results, but those checks do not waive size regressions. A synthetic
16-table case also rose from 0.201 to 1.756 seconds; these exploratory timings
expose repeated label scans, not a project-wide timing claim. The trial added
143 raw lines and is absent from production. Original failures, byte-size joins,
boundary checks and source snapshots remain in `vector-indexed-20260906/`.

The duplicate scheduler now counts each deficit once (`b99b7c37`) instead of
rescanning after every emitted copy. Independent review and 21,222 modeled states
preserve exact operations, first errors and atomic failure. Both quiet pairs
improve about 1.5%. All 724 successful UI cases per mode, both 15-case/175-label
hot lanes, 1,672 heavy contract outputs and 26 retained diagnostics are exact.
All 99 helpers, Foundry and ordinary workspace/all-target Clippy pass; full
workspace retains exactly the same 74 UI failures. An additional `-D warnings`
Clippy invocation failed on existing warnings outside this scheduler change and
is retained separately. Evidence is in `scheduler-copy-count-20260906/`; the
preceding sealed size/gas ledger remains unchanged by exact output identity.

Five more expectations are reviewed at `c062ba84`: three embedded child-initcode
MIR snapshots and the two dump disassemblies. All 36 compiles and 90 actual
executions match current, sealed and pinned solc oracles. All 32 optimized size
comparisons and 20 call-gas comparisons are non-increasing; unoptimized gas
increases remain explicit. Sources and FileChecks are unchanged, and all 14
focused revisions pass. The subsequent trial workspace run has 10,955 UI passes
and 69 failures: exactly these five resolved, no new failure IDs. Evidence is in
`writer-snapshot-runtime-review-20260906/`.

The automatic perfect-table cap16 trial is rejected and reverted. Although it
saves 1,745 UI creation/runtime bytes, 984 hot gas across 24 labels, and
67,966/65,916 heavy creation/runtime bytes without an individual size increase,
a 17-key default path costs 124→167 opcode gas versus sealed 136. Five defaults
create new sealed gas debt. All 1,593 focused executions return correct results;
correctness and aggregate gains do not waive the per-label gate. A shared-budget
constructor grows 18 bytes but remains 18 below sealed, a separately investigated
change rather than the rejection reason. No compiler timing was run after the
decisive gas failure. Evidence is in `auto-perfect-cap16-{trial,focused,heavy}-20260906/`.

Reusing perfect-table scratch storage was also rejected: both quiet pairs
regressed, 52.35→53.09 and 52.36→53.11 seconds, despite exact full output for all
432 Seaport contracts. The source is reverted. The model's allocation reduction
was insufficient evidence for a speed claim; first-attempt clearing also adds
work. `perfect-scratch-reuse-trial-20260906/` retains the measurements and proof.

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
