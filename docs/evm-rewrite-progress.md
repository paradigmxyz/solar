# EVM rewrite progress

The rewrite remains in progress. Functionality is broadly restored; final UI
expectations and per-case gas/size acceptance are still open. Detailed earlier
experiments are retained in [checkpoint history](evm-rewrite-checkpoints.md).

## Scope and architecture

Deletion commit `e5ba34f2` removed exactly the agreed 40 backend files. No
out-of-scope file required restoration. No deleted implementation was read or
recovered. The fresh unsupported API milestone compiled the workspace and all
targets; its source, executable and expected failure inventory remain preserved.

The replacement separates private physical stack scheduling, call/storage
planning, MIR instruction selection, physical block IR and target transforms,
deployment construction, and primitive fixed-point assembly. MIR semantics and
progressive lowering remain in their retained modules. The assembly stream has
no CFG optimization. No legacy backend or fallback is used.

Raw deleted scope was 34,638 lines, including comments, blanks and former local
tests. A production-only baseline count was not retained; it cannot be recovered
without reading forbidden source. Final reporting will distinguish raw scope
reduction from current production LOC.

## Verified milestones

- Scope/evidence audit and compiling unsupported API: complete.
- Fresh IR/text/opcodes, validation, scheduling and encoding foundations: built
  and covered by focused fixtures, relocation tests and stack-boundary replays.
- Runtime, internal calls, recursion, memory writers, deployment and immutable
  handling: restored across the runtime corpus and focused regression matrix.
- Full pinned project compilation: all 24 gas-mode corpus cases compile,
  including Seaport (432 contracts) and V4 (174 contracts).
- Final supported-functionality and output-quality gates: open.

The latest broad run-call matrix has 1,541 passes and 44 snapshot differences,
with no runtime or compilation failures. The in-repository Foundry lane also passes at the corrected terminal-proof
checkpoint and must be repeated if later changes affect it.
Standard JSON and remaining full UI expectations are not blessed wholesale.
Required stack-call symbolic comparisons reached bounded agreement in both
modes; additional Stop checks timed out and remain explicitly incomplete.

## Measured follow-up checkpoints

All measurements retain executable/source hashes, inputs, ordered call labels and
full reports below `target/codegen-bench/evm-rewrite-candidate/`. They describe
controlled experiments, not final acceptance. The sealed baseline is unchanged.

| Isolated change | Measurement |
| --- | --- |
| Streaming size-outline search (`f7ee9bbd`) | Seaport output identical; sampled RSS 5,744,812 → 616,732 KiB, wall time 159.44 → 155.21 s |
| Complete stack-permutation cycles (`87ccad2e`) | Nitro runtime −223 bytes in both modes; six hot labels −198 gas each; two UI size interactions investigated |
| Gas-only zero-store reordering (`2707c12c`) | All 15 runtime cases / 175 labels pass, no immediate gas or size increase; size mode byte-identical |
| Short-lived spill temporaries (`cf3c2ece`) | Nitro runtime −213 bytes in both modes; all 14 ordered gas labels unchanged |

Compiler timing samples remain exploratory because other host jobs were active.
Final timing comparisons must run sequentially and interleaved. The streaming
outline comparison has identical serialized bytecode for both full UI corpora
and all 432 Seaport contracts. It removes the previous large memory regression.

The spill regression passes all four codegen revisions, its MIR matches the
sealed compiler, and both optimization modes reach bounded symbolic agreement
with solc. The broad runtime UI checkpoint has 1,541 passes and 44 existing
snapshot-only differences, with no execution or compiler failures.

The latest complete baseline ranking predates these improvements. It has all 24
gas-mode compilations and all 15 runtime cases / 175 hot labels in both modes,
but still has per-case gas and size regressions. Earlier size-mode Seaport timed
out; later cached/streamed algorithms compile its exact input successfully.
A separate supplemental sealed-baseline run covers all nine size-mode heavy
projects without changing the original baseline archive or reports.

## Current review and commits

Work is committed in small, independently reviewable chunks. Root serializes
builds and freezes executables only when source hashes before/after agree.

The final terminal-prefix pass is committed as `fdd1868b`. Adversarial review
found an incorrect proof based only on net stack effect: DUP/SWAP/EXCHANGE can
read retained incoming words. Actual EVM replay returned `42, 7, 42` instead of
`99, 99, 7`. Whole-region required-stack analysis fixes all three cases. A second
guard rejects legacy shifts whose later legalization needs additional temporary
stack space. Code-relative observations also stop the proof.

Earlier checkpoints containing that new pass are explicitly marked ineligible
for final acceptance (`terminal-proof-status.json`). Their isolated comparative
measurements remain useful where the surrounding source is identical. Corrected
hot-corpus and adversarial replay runs pass; no expectation is blessed to hide a
behavioral mismatch.

The corrected `terminal-proof-fixed/solar` workspace run passes 1,335 tests;
its combined UI lane still fails on reviewed/pending output expectations. The
standalone UI run has 2,805 passes and 132 output differences before the latest
focused snapshot updates. There are no execution or compiler failures. The
existing terminal DCE helpers receive the same conservative legacy-shift and
code-observation guards in separate commit `109798c1`.

Cached tail reachability (`dac18cb2`) preserves exact UI and Seaport bytecode;
exploratory Seaport wall time falls 188.83 → 107.39 seconds. Emitted outline-return
costing (`f7bbf889`) preserves all hot gas labels, removes 222 UI size-mode bytes
without per-contract regressions, and exposes a separately measured compiler-cost
follow-up. Unused fresh private helpers are removed in `4224771b`.

Private sibling static frames (`b29958cc`) and exclusive-entry allocation pools
(`3b655f04`) reduce memory expansion and retained storage without exposing shared
regions to overlapping activations. Their new runtime fixtures reach bounded
symbolic agreement in both modes. Raw `JUMPDEST` barriers (`5af3c4a2`) and unknown
computed-target handling (`5307364f`) fix replayed control-flow/stack hazards.
Unknown targets conservatively disable absolute optimization height proofs and
preserve addressable blocks. This necessary correction increases some outputs;
a bounded return-provenance prototype was rejected because it expanded analysis
states without recovering useful proofs.

Parallel spill copies (`76ef4033`) remove unnecessary staging while retaining
MCOPY opportunities: identical runtime labels keep their gas, and creation/runtime
bytes fall by 246/140 in gas/size modes. The matched UI corpora fall by 1,215/625
bytes without individual regressions. Exhaustive simultaneous-copy maps and runtime
replays pass; the unbounded-loop symbolic fixture reaches depth 512 in both modes
and is not claimed as symbolic agreement. Dying terminal spill values (`efb3af32`)
remove another 683 Nitro bytes in both modes with unchanged gas. Its 16 focused
revisions pass and both symbolic modes reach bounded agreement.

Tiny taken-only terminal sharing (`17d043dc`) preserves all hot gas labels and
removes 21 runtime / 25 creation bytes in each mode. Both UI corpora have no
individual regressions (runtime −1,788/−1,288 bytes), and 612 independent assembled
replays pass. Final heap-store operands (`56cede0e`) remove 967/958 runtime bytes
with unchanged hot gas and no UI size regressions. Fixed-scratch differentials
reach bounded agreement; unrestricted-address solving remains incomplete.

The nearest-duplicate scheduler trial was rejected and restored: its locally
cheaper schedules interact with later zero materialization, increasing Flashloan
size and some UI outputs. A dispatch-clustering experiment was also rejected:
one-byte table addresses save extraction gas but lost fallthroughs add 19 bytes.
Both experiments retain their sources and measured evidence without production
fallbacks. Current work examines primitive table encoding and selective spilling.

The latest committed copy/spill checkpoint matches all 15 runtime cases, 175
ordered labels and 139 observations in both modes. Compared with the sealed
baseline, hot gas is 5,116,867 → 5,092,048 (gas) and 5,189,683 → 5,098,748 (size),
but 30/32 individual labels still regress. Runtime bytes are 116,656 → 133,593
and 113,051 → 129,710. All 694 sealed UI successes remain successful in each
mode, with the same eight known failures. Matched UI runtime bytes are
1,069,788 → 1,086,713 (gas) and 543,106 → 523,308 (size); 1,644 individual
creation/runtime size gates remain unmet. The nine heavy gas-mode cases were
not included in this short runtime rerun and remain pending final remeasurement.
Full rankings are in `committed-copy-spill-ranking/summary.json`.

Output quality, exact IDs and every gas label determine acceptance. The ranking
above predates the terminal and heap-writer improvements and will be refreshed
after the next accepted encoding checkpoint.

## Remaining acceptance

Complete targeted fixes, remove every new baseline failure, and investigate
changed snapshots before accepting them. Run the full workspace, UI, Standard
JSON and Foundry lanes on the final state, plus replay-confirmed differentials,
identical-ID UI size corpora, all-corpus hot gas in both optimization modes, and
whole-project time/RSS. Compare both creation and runtime bytes and every call
label. Finalize architecture/LOC measurements and archive candidate evidence.

The original baseline archive is preserved with SHA-256
`7ddbbe60c1305e0fbb411afdc2a7652ee55ac2b2ad71c668995695bd86bde5db`.
All 529 evidence checksums and 2,376 source fingerprints were verified. This
record is a progress report, not a claim that the acceptance gates have passed.
