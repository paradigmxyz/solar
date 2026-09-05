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

The latest broad run-call matrix has 1,537 passes and 44 snapshot differences,
with no runtime or compilation failures. The in-repository Foundry lane passed
at an earlier checkpoint and must be repeated at the final source state.
Standard JSON and remaining full UI expectations are not blessed wholesale.
Required stack-call symbolic comparisons reached bounded agreement in both
modes; additional Stop checks timed out and remain explicitly incomplete.

## Latest measured comparison

Frozen checkpoint `exact-pressure-full-corpus/solar` has SHA-256
`e265d25c13ab696b4c7da932daad20b846a42edcb44d93fb044de2068f21d5f2`.
All 15 runtime cases agree with solc on identical 175 ordered hot-call labels.
All nine whole-project compile cases preserve their contract inventories.

| Gas-mode runtime corpus | Sealed baseline | Candidate |
| --- | ---: | ---: |
| Hot-call gas | 5,116,867 | 5,102,307 |
| Creation bytes | 121,544 | 143,075 |
| Runtime bytes | 116,656 | 138,445 |

The aggregate gas improvement does not satisfy acceptance: 73 labels still
regress. The largest runtime-size gaps are Nitro, LibString, SignatureChecker
and Governor. Exact per-case rankings, artifacts, inputs and comparisons are
under `target/codegen-bench/evm-rewrite-candidate/exact-pressure-full-corpus/`.

Matched Seaport gas-mode compilation measured 50.12 seconds and 624.8 MiB peak
RSS, versus baseline 56.54 seconds and 845.0 MiB. Concurrent host activity means
final timing claims still require sequential interleaved confirmation. A
separate diagnostic invocation measured 247 MiB; it is not the matched benchmark
invocation and must not replace that comparison.

The sealed size hot-gas lane covers 15 runtime cases. Supplemental size-mode
whole-project measurements are being recorded separately for both the candidate
and the preserved comparison-only baseline executable; the original archive is
unchanged. Size-mode Seaport is slower than its gas-mode compilation, with the
identical-setting comparison pending.

## Current experiments and commits

Changes are committed in small verified chunks for selective reversion. Recent
commits separately cover bounded verifier contexts (`226a711b`), selector
fallthrough (`774617ed`), scheduling guards (`67aa3226`), terminal pressure
(`b540f90f`), storage metadata (`bf208a31`), terminal stack cleanup (`96bad84d`),
reviewed tail expectations (`eeef3230`) and fallthrough ownership (`3d5668fe`).

Literal reordering before compaction improves all matched runtime cases but
exposes three UI size regressions from disrupted store packing/outline patterns.
Those regressions are being fixed before committing the pipeline group.
Structural cleanup plus final DCE removes substantial bytecode, but one tuple
case exposes a placement regression. This checkpoint also contains concurrent
terminal-edge changes; its recorded results are a combined measurement.
Builds are now serialized through the root agent, with source hashes around
builds, to keep subsequent algorithm comparisons attributable.

A bounded batch outline candidate passes focused effect/stack replay tests. It
can trade gas for size in explicit size mode, so its actual per-label gas and
bytecode measurements remain required. No aggregate improvement overrides an
individual baseline regression.

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
