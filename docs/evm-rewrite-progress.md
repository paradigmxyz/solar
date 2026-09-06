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

At `2a63d1fb`, the fresh scope has 11,591 Rust lines across 33 files, versus
34,638 deleted raw lines: 23,047 fewer. These counts include comments, blanks
and local tests. A historical production-only count was not retained and is
not reconstructed from forbidden source.

## Verification checkpoints

The latest full workspace run (`borrowed-successors/`) has 1,352 passes, one
failing UI aggregate and two skips. Its UI lane has 10,875 passes and 62 snapshot
differences. Five of those expectations have since received isolated execution
review and updates; later source changes still need a fresh full run. The
Foundry lane passes. Standard JSON cases passed inside the full UI run.

Current isolated screens retain 715 successful UI sources per optimization
mode and the same eight known failures. Both hot reports retain all 15 runtime
cases, 175 ordered gas labels and 139 observations. The earlier complete
`cost-call-writer-common/all-corpus/` run compiled all 24 original gas-mode IDs,
including nine heavy compile-only cases. Current short hot reports intentionally
omit those nine; their final remeasurement remains required.

Writer correctness is covered by 531 compiled stress calls and independent
stack/memory models. The internal-call stack-return differential reaches bounded
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

A direct sealed/current checkpoint pair on the same archived input is
54.62→67.63 seconds (+23.8%), with peak RSS 822,828→632,296 KiB
(804→617 MiB). All 432 contract IDs and 348 nonempty creation artifacts match,
with no compile errors. This single pair establishes remaining time debt;
`sealed-time-checkpoint/` retains it separately from the isolated wins above.

Validation remains enabled. Routine comparisons use the debug compiler in this
checkout. Exact commands, source/executable hashes, profiles and measurements
are retained under `target/codegen-bench/evm-rewrite-candidate/`.

## Output quality still owed

The strict ledger at `enum-local-tail/sealed-ranking/` joins original
IDs and ordered call labels. Comment-only source amendments have exact bytecode
proofs and derived reports; sealed reports and the archive remain untouched.

| Matched corpus | Creation-byte delta | Runtime-byte delta | Call-gas delta |
| --- | ---: | ---: | ---: |
| UI, gas (694 original successes) | +2,543 | +4,307 | — |
| UI, size (694 original successes) | −29,824 | −25,306 | — |
| Hot, gas (15 cases) | +22,360 | +22,797 | −27,313 |
| Hot, size (15 cases) | +25,741 | +26,113 | −92,067 |

Aggregates do not pass acceptance: 1,339 individual UI artifacts remain larger;
24 gas-mode and 30 size-mode hot labels remain higher. Current hot size checks
also have 20/19 larger creation-or-runtime artifacts in gas/size. The required
tail observer fix exposes additional size debt rather than retaining unsafe
sharing. All 44 extra UI mode rows are reported separately from the baseline.

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
source-fingerprint reports; use its sealed derived report for future joins.

A three-line identical-stack shortcut was rejected: output was exact, but its
isolated compiler-time pair worsened 65.36→67.13 seconds (+2.7%). The one-line
verifier resize replacement retains exact diagnostics, all UI bytecode and hot
gas across both modes. `scheduler-identical/` and `stack-effect-resize/` retain
all source snapshots and comparisons; the second resize timing pair is under
`scheduler-identical-common/`.

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
