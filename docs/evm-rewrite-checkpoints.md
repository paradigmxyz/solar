# EVM rewrite checkpoint history

## Evidence and scope audit — 2026-09-05

The starting deletion commit is `e5ba34f2` over baseline `9cb036c0`.
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
