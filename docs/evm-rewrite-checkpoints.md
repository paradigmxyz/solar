# EVM rewrite checkpoint history

## Evidence and scope audit — 2026-09-05

The starting deletion commit is `e5ba34f2d3676b493aed6dc120c90a1b16605c0a` over baseline `9cb036c0`.
Git path/status metadata matches all 40 paths in the deletion manifest exactly;
there are no out-of-scope deletions to restore. No deleted source content was
read or recovered. Git numstat records 34,638 removed lines, including comments,
blank lines and former backend-local tests. A production-only baseline LOC count
is not recorded and cannot be reconstructed without reading forbidden source;
report raw scope LOC separately from current production LOC.

The baseline archive SHA-256 matches
`7ddbbe60c1305e0fbb411afdc2a7652ee55ac2b2ad71c668995695bd86bde5db`.
All 529 extracted evidence checksums and all 2,376 retained input hashes match.
The retained inventory has 703 Solidity, 201 MIR and 114 EVM IR fixtures under
`tests/ui/codegen/`, plus 35 in-repository Foundry projects. Baseline artifacts
remain unchanged under `target/codegen-bench/evm-rewrite-baseline-9cb036c/`.

## Verification coverage

Retained high-risk coverage includes phi transfers with still-live sources,
recursive frame arguments, deep stacks, empty runtimes, constructor argument and
immutable boundaries, runtime/deployment data, switches, relocation cascades,
extended stack access, stack overflow validation, and Standard JSON opcode,
link-reference and immutable-reference output.

A fresh `lowering/run-call/fmp_partial_writes.sol` fixture covers byte and word
stores overlapping the free-memory-pointer slot, preserving the earlier
observation across a branch join. Its four exact runtime calls run across the
standard codegen matrix. The sealed baseline and solc agree within the recorded
symbolic bounds in
both gas and size modes (exit 0); independent concrete Foundry execution
confirmed all four exact returns for both compilers in both modes. Evidence is
in `target/codegen-bench/evm-rewrite-candidate/fmp-partial-writes-baseline/`.
The frozen scalar candidate passes all four runtime calls in none/gas/size and
shows bounded agreement with solc in gas/size mode. Its new MIR expectation was
authored only after baseline and candidate MIR matched exactly. This added case must be reported
separately from the sealed baseline denominator.

The saved unsupported milestone was exercised with the retained annotation-aware
UI runner, linked into a temporary artifact that selects `solar-stub` directly.
The UI lane reported 2,111 expected failures and 783 passes; Standard JSON
reported 11 failures and 15 passes. Logs retain complete case/revision IDs and
diagnostic differences. The separate source-size screen retained all 703
primary inputs in both gas and size modes (702 sealed cases plus the new
regression); every compilation failed and no success-shaped bytecode escaped.
These failures are milestone evidence only, never accepted regressions.

## Milestones

- Evidence audit: complete; no restoration required.
- Unsupported compiling API: complete; workspace check/build of all targets and
  CLI/Standard JSON smoke diagnostics passed. The stub source and executable
  are retained in `target/codegen-bench/evm-rewrite-candidate/milestone-2/`.
  This is not functional success.
- Independent IR/encoding: pending.
- Scalar/control-flow execution: scalar values, lazy validated calldata words,
  branches, switches and simultaneous phi transfers execute; broader restoration
  remains in progress.
- Calls/memory/deployment: fresh `storage.rs`, `calls.rs` and machine integration
  compile. Internal-call stack-return runtime checks pass; recursive runtime
  checks currently expose unbounded call-cycle growth in physical IR validation.
  Constructor/immutable/data restoration and verification continue.
- Output quality and final acceptance: pending.

No snapshots have been blessed. Full workspace/UI/Standard JSON/Foundry,
replay-confirmed differentials, corpus-size and hot-gas acceptance remain open.

## Reconstruction checkpoints

The physical IR parser/verifier now passes all 20 retained validation fixtures.
Fresh opcode/disassembly helper tests and four stack scheduler tests pass; the
scheduler tests replay 4,000 deterministic random layouts plus target depth and
failure atomicity boundaries. Two pure assembler tests pass for mutually widening
forward references and whole-program-size relocation with fixed immutable width.
The assembler keeps label placement and PUSH widening in one monotone fixed point.

The frozen scalar executable and evidence are under
`target/codegen-bench/evm-rewrite-candidate/scalar/`. Its codegen correctness
inventory reports 938 passes, 1,513 failures and 488 filtered cases. The largest
remaining unsupported categories are internal calls/returns, allocation,
constructor arguments, data copies, select, and immutables. This remains an
incomplete checkpoint, not an accepted functionality or performance result.

A contract gap was resolved from retained `transform/lower_abi.rs` documentation
and generated MIR: external wrappers preserve validated scalar `Value::Arg`
entries as lazy physical ABI head words after clearing callable parameters.
Lowering therefore loads those words on demand at `4 + 32 * ArgIdx`; ABI
validation stays in retained MIR. This avoids inventing a second decoder.

Constructor argument copying and immutable patching are implemented in a separate
deployment module. Appended argument offsets refer to the complete encoded
program size and join the assembler fixed point. Immutable patches preserve bytes
outside their declared immediate width. Runtime verification of this expanded
path is pending integration with independently implemented call/frame lowering.

## Independent implementation and contract findings

`storage.rs` owns checked per-function frame layouts, spill reservations and
late allocation placement. `calls.rs` owns guarded activation setup and memory
restoration. `machine.rs` owns private MIR/return-label identities, entry argument
loads, physical continuation blocks and simultaneous edge transfers. It publishes
extra return words through the retained scratch-pointer convention at `0x20`.
Dynamic allocation begins above fixed storage and the active dynamic frame,
including a conservative frame-size bound through static callees. Memory escape
and transitive FMP effects prevent unsafe reclamation.

The handoff's argument summary omitted a retained interface detail: `lower-abi`
intentionally clears external wrapper parameters while keeping validated
`Value::Arg` entries as physical ABI head words. Its module and wrapper docs
specify lazy calldata loading at `4 + 32 * ArgIdx`; constructor arguments instead
refer to copied memory head words. This was resolved entirely through retained
lowering documentation and execution evidence.

Two focused storage arithmetic/escape helper tests pass. A sample of all 15
sealed runtime MIR artifacts contains 90 allocations among 22,724 printed
instruction/terminator lines (under 0.4%; not every allocation is deferred),
supporting sparse allocation-site maps. Details remain in
`target/codegen-bench/evm-rewrite-candidate/storage-allocation-occupancy.json`.

## Calls, spills, switches, and first complete hot screen

The fresh backend now executes constructor arguments, width-preserving immutable
patches, program-data relocations, internal multi-return calls, recursive frames,
and simultaneous phi transfers. Focused recursive static-frame, live-phi-source,
FMP partial-write and internal stack-return checks passed 15 revisions. The later
shared-public-body and recursive storage copy/delete screen passed all 12
revisions. High-pressure functions use frame-backed value homes and temporary
phi homes; ordinary scalar functions retain stack arguments and values.

Switch strategy selection is isolated in `switches.rs`: source-order chains,
balanced unsigned comparisons, bounded dense tables, modulo buckets and checked
bit slices. One planner shares growth allowances across runtime and deployment.
Wide indexed jumps split in physical IR according to packed-address width before
primitive assembly. All four retained switch-table runtime revisions passed;
14 switch snapshots remain unblessed pending output-quality review.

The frozen `default-cfg-local` checkpoint's hot report is complete, with all 24
case IDs retained. It predates later constructor-return, spill, verifier and
peephole fixes. Strict comparison reports 10 newly failed compilations, one
changed runtime observation set, 104 gas-call regressions and 24 size regressions.
The ENS observation mismatch reproduces a real optimized memory-copy error.
A reduced UI probe isolated stale stack facts after a permutation into an unknown
prefix; clearing those facts fixes the probe. Rechecking the full runtime corpus
is required. Reported successful-case totals are never substituted for the full
baseline denominator. Compile-time samples from concurrent development are
exploratory only.

The same checkpoint's UI screen matches 676 previously successful cases in each
mode, leaving 18 baseline successes failed per mode. For that identical subset,
gas runtime bytes changed from 1,045,318 to 1,192,810 and size runtime bytes from
521,029 to 678,536. These regressions are not accepted. The baseline evidence and
all candidate JSON, bytecode and diagnostic rows remain preserved.

The only deliberately changed existing expectation so far concerns an unsafe
reorder-pushes transformation at stack height 1,022: moving a PUSH above two
producers raises the peak from 1,024 to 1,025. The safer unchanged ordering is
supported by independent instruction replay; other output differences remain
under investigation. New focused regressions cover stack cleanup, unknown-prefix
permutations and boundary pressure. The final full workspace/UI/Foundry,
differential and both-mode output-quality gates remain open.

- Local stack safety review: fixed stale symbolic suffix identities after an
  inaccessible SWAP/EXCHANGE in stack-dedup and block CSE. The isolated ABI decode
  runtime probe now passes all none/cfg/peephole revisions. Permanent reduced
  `unknown_prefix.evmir` fixtures pass in both pass directories.
- Corrected only the authorized `reorder-pushes/chains` expectations: bb2's old
  proposed motion required 1025 stack words; bb3 permits one moved literal at
  1024 words but rejects the next. The reviewed fixture passes; other chain
  expectations are unchanged. Computed-JUMP human annotations now pass the
  retained `none/indexed_jump` fixture exactly.


## Calls, spill safety, and supported-operation audit

The expanded runtime UI checkpoint `writer-control-runtime-ui.log` records
1,522 passes and 41 exact-output snapshot differences, with no runtime mismatches
or compilation failures. Snapshot differences remain unblessed. This includes
recursive frame arguments, recursive aggregate storage operations, mutable writer
spills, wide recursive calls, 100 simultaneously live locals, constructors,
immutables, and the new partial-FMP-write regression. Earlier `calls-runtime-ui.log`
contained 128 real runtime failures: the dominant optimizer bug was stale symbolic
stack identities across inaccessible permutations, independently reduced and fixed
by the opcode agent. Its permanent `unknown_prefix.evmir` regression replaces a
removed temporary runtime probe.

`spills.rs` now owns pressure selection and activation-local home reservations.
Functions inside the target stack window keep the ordinary stack schedule. Wider
functions capture values once in homes and perform cyclic phi transfers through a
separate temporary region. Potentially overlapping writers evacuate live homes onto
the physical stack before the write and restore them afterward. The free-memory
pointer is not restored by this evacuation. The dynamic control pointer is preserved
only while a subsequent internal call or return needs it; restoring it over a terminal
ABI word at `0xa0` was caught by recursive bytes and aggregate runtime tests and fixed.
Prologue FMP initialization now requires a reachable aliasing memory read, memory-size
observation, or dynamic activation. Constructor argument copying sets its own aligned
allocation end.

The final boundary audit enumerated all 125 MIR instruction variants: 88 map to
physical opcodes or explicit machine lowering. The remaining 37 belong to retained
MIR lowering: `AbiEncode`/`AbiDecode` to ABI passes; semantic memory-object operations,
`Keccak256Bytes`, and slice word loads to `lower-memory-objects`; aggregate storage
operations to `lower-aggregates`; mapping/array slot operations to `lower-mapping-slots`;
`FrameLoad`/`FrameStore` to `lower-frame-slots`; `MemoryZero` to `lower-memory-zero`;
`MakeSlice`/`SlicePtr`/`SliceLen` to `lower-slices`; `Fmp`/`SetFmp` and nondeferred
allocations to `lower-alloc`; and `StoreImmutable` to `lower-immutables`. All eleven
terminators have a physical path except `RevertReturndata`, expanded by retained
`lower-abi`. Catchall reconstruction wording is replaced with an explicit unlowered-MIR
boundary diagnostic, not an implementation stub. The retained `lower-evm-shaped`
phase gate explicitly rejects only slices, FMP operations, immutable stores and
nondeferred allocations, relying on earlier named passes for the other invariants;
malformed hand-authored phase claims therefore remain checked errors at the backend.

Still open: performance restoration, review of exact-output differences, final complete
workspace/UI/Foundry/differential verification, and adversarial writer cases whose
unknown callee effects can overlap suspended caller homes. The current ordinary
opcode writer protection is not evidence that those transitive cases are solved.
- Reviewed expectation updates are limited to `constant_rhs` bb6 and
  `cancellations` bb5 folding completely to PUSH0 (3 bytes/6 gas becomes
  1 byte/2 gas), and `push_pop` bb2 reaching the same final DCE output earlier.
  Exact replay is retained in `evm-rewrite-local-expectation-evidence.json`.
  `memory` bb6 retains its reload: saving a copy reaches 1025 words versus the
  original peak of 1024. Other expectations in these fixtures are unchanged.
- DCE can retain an unused incoming word underneath an independent terminal
  body, removing its POP with a proved stack-capacity bound. The new
  `dce/terminal_prefix` fixture covers both removal and the 1024-word rejection.
  The retained ABI decode runtime matrix still passes. Independent replay of
  the new SAR legalization matches 56,734 edge/random cases; evidence is in
  `evm-rewrite-sar-evidence.json`.

### IR sharing evidence

The physical verifier now preserves known return-label provenance through
allocation guards and recognizes recursive transfers on all structural edges.
This matters when CFG simplification removes a guard's empty setup block.
The size-sharing candidate adds exactly `terminal-dedup, share-reverts,
tail-merge, outline, cfg-simplify` before block layout under `-Osize`; its
ordering is retained in the candidate directory. The complete run-call screen
has 1,480 passing revisions and 41 remaining MIR snapshot differences, with no
runtime or compiler failure in that screen.

An independent interpreter replayed all eight outline fixtures from every
original block, using random physical stack prefixes: 7,040 state comparisons
matched in both modes. Three stronger outline expectations were updated after
review. Including the trailing POP in the shared body reduces modeled encoded
size from 48 to 46 bytes and entry-path static gas from 53 to 47 for both
`outline_push` and `label_wraparound`. Sharing the common 2..12 sequence across
three sites in `outline_order` reduces modeled size from 91 to 64 bytes with
entry-path static gas unchanged at 89. These counts come from an independent
fixed-point encoding model and opcode replay, not runtime-corpus measurements.
The script, original test expectations, and JSON are retained under
`target/codegen-bench/evm-rewrite-candidate/ir-evidence`; all eight outline UI
fixtures now pass. Other snapshot changes remain unapproved.

## Full corpus restoration and size-sharing screen

The frozen `calls-spills-switches` checkpoint restores all 694 baseline UI
compilation successes in both modes, retaining the same eight known failures.
All 15 runtime cases match solc in both modes with all 175 gas-call labels.
This is still below the performance gate: gas runtime bytes are 167,167 versus
116,656 baseline, with total call gas 5,323,153 versus 5,116,867. Size runtime
bytes are 168,246 versus 113,051, and call gas 5,329,776 versus 5,189,683.

Later alias-safe spill preservation, unused FMP initialization removal, selected
branch fallthrough, selector consumption and exact addressable-label emission
restore 1,522 focused run-call UI revisions without compiler or runtime failures.
The remaining 41 failures in that lane are output snapshots, not blessed. A
workspace nextest progress run passes 1,334 tests before its UI gate fails;
that UI run reports 10,601 passes and 169 failures. It exposed size-sharing
recursive verifier invariance issues, since fixed and checked independently.
The retained omitted-immutable-lowering diagnostic now matches all four matrix
revisions, and driver failures propagate emitted guarantees without duplicate
stage-error messages.

The `size-sharing` checkpoint adds one recorded sharing group for size mode.
Its UI screen retains all 694 baseline successes: gas runtime bytes are
1,194,487 versus 1,069,788 baseline; size runtime bytes are 625,926 versus
543,106 baseline. The gate remains blocked by individual size regressions.
Both requested `stackAcross(uint256)` symbolic comparisons report bounded
agreement; full generated projects, bounds and result JSON are preserved.

One attempted concurrent hot run used the same RPC port for both modes. It was
interrupted and every artifact was quarantined under
`size-sharing/invalid-shared-rpc-run/`, explicitly excluded from acceptance.
Replacement runs use separate explicit RPC ports. Final compiler time/RSS
comparisons still require sequential interleaved runs without development load.

The Byzantium shift snapshot was also updated after two independent bounded
checks: 56,734 fresh-sequence cases and 50,112 retained-expectation versus
fresh-sequence versus integer-oracle comparisons. The latter cover zero,
negative/sign-boundary values, and shifts at 255, 256, 257, and the maximum word.
For the SAR sequence, encoded size drops from 42 to 18 bytes, each checked path
saves 71 opcode gas, and peak stack occupancy drops from nine to four words.
This is concrete replay evidence rather than a universal equivalence proof.
The original expectation and replay helper remain under `ir-evidence`;
Constantinople-and-later expectations are unchanged.
- Reviewed DCE saved-value changes preserve the ordered memory observations and
  exact stack at opaque markers in 100 bounded replays per block and mode; all
  fifteen changed block/mode pairs reduce both bytes and static gas. Evidence:
  `evm-rewrite-dce-expectation-evidence.json`. Normalizer `runs` bb3 selects a
  different exact permutation decomposition with the same seven SWAPs (7 bytes,
  21 gas); only its canonical-order expectation changed.
- Preferred live-prefix preparation now scores the last branch-producing opcode
  together with its unique cyclic successor's phi transfer. The frozen candidate
  reduces `sum-10-200` from 46,542 to 45,399 gas (1,143 saved), and runtime code
  from 133 to 132 bytes; the original baseline remains 45,385 gas. All four hot
  labels match solc, and the new `loop_live_order` standard matrix checks both
  successful loops and exact sum/increment overflow panic data. The earlier
  whole-function entry-order search was removed because it only relocated the
  remaining loop SWAP. Candidate artifacts: `evm-rewrite-candidate/preferred-prefix`.

## Call-boundary fixes and pressure measurements

The new `suspended_writer_spills` fixture exercises 20 live words across a recursive
callee that overwrites low memory. All four revisions pass. Its fresh MIR expectation
matches the sealed baseline exactly; a requested symbolic comparison is explicitly
incomplete because solc rejects the unsafe-assembly fixture as stack-too-deep. This is
not a differential pass. Live caller homes and active dynamic header words now survive
callee writers. Functions that need no addressable frame retain stack activations even
when they manipulate heap memory. Multi-return stack activations publish extra results
in a fixed per-function scratch buffer, consumed immediately by their callers.

Two real call-contract gaps were reduced. V4's never-returning `revert_error` acquired
a phantom return-label requirement from syntactic call sites; return classification now
uses reachable call sites and reachable return exits. The complete pinned V4 input
compiles 174 contracts without errors. Seaport exposed the overloaded MIR `Stop`:
retained inlining treats it as an internal return only for void functions, while
`inst-simplify` uses it for zero-length external returns in value-returning bodies.
The new `value_returning_stop` fixture fails physical verification with the earlier
rewrite executable and passes all four revisions after applying that same distinction.
Its new MIR expectation also matches the sealed baseline. Reduced TestBadContractOfferer
compiles; the direct full Seaport archive run was killed with exit 137 and produced no
JSON, so that full-project run remains incomplete pending the identical benchmark check.

Pressure selection now executes the private scheduler on canonical live layouts and
simultaneous phi edges. This avoids reserving homes merely because a dying operand was
counted twice in a conservative peak bound. Interval-based home reuse and single-step
stack temporaries reduce the remaining frame footprint. The broad run-call and deep-stack
screen records 1,536 passes, 45 snapshot-only differences, and no runtime or compiler
failures (`exact-pressure-runtime-ui.log`). Exact-pressure hot results preserve all 18
LibString call labels and match runtime outputs: medium replacement is 30,437 → 28,702
gas and long replacement is 33,117 → 31,522. LibString total hot gas is 464,037 → 456,153;
runtime bytes remain regressed at 55,415 → 59,281. The measurements are retained in
`exact-pressure-hot.json`; these localized wins do not satisfy the remaining whole-corpus
size and per-label performance gates.

The final pressure-entry guards passed 1,537 focused runtime revisions; the 44 remaining failures are output snapshot differences, with no runtime or compilation failures. Both symbolic attempts for `value_returning_stop` timed out after 60 seconds and are recorded as incomplete; exact runtime checks and the reduced Seaport compiler reproduction remain the available evidence.

The full Seaport input exposed exponential combinations of physical return-label
provenance before the first EVM pass in `CriteriaHelper`. The verifier now widens
after sixteen label contexts for the same block and exact stack height. It
forgets label identities, retains the height, and marks potentially affected
successors and caller continuations unproved; optimizations therefore cannot use
invented stack bounds. Distinct heights remain separate, so unbalanced loops
still reach the stack-limit diagnostic. All 26 validation/encoding/peak cases
and 33 recursion revisions passed. The identical full-project input then
compiled 432 contracts without diagnostics in 241.1 seconds with `-j1` and
`-Ztime-passes`, peaking at 252,752 KiB RSS. The earlier precise-state profile
hit the explicit 1.5 GB debug cap after 18.6 seconds and was terminated; this
aborted run is not a completed timing baseline. Both profiles are preserved.

The approved tail-sharing expectation update retains all original state effects
in 1,800 concrete replays. Its independent model gives 121 to 120 bytes, and
entry-path static gas 56 to 44; no original block entry gets a higher modeled
gas cost. All four tail-merge fixtures pass. The preferred-loop-prefix helper
also received an independent review: its checked permutation preserves the
fixed prefix, live-value set and opcode operands, with profitability measured
for the selected cyclic successor rather than claimed for every exit path.


## Detailed record through `2a63d1fb`

The following preserves the full earlier progress record. Its intermediate
counts and acceptance statements describe their named checkpoints; the current
status is in [the concise progress record](evm-rewrite-progress.md).

# Archived EVM rewrite progress

The rewrite remains in progress. Functionality is broadly restored; final UI
expectations and per-case gas/size acceptance are still open. Detailed earlier
experiments are retained in [checkpoint history](evm-rewrite-checkpoints.md).

## Scope and architecture

Deletion commit `e5ba34f2d3676b493aed6dc120c90a1b16605c0a` removed exactly the agreed 40 backend files. No
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

## Current measured increments

Wide packed targets (`99066af8`) use low-first extraction on native-shift forks;
power-of-two index scaling (`0c5862e2`) uses SHL. Together they save 624 gas
across 78 hot labels, with no label increase. Wide extraction removes 30 hot
runtime bytes and 498/14 UI runtime bytes in gas/size mode; scaling preserves
all sizes. Pre-shift forks retain their measured encoding. Independent assembled
replays, wrapping arithmetic checks and 80 codegen unit tests pass.

Repeated terminal-prefix removal (`e61ed3db`) recomputes stack bounds after each
accepted region. It preserves hot gas, removes 26/14 hot runtime bytes and
765/599 UI runtime bytes, with no individual size increase. A 1,022-word replay
checks cumulative capacity; deliberately removing the required remaining POP
causes the expected stack overflow. Six focused revisions pass.

Bounded cross-block residents (`23767c91`) keep arguments, Phi values and values
crossing unsafe writers or calls homed. Nitro shrinks 458/479 bytes and saves
108 gas in both modes. The identical 708-success UI inventories shrink
1,710/1,450 runtime bytes without individual regressions. All 36 focused
revisions pass; both symbolic modes reach bounded agreement. These are isolated
comparisons under the recorded common checkpoint, not final baseline acceptance.

The retained MIR argument-normalization trial produced byte-identical outputs
across both UI corpora and both hot modes, so its redundant driver call was
removed. Trial sources and comparisons remain in `canonical-arguments/`.

## Correctness review and current trials

The latest full UI checkpoint (`committed-entry-modulo/`) has 2,864 passes and
128 output differences; every reported failure is an output comparison. Its
Standard JSON lane has 15 passes and 11 differences. Investigation found a real
metadata bug among those differences: data compaction discarded unreferenced
mandatory runtime trailers. Fix `82b7903f` retains opaque trailing bytes, including
nested child metadata. Metadata hashes, immutable and library offsets, actual
deployment/library calls, and eight runtime/eight deployment capture round trips
pass. Reviewed Standard JSON expectation updates remain a separate checkpoint.

Fixed low-memory residence (`5b840d62`) removes 462/459 Nitro runtime bytes and
66 hot gas in each mode. Existing UI cases shrink 995/1,176 bytes without an
individual increase; the new recursive boundary fixture saves 2,058 more in
each mode. Both symbolic comparisons reach bounded agreement.

Terminal sharing needed additional correctness guards (`57d4cbf9`): independent
replays exposed changed PC/GAS values, escaped numeric labels and a 1,024-word
stack overflow. Nine focused fixtures and eight replay paths now pass. Gas mode
is unchanged, but conservative guards add 1,812 hot runtime bytes and 8,596 UI
runtime bytes under size optimization, with seven hot-label increases. These
are explicit performance debts. Reviewed private control-label provenance is
being implemented to recover safe sharing of generated continuation labels.

Post-compaction CSE (`54e9757f`) runs only in gas mode. It preserves every hot
label and removes five hot runtime bytes and 106 UI runtime bytes, with no
individual increase. The size-mode trial was rejected after it disrupted tail
sharing. Broad and storage-only dying-operand trials were also rejected after
local swap savings caused downstream regressions. The next trial pays the
complete block schedule and restores its exact original exit stack.

## Reviewed snapshots and static argument checkpoint

Private continuation provenance (`991c504b`) recovers 6,408 UI runtime bytes
under size optimization while retaining the escaped-label guards. Both hot
lanes execute correctly; its local address-width increases remain documented
in `private-control-labels/`, rather than treated as final acceptance.

The metadata fix and independently reviewed Standard JSON updates (`0e139223`)
now pass all 26 Standard JSON fixtures. Embedded child reconstruction certifies
49 MIR expectations (`4891eaa9`): 38 data/length changes and 11 inline-store versus
data-copy changes, with 147 runtime revisions and 30 FileChecks passing. Three
program-data physical snapshots (`c3272576`) additionally pass exact child-byte
and six assembly-round-trip checks plus all 15 selected revisions. No runtime
expectations or existing FileChecks were weakened. These reviews leave 76 of
the earlier 128 output differences for separate investigation.

Static spilling calls receive bounded arguments directly on the stack
(`65d46925`), reserve their spill homes in the existing frame ancestry plan,
and establish canonical entry state once. All 709 matched UI cases per mode
retain their contracts and success status; runtime and creation size each fall
by 1,387 bytes under gas and 1,112 under size with no individual increase. All
15 hot cases, 175 ordered gas labels and 139 observations remain exact. The
new nested overlapping-writer fixture passes four standard revisions, 72
independent calls, and a reduced bounded symbolic comparison. The unrestricted
symbolic timeout is retained as incomplete.

The latest workspace attempt passes 1,341 tests; its one failing UI runner
reports the same pre-review 128 output differences, with two skipped tests.
This is not a passing workspace result. The refreshed sealed comparison under
`static-spill-sealed-ranking/` still has 27/24 regressing hot gas labels and
22/18 regressing creation/runtime artifacts for gas/size, plus 1,464 UI artifact
increases. The 15-case runtime lane does not cover the nine heavy compilation
cases. Smaller isolated improvements do not satisfy these final gates.

## Bounded scheduling and target finishing

Small commits continue to separate implementation, reviewed expectations and
formatting. Whole-block dying-operand scheduling (`b1e74700`) saves 113 UI runtime
bytes and 117 creation bytes under gas optimization without individual increases;
size outputs remain exact. Five further data/immutable snapshots (`8418d744`)
pass independent assembly and boundary reads. The immutable byte-patch FileCheck
still requires a separately reviewed order-independent correction; it remains
withheld rather than forcing production order to match an incidental snapshot.

Earliest eligible taken exits (`65c18094`) remove 3,748/202 UI runtime bytes and
10 hot runtime bytes under gas/size, with unchanged hot gas and no individual
size increases. Increasing the ranked outlining queue (`642b4b40`) retains the
same eight-emission bound and removes 3,492 UI runtime bytes under size. Its
21 locally increased hot gas labels remain below the sealed baseline. On the
identical 432-contract Seaport input, creation/runtime totals fall by
132,148/61,697 bytes with no individual increase; sequential compilation takes
98.866/99.164 seconds before/after and peak RSS is 604,396/609,596 KiB.

Literal/copy ordering (`06d0ea37`) removes 2,018/1,730 UI runtime bytes and 22 hot
runtime bytes in both modes. Two local size increases were traced to outline
selection and remain below sealed sizes. Observation and stack-run boundary
guards fix independently reproduced PC changes and downstream regressions.
Strictly smaller caller-entry fusion (`8f70151e`) removes 389/382 UI runtime
bytes and 953/1,005 hot runtime bytes, with no individual size or gas increase.
Its reduced duplicate-argument fixture passes 384 independent calls against
solc and both compiler legs. Both trials retain rejected broader variants and
exact evidence; those rejected implementations are not active fallback paths.

The API audit found required old-target lowering missing from custom pipelines:
Byzantium shifts emitted unavailable instructions, and Homestead termination
needed INVALID rather than REVERT. The finishing fix and focused regressions
are under final validation. Its first complete pair preserves all 712 UI cases
per mode and all 15 hot cases byte-for-byte, including 175 ordered gas labels.
The MIR-capture audit has 36 matching runs across optimization and output modes.

The sealed ranking after strict caller-entry fusion is retained under
`strict-call-sealed-ranking/`. All 694 sealed UI successes still compile with
the same failures, but 1,361 individual UI size gates remain unmet. Matched UI
runtime totals are 1,069,788 -> 1,073,234 under gas and 543,106 -> 514,663 under
size. Hot totals are 5,116,867 -> 5,090,397 gas and 5,189,683 -> 5,098,661 size,
with 27/32 individually regressing labels. Runtime bytes remain
116,656 -> 130,461 and 113,051 -> 127,968, with 20/18 creation/runtime artifact
increases. The nine heavy gas cases are explicitly missing from this short
rerun. These aggregate improvements do not satisfy the final per-case gates.

## Latest committed validation

Required target finishing (`f269caad`) is committed with old-fork custom-pipeline
regressions and original-input public API validation. Its checked pair retains
all default UI/hot bytes and gas labels. The immutable byte-patch expectation
(`1035e270`) now checks three complete disjoint patches without imposing their
relative order. Forty boundary reads and twelve invalid constructors pass in
both modes. Both compilers preserve exact bytes after the comment-only source
amendment. Supplemental sealed rows at the real test path are retained under
`immutable-byte-patch-review/both-modes/`; `sealed-ui-derived.json` replaces only
those two remeasured rows and the one source fingerprint. The original archive,
rows and measured creation-size debt remain unchanged.

Dying operand groups (`f252665b`) compare against the existing unary whole-block
winner before acceptance, fixing two reproduced decoder regressions. The paired
712-case corpora have no individual increase: gas runtime/creation fall by
447/525 bytes and size is exact. All 15 hot cases preserve outcomes and 175
ordered labels; gas runtime falls by 113 bytes and gas by 216. The activating
dynamic-field fixture passes 384 independent calls and both bounded symbolic
modes. Sixteen disjoint outline groups (`79c567b1`) save another 123 UI size-mode
bytes, with hot bytes/gas exact. All 432 Seaport contracts compile, twelve shrink
and none grows: creation/runtime each fall by 1,111 bytes. Sequential timing is
98.123/98.125 seconds and sampled RSS is 558,032/584,192 KiB before/after.

The committed source hashes and debug executable match this frozen checkpoint.
Workspace nextest has 1,343 passes, one UI-runner failure and two skips. The
codegen UI lane has 2,523 passes and 71 output differences; all failures are
output comparisons still requiring review. The in-repository Foundry lane
passes. These are checkpoint results, not a completed acceptance run.

`multi-outline-sealed-ranking/` still records 1,357 UI artifact increases,
27/32 hot gas-label increases and 20/18 hot artifact increases in gas/size.
The nine heavy gas cases remain absent from this short runtime report. Current
work investigates the dominant Nitro wrapper's spill traffic. Independent
review rejected a proposed region-only call-write proof: backward arithmetic
can make a heap-derived pointer overlap compiler homes. No such optimization
was implemented; direct-writer behavior is being tested before further tuning.

## Draft PR checkpoint

Derived memory-write protection (`00c98255`) fixes a replay-confirmed corruption:
a twenty-input sum returned `0xdeadbfb4` instead of `210` after backward pointer
arithmetic overwrote a live spill. Exact physical frame/allocation ranges replace
coarse region assumptions. All four new UI revisions pass; 54 mismatches across
78 identical reference-agreement cases become zero, and all 96 candidate calls
return the expected sum. The pure range tests pass. A post-fix symbolic query
limit remains incomplete, not a general equivalence claim.

The correctness fix preserves all 714 size-screen case inventories per mode and
all 15 hot observations/175 ordered labels, but costs 12,681/12,693 hot runtime
bytes and 132 gas in each mode. There are 110 local UI artifact increases and
seven hot artifact/six hot-label increases per mode. This increases outstanding
performance debt and is not final acceptance. Identical-schedule costing
(`4a5f4d6f`) separately removes redundant analysis with exact UI/hot bytecode.
The residence-window cap-10 trial was rejected: one existing deep-stack fixture
grew five bytes in both modes, worsening sealed debt despite small Nitro gains.

Two disassembly reviews (`945f6fb1`) retain eight execution comparisons and four
exact capture round trips. The current committed codegen UI run has 2,533 passes
and 69 output-comparison failures; no failures are silently blessed. All 85
codegen helper tests and the in-repository Foundry lane pass. Backend Rust scope
is 10,705 raw lines in 32 files versus 34,638 deleted lines, a 23,933-line (69.1%)
reduction including comments, blanks and unit tests. Production-only historical
LOC was not retained and is not reconstructed from forbidden source.

The PR is a draft for reviewing the architecture and incremental commits.
Functionality reviews, per-artifact/per-label output-quality recovery, final
full-workspace validation and complete heavy-corpus remeasurement remain open.
The sealed archive checksum was reverified unchanged.

## Direct-writer residents

Direct memory writers now keep eligible live residents below an explicit stack
prefix of saved homes. Only writer operands require homes; calls retain the
conservative policy. This adds 46 production lines and a pure scheduler test
covering deep prefixes, duplicate operands and atomic overflow rejection.
All 90 helper tests and 531 same-source runtime calls pass. Isolated comparisons
retain 714 successful UI cases per mode and all 15 hot cases/175 ordered labels.
There are no per-artifact or per-label increases: UI creation/runtime bytes fall
by 1,387 in gas mode and 1,419 in size mode; hot creation/runtime bytes fall by
459 and total call gas by 48 in each mode. Evidence is in `writer-residents/`,
`writer-resident-tests/after-expanded/` and `writer-assembly-common/`.

The independent RPO interval-order trial is rejected. Its Nitro improvement
comes with eight UI artifact increases, including two existing nested-struct
fixtures already above the sealed baseline. Only that traversal hunk was
reversed; the evidence remains in `spill-interval-rpo/assessment/`.

## Assembler byte buffer

Ordinary opcodes, literal pushes, data and fixed immutable placeholders are now
encoded directly into one buffer. Separate ordered label/deferred-PUSH records
replace the per-instruction `Atom` stream. Fixed-point placement scans only
those records; emission reserves the resolved output size. This removes tiny
per-opcode allocations and avoids copying the buffer when no relocation exists.
It adds 66 production-section lines and 129 test-section lines relative to the
fresh previous assembler, with four new pure boundary regressions.

All 714 successful UI cases per mode, 15 hot cases/175 ordered labels per mode,
and 160 paired EVM-IR fixture lanes retain exact output. All 90 helper tests
pass. The sequential Seaport pair preserves all 432 contracts and their exact
creation/runtime bytecode. Time is 164.22 -> 164.69 seconds; this single pair
shows no speedup. Peak RSS is 757,356 -> 739,584 KiB, a 2.35% reduction. Evidence,
source/executable hashes, original outputs and review are in `assembly-buffer/`.
These measurements compare this representation change only; they do not clear
the rewrite's outstanding sealed-baseline performance debt.

The timing/CLI expectation review preserves all MIR timing lines and updates
only the truthful physical-pass sequence and one smaller selector-dispatch bin.
Sixteen sealed/current constructor/dispatch executions agree; all five focused
UI/FileCheck cases pass. Four expectation files change, with originals, raw
outputs and replay traces retained in `timing-cli-review/`.

Compact-push construction now reuses the immediately preceding literal under
complete-pair stack, byte and gas checks. The focused size/Berlin disassembly
regains the sealed cost (one/two bytes and two/three gas saved). The harmless
shifted-string coefficient change is separately value/cost checked before
updating two FileCheck lines. All eight focused cases pass; all 714 UI cases
per mode and both 15-case hot reports remain byte/gas identical for this isolated
change. Evidence and original expectations remain in `compact-adjacent-literal/`.

Spill preservation now checks definition availability before querying future
uses. Existing live-in facts plus a sparse map built during entry construction
exclude not-yet-defined SSA results, including the current call result, while
preserving Phi/loop values and shared live homes. The isolated change removes
579/610 UI bytes and 2,213/2,222 hot runtime bytes in gas/size, with no increases
or changed hot labels. All 531 writer calls and 32 sealed/solc reference calls
agree. A new standard matrix plus raw-IR revision passes all five cases; the
before compiler fails the raw-IR regression. Evidence: `writer-home-availability/`.

Function layouts are now constructed only for the artifact's reachable call
graph, in the same ascending function order. Constructor sparsity justifies a
private sparse map; dense block IDs and storage planning remain unchanged. Across
25 corpus lowerings this omits about 49% of layouts and half the block-entry
scans. Exact UI/hot and 432-contract Seaport output is unchanged. Sequential
Seaport time is flat (140.174 -> 140.171 seconds), with peak RSS 737,000 -> 734,080
KiB; this is a work/allocation reduction, not a demonstrated compiler speedup.
Evidence is in `function-layout-occupancy/` and `reachable-layouts/`.

Dying direct-writer operands can now stay resident when the complete operand and
backup window fits sixteen words. Larger windows retain the frozen-prefix
protocol. This adds 30 lines in production files and one pure scheduler regression.
All 92 helpers and 531 writer replay calls pass. The isolated comparison retains
715 successful UI cases per mode and all 15 hot cases/175 ordered labels, with
no individual byte or gas increases. UI creation/runtime bytes fall by 298/255
in gas/size; hot creation/runtime bytes fall by 103/93 and call gas by 42 in
each mode. Evidence: `dying-writer-operands/` and
`writer-resident-tests/dying-after/`. Sealed-baseline debts remain open.

Call-entry fusion now considers the complete argument preparation and continuation
insertion, while retaining the prior narrow fusion as its incumbent. This adds
39 lines in production files. Both compilers pass all 24 focused revisions; the helper
suite passes all 92 tests. Exact UI/hot identities and outcomes are retained.
Hot creation/runtime bytes fall by 630/620 in gas and 585/575 in size, with no
artifact increases. Five Maple approve labels each save 12 gas. Nine labels per
mode rise locally by 1, 11 or 12 gas but remain below their sealed baselines;
aggregate call gas falls by 522/648. The four UI artifact increases are each one
byte in `static_frames.sol`, still 155--210 bytes below sealed; other UI runtime
bytes fall by 277/278 overall. These reviewed tradeoffs introduce no sealed debt.
Maple still owes 17/19 gas per approve label. Evidence: `complete-call-entry/`.

Three disassembly/custom-pipeline snapshots are updated after reviewing capture
selection and explicit physical control flow. The custom-pipeline FileCheck now
requires the jump target to exist later without requiring physical adjacency.
All seven focused UI revisions, two IR assembly roundtrips and 144 concrete
sealed/current deployment/dispatch replays pass. Both optimized modes save one
byte per artifact and three opcode gas on exercised zero-value paths. The
unoptimized/custom-pipeline size increases are explicitly retained in the review;
no optimized regression is hidden. Evidence: `dump-contract-review/`.

Outlining now computes physical stack-height prefixes once per block and looks
up each candidate start, replacing repeated prefix scans. Unknown effects end
the cache at the same point where the former scan declined the candidate. This
adds 14 lines in the production file. All 715 UI successes per mode and both
15-case/175-label hot reports retain exact bytecode and gas; 92 helpers pass.
Two sequential Seaport pairs preserve all 432 contracts exactly. Times are
132.95 -> 125.65 and 132.64 -> 126.69 seconds, a 5.0% reduction in paired median
time. Peak RSS rises 2.0--2.6%; this bounded cache trades memory for avoided work.
The earlier larger Option-cache prototype and its measurements remain separate.
Evidence: `outline-prefix-heights/compact/`; no pass was removed or reordered.

Literal construction now chooses a scalar four-form plan before allocating the
winning instruction sequence. Cost-only queries allocate no instructions;
candidate order, fork/budget rules and exact tie behavior remain unchanged.
The production helper grows 83 -> 127 lines; a pure differential helper test
checks all selected instructions and costs. All 92 helper tests pass, and all
715 UI successes per mode plus both 15-case/175-label hot reports remain exact.
Two sequential 432-contract Seaport pairs also retain identical bytecode:
137.52 -> 129.18 and 134.17 -> 124.91 seconds, a 6.5% median time reduction.
Peak RSS changes +0.2% and -0.3%, effectively flat in these two pairs. Evidence:
`immediate-cost-plan/`, including initial build failures and their corrections.

A bounded physical peephole now keeps incoming words while restoring distinct
fixed homes, replacing the immediately reversed reload run. Stores retain exact
order, addresses and values; the extra transient word requires a capacity proof,
and module observations block the rule. This adds 106 lines in production files.
All nine new UI cases, eight affected contract revisions and 408 paired runtime
calls across three forks pass, including 24 boundary calls. Guarded isolated
comparisons retain all 715 UI successes per mode with no increases: gas/size
creation and runtime totals fall by 216/73 bytes. Both 15-case hot reports remain
exact. The earlier Nitro opportunity estimate did not account for its observer
barrier; it is not a measured saving. Evidence: `memory-roundtrip/guarded/`.

Stack normalization can now sink a leading literal past a pure permutation
when its distinct symbolic identity finishes on top. The existing cycle solver
handles the original words, and full usage/cost gates preserve its incumbent.
This adds 57 lines in production files. Review caught an uncommitted PC
observation regression (12 -> 10); the final rule uses the module-wide observer
permission, and context-free schedule costing disables it. PC/GAS/public-label
replays now remain exact. Ten UI revisions, 11,250 stack/memory models and 52
encoded executions pass, including exact 1,024-word peaks. Matching 715 UI
successes per mode retain all outcomes with no artifact increases: runtime bytes
fall by 2,033/1,578 in gas/size. Both hot modes save 48 gas and 24 creation/runtime
bytes with no individual increases. Evidence: `leading-literal-permutation/guarded/`;
the superseded unguarded candidate and its failing replay remain preserved.

Verifier instruction names are now formatted only on diagnostic paths. The
change adds four production lines and preserves all 26 validation diagnostics
exactly, all 715 UI successes per mode, and both 15-case/175-label hot reports.
Two sequential Seaport pairs retain all 432 contracts byte-for-byte: 74.85 ->
68.60 and 74.61 -> 69.13 seconds, a 7.8% median reduction. Peak RSS varies
+4.8% and -1.5%; no consistent memory improvement is established. Evidence:
`lazy-verifier-diagnostics/`. No validation was disabled or weakened.

Call-entry selection now builds the original-plus-entry sequence once and
reuses its caller cost when there is no entry tail. Candidate ordering, exact
usage/cost gates and tie behavior remain unchanged. This adds three production
lines. All UI and hot bytecode, observations and gas are exact; independent
models cover 10,098 sequences and 6,732 equal-tail costs. One sequential
432-contract Seaport pair is 70.38 -> 68.63 seconds (-2.5%), with peak RSS
666,608 -> 634,124 KiB. This single pair is provisional timing evidence.
Evidence: `call-entry-cost-reuse/guarded/`.

Successor enumeration now borrows terminator targets and chains the conditional
false edge, eliminating the helper Vec without changing edge order or duplicate
counts. This adds one production line and a pure helper test. The frozen before
binary exactly matches retained reports; all 715 UI successes per mode and both
15-case/175-label hot comparisons preserve bytecode, observations and gas. All
432 Seaport contracts remain exact. One sequential pair is flat: 68.37 -> 68.61
seconds (+0.3%), with peak RSS 617,528 -> 654,476 KiB (+6.0%). No speedup or
memory reduction is claimed from this pair. Evidence: `borrowed-successors/`.

The refreshed workspace run has 1,352 passes, one failing UI aggregate and two
skips. Its UI lane has 10,875 passes and 62 snapshot differences: three reviewed
dump cases are resolved, while two immutable snapshots and the future-writer
IR snapshot now differ. No expectation was blessed in this run. Independent
review finds 17 snapshot-only differences and 45 with latent FileCheck failures;
those assertions require investigation. The frozen guarded compiler reaches
bounded symbolic agreement on the internal-call stack-return fixture in both
modes. Current raw backend scope is 11,532 Rust lines across 33 files, including
comments and tests: 23,106 fewer than the deleted raw scope.

The future-home writer custom-pipeline expectation now pins its directly
carried target and exact single-live-base restore sequence. All five revisions
pass, with 18 concrete current calls (including the custom pipeline) and eight
matching solc calls. Four extra solc probes hit its free-memory pointer or wrap
the address; those layout-dependent failures and traces are retained explicitly,
not counted as agreements. Original tracked call inputs are unchanged. Comment
edits preserve exact generated artifacts in both modes and the custom pipeline.
Evidence: `future-home-current-review/`. The refreshed Foundry lane also passes.

Four small ABI/termination snapshots now reflect explicit physical branches.
Their assertions retain selector, calldata-boundary, loop self-edge, return and
revert obligations. Four focused UI tests pass. The five-case review retains
513 concrete sealed/current/solc checks, eight bounded symbolic agreements and
five exact capture/assembly roundtrips. Only the four cases with no optimized
size/gas regression are updated; enum conversion remains unchanged with a real
five-byte debt in both modes. Its duplicated conditional suffix has been
identified for a separate profitability trial. Evidence: `abi-termination-review/`.

Protected writer results now move beneath each disjoint absolute-home chunk
with one SWAP instead of one per restored word. Relative address protocols and
all overlap cases retain the original order. Removing the unused context and
infallible result leaves a net 23 production lines. All 660 independent
stack/memory models and 531 compiled writer/call/recursion replay calls pass.
Matching 715 UI successes per mode save 51 creation/runtime bytes per mode with
no increases. Both hot reports preserve every outcome and ordered gas label;
Nitro creation/runtime shrink 121 bytes in each mode and measured call gas is
unchanged. No predicted static-opcode saving is counted as a hot-gas win.
Evidence: `restore-result-study/` and `writer-resident-tests/restore-result-after/`.

Tail merging now declines observed code addresses, unknown computed entry and
gas observations, including gas forwarded to calls or creation and observations
in a successor block. The fix adds 33 production lines; the extracted sibling
code-observation predicate preserves its previous policy. All 60 concrete
counterexample calls now retain all 30 paired observations, and all 24 focused
UI revisions pass. Canonicalized positive fixtures preserve exact no-pass bytes.
Gas-mode UI/hot output is unchanged. Size mode exposes correctness debt: UI
runtime/creation grow 2,846/3,486 bytes across 341 increasing artifacts; hot
runtime/creation grow 2,034/2,063 bytes across 19 increasing artifacts, while
matched call gas falls 292. These regressions remain acceptance blockers; unsafe
sharing is not retained to hide them. Evidence: `tail-merge-observers/`. A
separate terminal-body forwarded-gas counterexample remains under repair.

Conditional-tail costing now counts a conditional label push and JUMPI rather
than a single terminal byte (+2 production lines). Matching UI screens save
15 creation/runtime bytes in size mode across two cases, with no increases;
both hot reports remain exact. Six affected UI revisions pass. The tuple case
has 288 concrete four-compiler calls and bounded symbolic agreement before and
after in actual size mode. Its cold-path opcode increases remain below sealed,
and transaction gas is unchanged. A focused physical fixture has 48 exact
branch replays; explicit sharing adds 12 opcode gas there and is not claimed as
a gas win. Enum output remains unchanged because branch-target forwarding
happens later; the original enum hypothesis was incomplete. Evidence:
`conditional-tail-cost/`.

Terminal-body sharing now excludes forwarded gas as well as direct GAS reads.
One shared predicate keeps the tail-global and terminal-body checks consistent
while preserving their different scopes (+1 production line). The independent
CALL/CREATE counterexamples retain all 16 paired execution results after the
fix; all 13 terminal-dedup UI revisions pass. All 715 UI successes per mode and
both 15-case/175-label hot reports remain bytecode, observation and gas exact.
Evidence: `terminal-forwarded-guard/`.

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
