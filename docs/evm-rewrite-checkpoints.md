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


## Archived progress ledger — 2026-09-08

The following ledger is preserved verbatim from the former progress page.
Its successive status statements and counts describe historical checkpoints,
not the current checkout. See [current progress](evm-rewrite-progress.md)
for the accepted local state and remaining work. No earlier evidence,
baseline hash, rejected experiment or limitation has been removed.

Archived source SHA-256: `d6efc76fe536cb8100a3684132fa40fc156791323a37392785cb631303696c51`.

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


### Retained writer addresses accepted as a bounded milestone

Commit `8c52191f` retains an immediately produced, homed MSTORE address above
an already frozen stack prefix. The existing writer template must cover every
backup, preparation must emit no code, and the producer store must remain a
canonical absolute store. DUP1 plus SWAP1 replaces its later PUSH/MLOAD without
changing homes, protection, memory writes or local gas. The insertion preserves
the producer's source origin. The writer helper owns the check; lowering only
coordinates the existing preparation boundary. There is no new pass or search.

Final debug binary `db2fee52` has the same complete outputs as measured draft
`5a32ced0`; its only source refinement is Clippy's exact Boolean complement.
The final official workflow repeats both full/Size lanes from that final source.
All 24/15 ordered IDs, 175 ordered gas labels and 139 observations per compiler
match; call gas is unchanged in both modes. Nitro creation/runtime each shrink
65 bytes and deployment saves 14,061 gas. The Size runtime corpus is exact.
The identical-input UI screen has ten smaller Gas objects, creation/runtime
totals each down twelve bytes, no growth and all 2,470 Size objects exact.

The nine heavy projects preserve all 1,672 contracts and 3,344 objects: 271
objects shrink, none grow, creation decreases 91,549 bytes and runtime 71,304.
All 91 changed source maps, 26 links and 515 immutable sites are reviewed,
including packed control tables, embedded creation objects and the changed
padding/allocation bookkeeping in NavigatorDeployer. The review explicitly
accounts for generated unknown origins rather than discarding those records.
The original positive sealed debt remains 18,126,242 creation bytes across 526
objects and 14,982,693 runtime bytes across 533 objects. Router is 23,795 versus
sealed 9,822 runtime bytes; the 182-byte gain does not resolve its 13,973-byte debt.

Primary final compiler time is +0.993516%, peak RSS -0.087879%. Nonoverlapping
Solmate/Solarray slowdowns prompted a quiet reversed five-case repeat: time
-0.406457%, RSS +0.599691%, with Solarray still +1.485% in that repeat. Both runs
remain recorded; no speedup or blanket noise claim is made. The supplemental
run has identical compiler inputs but no reusable solc smoke-profile record,
so it establishes neither runtime comparison nor solc agreement. Its initial
mode-selection preflight failed before compiler execution and is preserved.

The milestone adds 77 physical lines in production files. The backend now has
45 files/16,521 physical lines versus 34,638 in the deleted scope: -18,117.
These counts include comments and inline tests; a strict production-SLOC
baseline is unavailable. Five new test files cover ordinary and frozen-prefix
activation, refusal controls, ordered surviving values and debug origins.
All 49 fresh focused calls pass, 21 paired gas comparisons are exact and 18
negative FileCheck mutations fail. The symbolic attempt is exit 2/incomplete
after its Forge timeout, not agreement. Three initial fixture integration
failures were corrected only in new files: ten ROOT header prefixes and a
redundant pretty-JSON flag. Original expectations were unchanged.

Workspace tests report 1,395 passes plus the failing UI aggregate. The final
pinned UI run has 11,658 passes, two original failures and 853 filtered revisions;
all six new revisions/tests pass. All 36 Foundry projects pass (772/765 tests),
matching gas and two reported sizes down 94 bytes. Clippy, formatting and typos
pass. Evidence is retained in `writer-operand-cache-workflow-20260907/` and
`writer-address-regression-proposal-20260907/`. The existing cold-fallthrough
and calldata-alias failures, original memory-contract defects and sealed
performance debt remain open. This is not whole-rewrite completion.


### Terminal tail-call transport candidate

The next candidate extends the existing scheduler entry bypass to bounded,
read-free tail-call closures ending in fixed-range reverts, including Size mode.
SSA arguments already exist in MIR; their static-frame stores arise at the
backend boundary. Each omitted ancestor interval must miss every descendant
revert range. Ordinary entries and reservations stay intact. This follows the
prior-art direction of preserving accessible values through stackification;
it introduces neither source-memory promotion nor an assembly-stream pass.

Frozen draft `4d6e5953` builds. The first compiled draft exposed a budget-cache
regression: a nearly exhausted parent poisoned a later leaf request, growing
its runtime by twelve bytes. Terminal leaves now spend no expansion budget;
their source-linear scans remain cached once, and the corrected example shrinks
four bytes. A 261-instruction leaf preserves its old bytecode exactly.

The 1,630-case UI size screen retains identical inputs, IDs and statuses, with
57 changed objects and no growth. Gas creation/runtime each shrink nine bytes;
Size creation/runtime shrink 500/442 bytes. All 222 focused calls match exact
status and payloads, with no measured gas or peak increase. Ancestor-overlap
stores remain, and eight observer controls are byteexact. These are provisional
checks: official hot-gas, heavy output, compiler-time, metadata, symbolic and
full test gates remain open. Original expectations have not changed. Evidence
is under `terminal-tail-entry-workflow-20260907/`,
`terminal-tail-closure-tests-20260907/` and `terminal-tail-budget-tests-20260907/`.


### Terminal tail-call transport accepted as a bounded milestone

Commits `3d6dc3c4` and `5661a94c` land the closure transport and its budget/cycle
regressions. The scheduler reuses its existing checked entry reconciliation;
a sparse per-artifact cache certifies descendant effects and fixed revert ranges.
Nonleaf expansion is bounded, terminal leaves are scanned once without consuming
that budget, and every omitted ancestor interval is checked independently.
The assembler, MIR, ordinary entries and frame reservations are unchanged.
This adds 89 physical production-file lines. The backend is now 45 files and
16,610 physical lines, versus 34,638 in the deleted scope: -18,028. Counts include
comments and inline tests; a strict production-SLOC baseline is unavailable.

Frozen `4d6e5953` is the measured candidate; the committed source differs only
in one verified rustfmt whitespace wrap. Both official lanes pass: all 24/15
ordered IDs, 175 gas labels and 139 observations per compiler agree, with exact
call gas, deployment gas and complete outputs. Four MIR helper-name bijections
account for the only changed text artifacts. Fresh complete fingerprints bridge
all nine heavy projects to their accepted raw outputs: all 1,672 contracts,
3,344 objects and metadata are exact, preserving original producer provenance.
The 1,630-case UI screen has 57 changed objects and no growth: Gas creation and
runtime each decrease nine bytes, Size decreases 500/442 bytes respectively.

ColdCall runtime improves 186->177 bytes in Gas and 200->176 in Size. Six abort
labels save 24 gas each in Gas and 31-55 in Size; nine other labels are unchanged.
All fifteen labels remain at or below sealed gas in both modes. Across 270 focused
calls, exact payload/status and measured peaks pass. Thirty-two paired debug
captures and six changed maps preserve known origins and function markers;
requesting metadata leaves bytecode unchanged. One symbolic attempt reports
bounded agreement for ordered arguments (64 paths/queries, depth 1024,
96 return bytes). Its actual candidate prefix is independently verified;
this is bounded evidence, not an unrestricted equivalence proof.

All 33 new UI revisions pass. Independent fixture checks reject 66 negative
FileCheck mutations. Final UI is 11,691 passed, two original failures,
853 filtered; no original test or expectation changed. Workspace has 1,395 other
passes and two skips. All 36 Foundry projects pass with exact IDs, gas and reported
sizes (772/765 tests). Clippy, formatting, typos and whitespace checks pass.
Primary compiler-time geometric mean is -4.894385%, RSS +0.830953%; factorial's median is
13.615->14.382 ms (+5.629%) with overlapping sample ranges. These measurements
establish neither causation nor per-case compiler-time dominance.

Evidence and final independent approval are under
`terminal-tail-entry-workflow-20260907/`, `terminal-tail-closure-tests-20260907/`
and `terminal-tail-budget-tests-20260907/`. The original cold-fallthrough and
calldata-alias failures remain, as do the memory-contract defects and unchanged
33,108,935 positive sealed size-debt bytes across 1,059 objects. The next isolated
fallthrough artifact reaches 171 bytes with lower gas, but needs a checked capacity
guard for a path whose peak rises 3->4 and a production metadata proof. It is
retained in `cold-success-carry-model-20260907/`, not installed. The goal remains
incomplete.


### Cold-success carry rejected after native corpus screen

The after-layout experiment reproduced the measured 176->171-byte Size runtime
and both success fallthroughs, but needed 320 helper lines plus five wiring lines.
The fresh 1,642-case corpus screen preserves every input, status and contract ID:
only ColdCall changes, saving five creation and five runtime bytes; all Gas
objects are exact. This benefit does not justify a dedicated layout-sensitive
matcher. The exact patch was reverted, with the frozen candidate and all evidence
retained under `cold-success-carry-workflow-20260907/` and the corresponding
proposal/tests directories. No original test or expectation changed.

Twenty native structural captures pass, including sixteen refusal controls.
Eight creation/runtime objects are unchanged by requesting debug output. Full
metadata and fresh runtime acceptance were deliberately not completed for this
rejected implementation; the earlier artifact traces remain separately identified.
The new official full/Size baseline is retained, with all 24/15 IDs and 175 gas
labels exact against the accepted milestone. Further work targets repeated
operand materialization in the existing scheduler, informed by the pinned solx,
Venom and Sonatina studies, rather than extending the rejected matcher.


### Operand materialization accepted as a bounded milestone

`b2b6db45` keeps a repeated immutable argument on the private stack through a
commutative producer and ordered consumer. One candidate in the existing operand
chooser reuses its complete gas/byte/peak checks; ordinary replay guards remain.
The change adds 77 physical production-file lines. The backend is 45 files and
16,687 physical lines, 17,951 fewer than the deleted scope, including comments
and inline tests; a strict production-SLOC baseline remains unavailable.

All 1,642 UI corpus IDs, statuses and 822 source hashes match. Thirty-two Gas
contracts shrink: 45 creation and 44 runtime bytes total; all Size objects are
exact. The nine-project audit accounts for 1,672 contracts and 3,344 objects:
four objects shrink by five creation and five runtime bytes total, with no
growth. Source maps and link/immutable tables remain exact. Both official
runtime lanes preserve their 24/15 IDs, 175 gas labels, 139 observations per
compiler, bytecode and deployment gas. Two MIR helper-name bijections per lane
are separately proved; complete raw outputs and original producers are retained.

All 222 focused calls preserve status, return data, gas, stack peak and memory
peak. One activated XOR/comparison differential reports bounded agreement, with
an exact executable-prefix bridge; this is neither unbounded proof nor a
counterexample replay. Native debug origins, relocated branches and metadata
request neutrality justify two precise snapshot updates. The new fixture covers
six producer classes and two refusals; twelve mutants and the accepted baseline
fail its activation checks. Final UI: 11,696 passed, two original failures,
853 filtered. Foundry: all 36 projects, 772/765 tests, IDs, gas and reported sizes
exact. Clippy, formatting, typos and whitespace checks pass. The earlier workspace
exit remains nonzero; its other 1,395 tests passed, with two skips.

Primary compiler-time geometric mean is -4.11935%, RSS -0.21409%, but 16 of
24 medians rose. The reversed eight-case run is +0.44575% time and -0.48048% RSS,
with four medians higher and four lower. Seaport remains slower in both separately
ordered single-sample comparisons (+10.10%, +2.27%). This is a measured size
improvement, not a demonstrated compiler speedup. All samples and missing-solc
reference warnings from the timing-only repeat are retained; it makes no runtime
or solc claim. Evidence is under `operand-materialization-workflow-20260907/`
and `repeated-argument-carry-tests-20260907/`. The original cold-fallthrough and
calldata-alias failures, memory-contract defects and broader sealed performance
debt remain open.


### Shared conditional helper rejected before installation

A smaller after-layout conditional-sharing model saves six Size bytes on the
cold-call fixture, but increases current path gas while staying within the sealed
labels. Its 135-line implementation proposal passed static safety review. A
1,257-contract Size bytecode census found the complete shape only in that fixture;
fourteen objects could not be fully decoded. This does not exclude shapes removed
by later passes, but the observed scope does not justify the helper. The proposal
and seventeen fixture drafts remain uncompiled; no production code or expectation
was installed. Evidence is in `conditional-tail-sharing-proposal-20260908/`.

The fresh baseline remains reusable: its compiler is byte-identical to the
accepted operand-materialization binary. All 1,642 existing UI IDs preserve their
objects and statuses, with two IDs added by the new regression test. Full/Size
workflow outputs, gas labels and metadata match the accepted baseline; four MIR
files differ only by proved helper-name bijections. Results are retained under
`conditional-tail-sharing-workflow-20260908/`.


### Shorter spill restoration accepted as a bounded milestone

Commit `eba00ed7` makes the existing selected-home writer restore its two saved words in stack
order. Three SWAPs replace seven after both originals have been loaded; disjoint
or identical aligned homes make the reversed restore order safe. No pass or
analysis is added. The backend loses 11 physical lines and now has 16,676 across
45 files, 17,962 fewer than the deleted scope, including comments and inline tests.

The identical-input UI screen retains 1,644 IDs and 5,028 objects: 22 Gas objects
shrink by 68 creation and 68 runtime bytes total; Size is byte-exact. Across nine
archived projects, all 1,672 contracts remain: 302 objects shrink by 326,229
creation and 262,395 runtime bytes, with no growth. Router falls from 23,795 to
23,247 runtime bytes. The complete metadata review accounts for changed source
maps, embedded-object sizes, chooser changes and relocated references. No
diagnostic threshold is crossed. Official full/Size runs preserve their 24/15
IDs and 175 gas labels per mode. Nitro loses 212 bytes in each object and 45,857
deployment gas; measured call gas is unchanged.

The memory model covers 3,184 cases. All 124 focused candidate/baseline call pairs
preserve status, payload and peaks; seven Gas calls save 12 gas. Another 56 calls
execute actual extracted bitmap fragments: 26 successful pairs save 12 gas with
identical returned memory and two extreme-address failure pairs match. Thirty
older sealed bitmap checksum mismatches remain explicitly separated. The fresh
activated symbolic differential times out after 35 seconds and is incomplete;
its executable-prefix bridge is exact, but it supplies no agreement proof.

Only two restore CHECK groups and their reviewed IR/debug snapshots change.
Runtime directives remain intact, eight negative CHECK mutants are rejected, and final
UI is 11,696 passed, two original failures, 853 filtered. The workspace's other
1,395 tests pass with two skips. All 36 Foundry projects pass with unchanged
gas and two reported sizes each down 292 bytes. Clippy, formatting, typos and
whitespace checks pass.

Primary compiler-time geometric mean is +0.8285%, RSS -1.0436%. A reversed
three-case repeat is -0.7803% time and +0.1275% RSS. The original v4-core +52.8%
single-sample result does not reproduce in its repeated median, but a fourth
candidate sample again crosses the cutoff; Aave remains +1.97% with overlapping
ranges. These measurements establish no compiler speedup. Evidence remains in
`writer-restore-order-workflow-20260908/` and
`writer-restore-order-independent-20260908/`. The two original UI failures,
arbitrary-memory contract defect, and 32,520,301 positive sealed size-debt bytes
across 1,059 objects remain open. The whole rewrite is incomplete.


### Prior-art follow-up and ordinary sharing screen

The MUL recipe proposal remains unimplemented: no complete eligible home bank
was found in the current Router or retained writer fixtures. The solx, Venom
and Sonatina review preserves the distinction between moving one-use work and
duplicating expressions, including shared-input costs and observer constraints.

The handoff compares final gas and size against sealed `9cb036c`. Earlier reports
that rejected sharing solely for losing intermediate gas gains imposed an extra
constraint; `global-alias-policy-review-20260908/` explicitly corrects it without
overwriting the historical evidence. Current Gas already reuses the calldata
value within each alias-test arithmetic arm. Its remaining runtime-size debt is
57 bytes in Gas and 26 in Size.

A fresh debug build is byte-identical to accepted `9ed4da3c`. The default and
explicit early TerminalDedup/TailMerge screens retain all 1,644 IDs, 823 source
hashes and 5,028 objects, with identical statuses and contract sets. Sharing
shrinks 1,094 objects, grows 40 and changes two at equal length; Size is exact.
Gas aggregate creation/runtime sizes fall by 14,076/14,018 bytes, and the alias
runtime falls from 210 to 184 bytes, still 31 above sealed.

Of the 40 growing objects, 16 remain within sealed size, four introduce sealed
debt, eight worsen existing debt and 12 cannot be strictly joined because the
source changed or the original ID is absent. The unchanged packed-static-hash
fixture grows from 115 to 144 runtime bytes against sealed 118: early suffix
sharing prevents complete-wrapper deduplication and duplicates ABI validation.
This rejects the broad chain on actual sealed size regressions. No production
pipeline or expectation changes, and no runtime-equivalence or timing claim,
follow from this screen. Raw outputs, exact pipelines, joins and independent
review remain in `sharing-budget-workflow-20260908/`.


### Validated scalar reuse: first draft rejected

A 22-line net MIR change reused raw words already read by scalar ABI validators,
keeping every raw load and failure branch unchanged. It built successfully but
failed the identical-input screen: 317 objects shrink, 95 grow and eight change
at equal length across the same 1,644 IDs. Of the growing objects, 33 worsen
sealed debt, six introduce new debt, 36 remain within sealed size and 20 lack
an exact sealed source/ID join. The alias fixture saves only two bytes per mode.

Replacing typed arguments before return encoding loses its existing canonical input
proof. The unchanged scalar-validation fixture gains redundant cleanup
and an enum panic path, growing 53 bytes in Gas and 43 in Size. The draft was
reversed using its retained patch; source and rebuilt binary exactly match
accepted `9ed4da3c`. No expectations changed, and the proposed 30-call fixture
remains unexecuted. Evidence is in `validated-scalar-reuse-workflow-20260908/`;
a possible return-encoding order correction remains a separate investigation.

The ordering follow-up is held without a build. Fixed-bytes indexing has a
separate representation split: validated `bytes1`/`bytes7` become stack-fed,
while unvalidated `bytes32` remains a lazy argument. The formerly shared
BYTE/shift/mask/return body is duplicated, adding 22 Gas bytes. Encoding returns
earlier can preserve canonicality facts but has no direct mechanism to restore
this sharing; a coincidental layout improvement is not ruled out or claimed.
No eager unvalidated loads or type-specific exception is proposed.


### Live segments: three residency screens rejected

Per-block live segments remove artificial occupancy in unrelated blocks while
retaining the existing within-block endpoints, global ranking and home assignment
algorithm. Ordinary home addresses can still change with the selected residents.
The first draft used these segments for both ordinary and Phi selection. The
identical-input UI screen preserves all 1,644 IDs, 823 source hashes and 5,028
objects, with unchanged statuses and contract sets. Eighteen objects shrink,
four grow and two change at equal length. Gas creation/runtime totals each
fall by 127 bytes; Size totals each fall by 181 bytes. Router runtime falls
from 23,247 to 22,379 bytes, but nested calldata storage grows from 5,784 to
5,947 and parallel spill copies from 586 to 588. These regressions reject it.

Native diagnostics reproduce the exact baseline and candidate objects. In
parallel spill copies, seven additional ordinary residents displace nine Phi
residents; both emission trials pass. In nested storage, seven ordinary gains
accompany a larger Phi proposal whose writer-protection cost fails the existing
guard. Rejecting that complete proposal loses 28 previously accepted Phi
retirements. Neither regression is explained by lost ordinary residents or
unexpected source changes. A fresh sealed-binary run on the exact current
parallel-copy source gives 481 Gas runtime bytes, so its matched supplementary
size debt worsens from 105 to 107 bytes. The historical unmatched row remains
unchanged.

Two focused follow-ups isolate the phases. Ordinary-only segments produce
parallel/storage/Router Gas runtime sizes of 588/5,734/23,435 bytes; Phi-only
segments produce 524/5,997/23,918. Each retains a growing case and is rejected.
The Phi-only proposal leaves ordinary homes stable, showing that more precise
occupancy alone does not establish profitable edge and writer transport.

All three source patches, frozen binaries, exact inputs and raw outputs remain
in `resident-live-segments-workflow-20260908/`, with the independent native-set
review alongside them. The saved rewritten source was restored and the rebuilt
compiler is byte-identical to accepted `9ed4da3c`. No test or expectation was
changed. These rejected screens make no runtime-equivalence, hot-gas or compiler
timing claim; they did not rerun the full acceptance suite. A larger retry patch
remains held and uncompiled because retrying failed proposals cannot repair an
accepted but more expensive allocation. The next design question is joint
ordinary/Phi selection with edge and writer costs, not another occupancy policy
layer. The existing whole-rewrite correctness and size debts remain open.


### Joint residence: runtime and project gates reject draft

The next prototype replaced the competing optional pools in eligible static Gas
owners with reachable Phis. It retained short local exemptions, allocated homes,
then selected ordinary and Phi values together using per-block live segments and
cyclic-use priority. A shared initialized/live-home collector fed an admission
filter that checked complete MSTORE protection banks before each removal. The
existing physical emission and final writer guards remained authoritative.

The native bank diagnostic explains why individual savings are insufficient:
all 32 subsets of five proposed removals were priced by the existing chooser.
Banks with 14, 13 or 12 homes cost 55 bytes/130 gas; dropping to 11 homes costs
83–85 bytes/135 gas. Each singleton removal looks free, but every third removal
crosses the template threshold. Shared-address ownership and initialization were
retained, and bank-cost updates committed only when all affected writers passed.

The full UI screen preserves 1,644 IDs, 823 source hashes and 5,028 objects.
Twelve Gas objects shrink by 615 creation and 615 runtime bytes total; Size is
byte-exact. Focused Parallel/Storage/Router runtime sizes fall from
586/5,784/23,247 to 524/5,634/22,795 bytes. All three focused MIR bodies remain
exact. These positive results did not establish acceptance.

The prescribed full workflow preserves 24 cases and 175 ordered gas labels.
Six Nitro calls each add 22 gas versus the accepted compiler while remaining
232 gas below the sealed baseline. More importantly, 376 fresh-deployment
focused calls cover 47 original labels, four compiler legs and both modes.
Every oracle passes, but 16 Gas labels grow versus the accepted compiler;
eight also exceed sealed gas. Parallel's zero-round path grows 982→1,046 gas
against sealed 809; the equivalent Cycles path grows 550→600 against sealed 581.
Two nested-memory calls grow by 308/393 gas and worsen sealed debt. Size gas is
unchanged. Return/status agreement does not waive these runtime regressions.

Across all nine archived projects, 1,672 contracts and 3,344 objects remain.
Of 193 changed objects, 163 shrink and 30 grow. Every growing object worsens
existing sealed debt, despite aggregate creation/runtime reductions of
178,104/80,056 bytes. ReadOnlyOrderValidator runtime grows 9,627→9,876 against
sealed 5,690; MockEntryPoint grows 5,235→5,238 against sealed 2,370. Metadata
changes were not approved after these actual size failures.

Trace attribution shows that the zero-round regressions mainly add retained-value
SWAPs. The nested-memory calls execute four/six more bitmap protection templates;
the gate's broader initial home allocation is not the accepted allocation's cost
reference. Quiet compiler-time geometric mean is -1.52%, RSS +0.61%, with differing
sample counts and per-case regressions; no speedup is claimed. The failed draft was
preserved under `joint-residence-workflow-20260908/` and the saved rewritten
source restored. Rebuilding matches accepted `9ed4da3c` byte-for-byte. No test or
expectation changed. Full workspace/UI-oracle and symbolic gates were not rerun
for this rejected draft. A separate investigation will address last-use operand
preparation refusing missing literals before their ordinary materialization.


### Materialized dead operands: bounded milestone

Commits `aeeed485` and `6540ccc4` add a final Gas operand-order trial and a
reduced entry-order regression. Missing operands are materialized in the existing
order before last-use scheduling. The unified block chooser retains the old
unary, multi-operand, argument-carry and entry-order choices before trying it.
An existing entry winner returns immediately, preserving its successor stack;
otherwise the new trial must preserve the actual old winner's stack prefix.
The assembler, IR passes, cost model and spill allocator are unchanged.

Three earlier drafts were rejected or superseded. Changing existing operand
policies lost previous winners; a separate policy fixed the UI regressions but
still displaced P256's later entry choice. Captured block bodies show why local
cost improvements were insufficient: downstream stack normalization and CSE
turned the new ordinary-path schedule into 32 extra bytes. The final chooser
restores that winner without another entry replay or simulated pass pipeline.
The reduced fixture rejects the failed draft and deliberate swap mutations.

The original UI size screen preserves 1,644 IDs, 823 source hashes and 5,028
objects. There are 453 shrinking objects and no growth: Gas creation/runtime
fall by 2,296/2,202 bytes. Size bytecode is exact. Across nine archived projects,
1,672 contracts and 3,344 objects, 1,009 objects shrink and two retain their
length; none grows. Creation/runtime totals fall by 49,869/47,767 bytes. Both
equal-length MerkleTreeMock changes are equivalent operand permutations.

The official workflow preserves all 24 cases and 175 ordered hot-gas labels;
all 15 runtime cases match the exact-input solc reference. Two Aave labels save
6 gas each and six Governor labels save 12 each. Other labels are unchanged.
The Size run preserves all 15 cases, 175 labels, bytecode and execution costs.
Five-sample compiler-time geometric mean is **+2.94%**, RSS **+0.66%**. Governor,
Forge, PRB and Solmate have disjoint slower sample ranges; compilation is a
remaining regression, not a speedup or a dismissed noise result.

Final workspace verification has 1,395 other tests passing and two skipped;
the UI suite has 11,701 passes and the two original failures. All 36 Foundry
projects pass (772 compiler tests, 765 solc tests), with no increased gas or
contract size against the previous accepted compiler. Clippy, formatting,
typos and diff checks pass. Five instruction snapshots changed after independent
source, MIR, FileCheck and stack/effect review; their existing checks and runtime
expectations remain intact. Neither original failing expectation was blessed.

Focused evidence includes 216 fresh deployments/calls across four compiler legs
and both modes, with every oracle passing. A fresh symbolic last-word calldata
case obtains bounded agreement with solc over the recorded lengths and budget.
Those executions used draft3; complete draft4 bytecode/input bridges justify
reuse without claiming a rerun. The new fixture has 48 fresh four-leg calls and
eight additional sealed calls. Its Gas size remains 960 bytes above sealed in
each object, although execution saves 1,071/1,192 gas; Size saves 162 bytes and
991/1,254 gas. These new IDs remain separate from the original corpus ledger.

Independent metadata review covers 553 changed source maps and 69 relocated
reference tables. Known consuming-opcode ownership and ordered call events are
preserved; the one LT-to-GT change reverses operands with equivalent stack and
memory effects. Inserted scheduling checkpoints remain explicitly unknown.
Link and immutable identities, payloads, widths and source ownership are retained.

Frozen compiler `3133846e`, exact producer/input pins, rejected drafts, full
measurements and review manifests are retained in
`target/codegen-bench/evm-rewrite-candidate/dead-operand-materialization-workflow-20260908/`.
The reduced fixture's sealed evidence is in the adjacent
`entry-winner-regression-proposal-20260908/`. This change adds 29 backend physical
lines: 16,705 across 45 files, 17,933 fewer than the deletion inventory. This is
physical line counting, not a strict production-SLOC comparison.

The whole rewrite remains incomplete. The original cold-fallthrough and calldata
alias UI failures, raw-memory/spill ownership defect, and 32,427,696 bytes of
positive sealed size debt across 1,050 archived objects remain open. The current
compile-time increase also needs attention. No push was attempted for these
commits because the earlier automatic approval rejection remains unresolved.


### Duplicate materialization replay gate

Commit `e9f3b5cf` adds a necessary-condition scan to the new operand trial.
Without a multi-operand opcode containing a nonresident input, that trial must
repeat the existing DeadOperands body or fail under the same loading rules.
Missing unary operands become shallow and take the same canonical fallback;
missing resident operands cannot load in Gas mode. Independent review also
checks the original Phi policy, argument-carry candidate and old entry winner.
This removes redundant work inside the output-improving milestone.

The corrected UI screen preserves all 1,644 original IDs plus two new fixture
IDs, 824 source hashes and 5,032 complete objects byte-for-byte. An initial run
used a different experimental pass ordering; the command join rejected it.
Its raw results remain separate and are not evidence for this candidate.
Full and Size official runs preserve all cases, 175 ordered gas labels each,
139 observations each and complete output fingerprints. Exact heavy fingerprints
bridge all 3,344 objects and metadata to the previous reviewed captures, retaining
the actual original producers. Two MIR dumps differ only by proved bijective
literal-helper renaming.

Quiet full-run compiler-time geometric mean is -0.70% against the ungated
milestone, still +2.22% against retained `9ed4da3c`; RSS is -0.13%/+0.53%.
Solarray's five-sample range is disjoint and 1.43% slower. These are measured
comparisons, not a universal or causal speedup claim. The concurrent one-sample
Size run supplies output evidence only. Workspace results remain 11,701 UI
passes, two original UI failures, 1,395 other passes and two skips. Clippy,
formatting and typos pass; no tests or expectations changed. Previous focused,
Foundry and symbolic execution evidence is retained rather than relabeled as
fresh gate executions.

Frozen binary `0e61860e`, commands, corrected and refused screens, full results,
source pins and independent review are retained under
`target/codegen-bench/evm-rewrite-candidate/materialization-trial-cost-20260908/`.
The guard adds ten physical backend lines: 16,715 total, 17,923 fewer than the
deletion inventory. Original correctness, UI and sealed size debts are unchanged.

### Repeated negated literals on the physical stack

The Gas-only compact-pushes pass now retains one repeated PUSH/NOT result per
block when full-body transport reduces bytes without increasing static gas.
Selection is deterministic; existing physical stack facts enforce legacy DUP/
SWAP reach and the 1,024-word limit. MIR scheduling, memory homes and the primitive
assembler are unchanged. The design follows the constant-reuse opportunity in
our pinned solx, Venom and Sonatina research, with measured local costs instead
of an upstream width threshold.

Against frozen `0e61860e`, the matched UI screen has 1,646 IDs and 5,032 objects:
eight shrink, none grow, and Gas creation/runtime totals each fall 977 bytes.
Size objects are exact. The entry-order witness saves 940 bytes per object and
15/33 gas on its ordinary/doubling calls; it still exceeds the sealed Gas object
by 20 bytes. Its two adjacency checks now allow the new transport while retaining
the old winner and rejecting the known scheduling mutants. Original cold-call
and calldata-alias tests are untouched.

The official full workflow retains 24 IDs, 175 ordered gas labels and 139
observations. All runtime cases match solc; 19 Aave labels save 12 gas and two
save 24, with no increases. Aave creation shrinks eight bytes. The nine heavy
projects retain all 1,672 contracts and 3,344 objects: eight shrink two bytes,
none grow. The Size supplement is byte/gas exact. Compiler-time geometric mean
is +1.16%, RSS -0.61%; several per-case sample ranges are disjoint. These output
wins carry a measured compilation cost, not a compilation-speed claim.

All 36 Foundry projects pass (772 compiler tests, 765 solc tests), with reported
sizes and gas unchanged. The three additionally changed UI sources pass 530
fresh focused calls; pinned solc rejects one unchanged packed-array source,
whose calls still run against current, candidate and sealed binaries. A new
narrow-cache control saves seven bytes per object and 14 gas, with nine fresh
calls and bounded solsymdiff agreement. Earlier wide-fixture executions transfer
only through exact bytecode bridges and retain their actual producer hashes.

Final workspace results are 11,714 UI passes and the two original failures,
1,395 other passes and two skips. All newly installed matrix/runtime tests pass;
Clippy, formatting, typos and whitespace checks pass.

Actual metadata/debug captures are byte-neutral. Generated transport has unknown
source ownership; one downstream SWAP/AND cleanup also drops an AND checkpoint
under the existing metadata merge rule. Full invocation/source-map review and
all raw inputs, failures, producers and comparisons are retained under
`target/codegen-bench/evm-rewrite-candidate/literal-cache-workflow-20260908/`.
The helper adds 194 physical backend lines: 16,909 in 46 files, 17,729 fewer than
the deletion inventory. This is physical LOC, not a strict production-SLOC count.
The two original UI failures, raw-memory/spill ownership defect and remaining
32,427,688 positive heavy bytes versus sealed remain open.


## Profitable Size tail groups: 2026-09-08

Commits `96c07690` and `d9fa3bd8` enlarge an existing ordinary Size tail pair
only when the body covers two PUSH3/JUMP transfers and a marker. Static Jump
receives zero terminal credit; JumpI receives its minimum fork-specific width.
Each additional member independently passes split, destination and stack checks.
The initial body-eight groups grew seven UI objects through lost fallthroughs,
duplicated prefixes and four widened references. The stricter reserve restores
all seven objects while retaining 56 shrinking objects and -559 creation/runtime
bytes each. No Gas UI or full-workflow output changes.

Frozen final `fca09c9a` follows the Clippy-equivalent usize comparison change
from `>= 1 + 2 * 5` to `> 2 * 5`; prior `426d54da` and the separate test-build
`97480f82` remain distinct producers. Full 24/175/139 output joins include all
nine heavy projects; Size 15/175/139 saves 11 bytes in ERC20Mock creation/runtime
and 2,376 deployment gas with unchanged call gas. Quiet full time is -1.06%,
RSS +0.30%; no causal speedup or concurrent Size timing claim is made.

The exact alias objects transfer the earlier 126 fresh calls: Size179→166,
three account-3 labels +11 gas against the prior candidate, still below sealed;
Gas remains exact. The larger fixture has 24 fresh successful calls and unchanged
gas at Size95→79. A fresh bounded Size solsymdiff checks typed address/uint256
inputs against pinned solc. Four revisioned fixtures add eight UI cases.

Final workspace has 11,724 UI passes and only the original alias assertion fails;
1,395 other tests pass and two are skipped. Foundry36/772/765 passes with reported
sizes/gas exact, using a normal debug build from final sources; no separate
immediate pre/post Foundry binary hash was recorded. Clippy/fmt/typos pass.
Ninety-two actual metadata/plain captures cover 64 programs and 58 changed
objects. All 75,054 map projections match emitted origins; shared/removed events
and the library deployment-address relocation are explicitly reviewed. The
third-member forwarding fixture records inherited event loss, not a new promise
of event retention. All nodes and native jobs are reaped.

Evidence is retained under `size-long-tail-group-workflow-20260908/`, including
both rejected and accepted patches, native trace groups, exact-source cold
comment/directive bridges, actual producers and independent review. Backend
physical LOC rises by22 to16,931; sealed heavy debt and raw-memory corruption
remain open. The separately committed cold-call expectation migration is a
reviewed test contract change with30 passing runtime checks, not a new layout
optimization. Every prior progress paragraph is preserved above verbatim.


## September 8: eager consumer scheduling

Accepted production `5f70b2f2`, tests `5b06f7ab`, and independent writer-fixture
policy migration `9e27b108`. Frozen normal debug compiler is
`solar-eager-consumer-final`, SHA256
`2b032ba7e02ff1acbb48c57d15a371ad230ee5ab7859f8cab1048a5eda57e7ae`.
Evidence is under `target/codegen-bench/evm-rewrite-candidate/eager-consumer-workflow-20260908/`.

The existing late MIR pass consumes two or more distinct dying SSA results
before ordinary writes when a block's local live-result count exceeds target
reach. Reads, writes and observation barriers keep their order. The contraction
runs under None too; the previous segment scheduler stays optional. It reuses
scratch bitsets and block buffers, introduces no spill permission, and leaves
physical stack checks to lowering. It adds 188 physical Rust lines outside the
backend. The backend remains 16,931 lines; these counts include embedded tests
and are not strict production SLOC.

The unrestricted draft had eight growing Gas entries and fifteen growing Size
entries. Restricting motion to actual contractions removed all but one growth;
the pressure gate restored that call-return case to baseline bytes. Every trial,
failed expectation and exact emitted-object comparison remains in the archive.
On the original unchanged corpus, Gas creation/runtime totals fall 1,270/1,268
bytes and Size falls 1,258/1,256, with no growth. After adding the two original
readback sources and the reviewed writer migration, all 1,652 IDs and 827
source hashes match the explicitly bridged frozen baseline; totals fall
1,458/1,455 Gas bytes and 1,943/1,940 Size bytes, with no growth.

All 24 original readback calls pass across None/Gas/Size, including the captured
storage checksum. The standard matrix executes 32 calls and both new MIR tests
pass. Workspace: 11,734 UI passes, one original alias assertion failure, 1,395
other passes, two skips. All 36 Foundry projects retain 772/765 passing tests
and identical reported gas/sizes. The first passing Foundry run omitted reports;
the retained reported rerun corrects that evidence gap. The writer migration
keeps all five original oracles and actual live-home overlap while preventing
its scratch region from aliasing the raw source write. Only that source's MIR
snapshot and the newly required None timing line were updated.

Full Gas retains all 24 IDs, 175 labels, 139 observations and complete outputs.
The Size supplement retains 15/175/139 and exact outputs/gas. All nine heavy
projects retain all 3,344 objects and requested metadata; positive sealed size
debt remains 32,427,688 bytes across 1,050 objects. The quiet full compile-time
geometric mean rises 0.543%; a reversed five-sample repeat confirms OpenZeppelin
+1.64% and Morpho +3.11%, with disjoint sample ranges. These costs remain real;
a smaller sole-user analysis is the next measured change.

Independent metadata review checks 88 captures and 14,600 source-map entries;
all plain/debug objects and ABI joins match. One duplicated empty-revert owner
has a corresponding extra real checkpoint. Two symbolic arbitrary-input runs
return unknown at 2/10-second solver limits; neither is agreement. A separate
shared-capture extension passes 36 calls plus 244 address-sweep calls with
baseline-identical objects, establishing only bounded refusal coverage. General
memory ownership and the overall rewrite remain open.


## September 8: compact consumer tracking

Accepted `ce593973` replaces the sole-consumer enum with `Option<InstId>` and
builds the shared-definition bitset during active-use traversal. The existing
generation sentinel distinguishes first use from shared or terminator-only use.
It removes a final instruction scan and five physical Rust lines; consumer
entries shrink from eight to four bytes. Scheduling policy and layer boundaries
remain unchanged. Frozen debug compiler `solar-sole-user-draft1` has SHA256
`5bc3e1f9384b38c5a50b9ec8155e253cfe92eb0d5556b783137607b4586c6f21`.

Evidence is under `target/codegen-bench/evm-rewrite-candidate/sole-user-workflow-20260908/`.
All 1,652 UI IDs, 827 source hashes and 5,044 complete objects match the accepted
eager-consumer baseline. Full Gas joins all 24 IDs, 175 ordered labels and 139
observations per compiler; Size joins 15/175/139. Complete output fingerprints,
physical artifacts and gas match. Two MIR artifacts differ only by verified
literal-helper name bijections. One UI stderr reorders ten identical warning
blocks. No expectation changed. Workspace retains 11,734 UI passes, the original
alias assertion failure, 1,395 other passes and two skips. Formatting, Clippy
and diff checks pass. All 146 production source pins and the frozen binary match.

The full observed compiler-time geometric mean is +4.2173%, RSS +0.0991%.
Another task's profiling/Foundry workload overlapped the host; the process
snapshot supersedes the runner's initial quiet-run description. Our own agents
were quiet. Candidate `--repeat-long-compiles` retains five samples for every
case, whereas Seaport, OpenZeppelin and Solady baselines stopped after one.
Individual deltas and samples are retained, but neither a speedup nor a causal
slowdown is established. Concurrent Size timing is unused. The independent
report and acceptance manifest record this bounded simplification; all original
alias, sealed size/gas and general memory-ownership acceptance debts remain open.


## September 8: reject validated-word cleanup growth

The combined validated-word/owner-equality trial is rejected. Frozen compiler
`solar-alias-owner-draft1` is SHA256
`c575adf1475672890e60d556b8438a958677ace1cd4f39f3ac1f1d41ee069d28`.
Its unchanged UI corpus joins 1,652 IDs, 827 source hashes and 5,044 objects;
Gas creation/runtime totals shrink 39/37 bytes and Size shrinks 54/52, with no
individual UI growth. Original alias Size reaches 152 bytes versus sealed 153,
and 126 fresh alias calls match their oracles without exceeding sealed gas.

The official full benchmark and Size supplement expose the missing coverage:
Fractional, Maple ERC20 and Governor grow by 35, 34 and 17 bytes respectively
in both creation and runtime, in both modes. Each worsens exact-input sealed
debt. Full joins all 24 IDs, 175 ordered labels and 139 observations; Size joins
15/175/139. Execution gas is unchanged. Eight heavy project output fingerprints
change; per-object heavy captures were not collected for this rejected trial.
Raw compile time is +0.73%, RSS -0.08%, with five samples on both legs; recorded
baseline host contention prevents a causal timing claim.

Reusing a raw validation word changes the physical stack at body entry. Maple
loses compact address-mask recipes and gains edge cleanup; Fractional's increase
comes from its embedded NFTShare creation object. The narrow MIR patch is
reversed exactly and its proposed tests remain uninstalled. No original source,
expectation or oracle changed. The isolated actual-owner equality change is also
rejected: alias Size overflow costs 218 gas versus sealed 215, and runtime is
154 bytes versus sealed 153. Its four new UI revisions pass, but the six new
uncommitted fixture files were removed by verified installation hashes; they
remain in the evidence archive. The broad forwarding-chain trial is
also rejected: it grew AbiFixedArray by 16 bytes and basic RunCall by one, and
exceeded sealed alias overflow gas. Equal-pair normalization was deferred after
zero real corpus changes. All binaries, failed checks and comparisons remain in
`target/codegen-bench/evm-rewrite-candidate/alias-owner-workflow-20260908/`,
`tail-chain-workflow-20260908/` and `tail-normalization-workflow-20260908/`.

The independent heavy-home investigation identifies a separate cost-model bug:
SuggestedActionHelper rejects Phi-home retirement when one writer bank shrinks
from twelve to eleven homes. Its remaining bitmap can still cost 55 bytes and
130 gas, but a hard twelve-home cutoff selects 89 bytes and 135 gas instead.
The proposed nine-home contiguous / eleven-home bitmap floors retain exact
profitability, stack-capacity and ownership guards. Static review and pure
helper test cases are ready; no production change or native improvement is
claimed yet. Pinned solx, Venom and Sonatina evidence and explicit applicability
limits are retained in `writer-profitable-bank-floor-proposal-20260908/`.


## Gas-first Phi writer milestone — September 8

`e34bb6e6` retains the twelve-home contiguous floor, admits profitable eleven-home
bitmaps, and orders actual Phi writer protection by gas then bytes in Gas mode.
The broad threshold-only and bitmap-only trials remain rejected: nested calldata
grew by 150 bytes and 443 gas; separate heavy objects also grew. A native trace
showed the byte-first veto rejecting 75 bytes / 123 gas in favor of 55 bytes /
130 gas. The final rule restores the accepted nested output and gas without
changing ownership, capacity, actual lowering or checkpoint rollback contracts.

The final candidate is frozen as `solar-writer-bank-gas-first-draft1`, SHA256
`3dd1ec58737d0a49ce0d5fb2037da56ed49fa61218e7d3cd3d7bbdce0367ffd2`.
Evidence is under
`target/codegen-bench/evm-rewrite-candidate/writer-bank-floor-workflow-20260908/gas-first/`.
All 146 production source pins remain stable. Nine heavy projects preserve all
1,672 contracts and 3,344 objects: 277 shrink, none grow, creation saves 329,454
bytes and runtime 266,721. Embedded child amplification is included. Positive
sealed debt falls to 31,831,619 bytes; this is not net corpus growth.

The official full workflow preserves 24 IDs, 175 ordered gas labels and 139
observations per compiler. All execution gas is exact; Nitro creation/runtime
save 167 bytes each. The Size supplement preserves 15 IDs and the same label and
observation counts with complete outputs exact. The original UI screen retains
1,652 IDs, 827 hashes and 5,044 objects; creation/runtime each save 205 Gas bytes
with no growth and exact Size output. The explicitly composed new-fixture
extension has 1,654 IDs, 828 hashes and 5,048 objects, preserving every original
input. All 112 changed source maps and 17 relocated reference tables pass the
bounded independent review; removed optimizer checkpoints remain documented.

Foundry retains all 36 project/configuration IDs and 1,537 test records with
exact statuses and gas. Two DSTestPlus objects save 128 bytes each. A reduced
symbolic run reports bounded agreement over nine paths and fourteen queries;
its exact executable prefix matches both accepted and final compilers and
differs from the rejected threshold-only compiler. No counterexample required
replay; this is not unbounded equivalence or full memory-ownership proof.

`38289f90` adds an independent reduced UI/runtime fixture. Its physical check
passes with accepted/current compilers and fails with the rejected compiler.
The original nested source and its three oracles remain unchanged. Actual full
workspace results are 11,739 UI passes, the same original alias failure, 1,395
other passes and two skips. The rebuilt workspace executable has SHA256
`bb34783e85b4079f73615e04a3a1f8393d1dbd2a3fa2b879520166273a42e523`;
its unchanged production source pins and distinct producer identity are retained
in `workspace-completion.json`. Clippy, formatting, typos and diff checks pass.

Raw full compiler geometric mean is +4.18% and RSS -0.59%. Baseline contention
and a measured 54.26-second candidate parser overlap remain recorded; no causal
speed claim follows. Backend counts are 16,944 physical lines, 15,639 excluding
trailing test modules. The retained count-only baseline permits a conditional
13,367 production-section-line reduction but lacks a sealed revision/hash link.
Comments are included; this is not strict SLOC. General memory ownership, alias
size/assertion debt and whole-corpus sealed regressions remain open.

A fresh fetch found main `becd2143` beyond the last merged `6059f0c0`, including
solx benchmark support and MIR/backend file reorganization. Only path metadata,
retained-layer changes and benchmark infrastructure were inspected. Its backend
implementation has not been read or integrated. The next merge must preserve
the rewritten backend and resolve retained interfaces independently. Remote PR
1388 remains draft at `2d5f077f`; its old failing CI is not this local milestone.


## Retained-layer main merge — September 8

`e40b84f0` merges main `becd2143` using an explicit retained-layer three-way
application. No incoming backend body was read or materialized. All 47 backend
files preserve their implementations after import-path normalization; all 146
premerge codegen files have current counterparts. Eager scheduling, consumer
tracking and memory DSE survive unchanged. All 4,172 existing test files remain
present. The new call-summary cache and debug APIs are retained-layer changes,
not a silent replacement of rewritten code.

The frozen merge compiler is `solar-main-becd2143-draft1`, SHA256
`82012fcd6f18d09ba4ac3f12879a92aefb043e9290e82f199d9b1bd49e3ab542`.
Evidence is under
`target/codegen-bench/evm-rewrite-candidate/main-becd2143-merge-20260908/`.
The current 147 codegen source pins remain stable. Identical-input UI comparison
preserves 1,654 IDs, 828 source hashes and 5,048 complete bytecode objects. The
fresh nine-project heavy capture directly compares accepted writer output:
all 1,672 contracts and 3,344 objects are byte-exact. Its output fingerprints
also match the fresh full workflow; baseline raw JSON differs in reviewed
source-map markers and compiler-note locations, not bytecode.

The full workflow preserves 24 IDs, 175 ordered gas labels and 139 observations
per compiler; the Size supplement preserves 15 IDs and the same labels and
observations. Execution and deployment gas are exact. Matching pinned solc
references are reused; optional solx references are absent and are not invented.
The new benchmark workflow's 79 Python tests pass. Raw full compiler geometric
mean is -0.90%, RSS +1.05%. The reference run's parser overlap is retained, and
this is not a controlled interleaved speed measurement. Concurrent Size timing
is unused. Foundry preserves 36 configurations, all 1,537 test records and 244
reported sizes directly against the accepted writer milestone.

`503dcfc8` separately updates twelve reviewed stdout expectations and one
FileCheck directive. The independent debug audit checks 32 program views and
1,812 actual instructions. Twelve non-transfer invoke contexts disappear and
thirteen fixture source-map entries lose incorrect external RETURN `o` markers;
source ranges and other contexts remain exact. The full heavy metadata review
covers 729 changed sourceMap fields and 6,423 entries: only `o` to `-` at actual
external STOP/RETURN instructions, with all other contract fields and source
ranges preserved. This intentionally adopts main's stricter transfer semantics.

The final workspace has 11,747 UI passes, the original alias failure, 1,557 other
passes and two skips. Its distinct actual debug executable is recorded in
`workspace2-completion.json`; it is not relabeled as the frozen benchmark binary.
Clippy, formatting, typos and diff checks pass. The merge and metadata update are
local commits; they do not establish green remote CI, a successful push, sealed
size parity or completion of the rewrite goal.
