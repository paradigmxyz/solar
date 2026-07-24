# MIR and EVM IR optimizer audit

This audit compares all 35 MIR passes exposed by `mir-opt`, the pipeline-only
`defer-alloc` variant, and all eight EVM IR passes exposed by `evm-opt`. It
focuses on correctness, missed transformations, phase placement, and EVM gas
and size tradeoffs.

## Scope and method

The comparison used local source checkouts at these revisions:

| Compiler | Revision | Main areas inspected |
| --- | --- | --- |
| LLVM | `a815e6f267c1` | `InstCombine`, SCCP, GVN/PRE, DCE/ADCE, loop passes, CFG simplification, jump threading |
| rustc | `29e68fe2295f` | MIR simplify-CFG, GVN, jump threading, destination propagation, dead-store elimination |
| solc | `d8fbf3676eae` | Yul optimizer suite, control-flow simplification, load/store analysis, block deduplication, stack compression, dispatch |
| Vyper/Venom | `5067b86906f4` | Venom SCCP, DFT, load elimination, store elimination, CFG normalization, dispatch |
| Fe/Sonatina | `bc5a9c3e8001` / `55ca888f1fc8` | Fe lowering and Sonatina CFG, data-flow, GVN, LICM, DCE, load/store and EVM passes |
| Plank | `386cc0d725ee` | EVM-oriented IR pipeline, CFG cleanup, constant propagation, DCE and code generation |

LLVM and rustc provide the strongest references for SSA and CFG algorithms.
solc and Venom provide the most relevant semantic and gas-cost comparisons.
Sonatina is especially useful where a conventional SSA algorithm is adapted to
an EVM backend. Fe and Plank have fewer directly comparable mature transforms,
but help check phase boundaries and EVM-specific assumptions.

The principal implementation anchors were:

- LLVM: `llvm/lib/Transforms/Scalar/{NewGVN,JumpThreading,SimplifyCFGPass}.cpp`,
  `llvm/lib/Transforms/Utils/{SimplifyCFG,LoopSimplify,SimplifyIndVar}.cpp`,
  and `llvm/include/llvm/Transforms/Utils/SCCPSolver.h`.
- rustc: `compiler/rustc_mir_transform/src/{gvn,jump_threading,dest_prop,dead_store_elimination}.rs`
  and its simplify-CFG modules.
- solc: `libyul/optimiser/{FullInliner,LoadResolver,UnusedStoreEliminator,EqualStoreEliminator,EquivalentFunctionCombiner}.cpp`
  and `libsolidity/codegen/ir/IRGenerator.cpp`.
- Vyper/Venom: `vyper/venom/passes/sccp/sccp.py`,
  `vyper/venom/passes/{mem2var,dead_store_elimination,simplify_cfg,cfg_normalization,dft}.py`,
  and `vyper/codegen/module.py`.
- Sonatina: `crates/codegen/src/optim/{sccp,gvn,load_store,licm,known_bits_simplify,range_branch_simplify}.rs`,
  `crates/codegen/src/isa/evm/{late_block_merge,late_section_merge,exact_func_merge}.rs`,
  and `crates/codegen/src/isa/evm/static_arena_alloc/`.
- Plank: `plankc/sir/crates/passes/src/optimizations/` and
  `plankc/sir/crates/passes/src/transforms/critical_edge_splitting.rs`.

Priority labels mean:

- **P0**: correctness or invalid-IR risk.
- **P1**: high-value missed optimization or structural limitation.
- **P2**: useful extension after the P1 foundations.
- **P3**: low expected value, specialized, or already competitive.

“Kept” means the change is included with this audit. “Roadmap” means it needs
more design or corpus validation. “Rejected” means an attempted change failed
validation or did not meet the codegen acceptance criteria.

## Findings kept in this change

### Correctness

- `lower-dispatch` now rejects one-to-three byte calldata before selector
  matching whenever a selector ends in zero.
  `CALLDATALOAD` zero-padding could otherwise make the short input match that
  selector. solc guards the whole selector switch; Vyper also uses the
  trailing-zero condition to avoid unnecessary checks in one routing form.
- EVM codegen no longer has a second, implicit selector-routing path. The
  backend accepts only final `evm-shaped` MIR; ordinary allocation operations
  must be lowered before that phase, while deferred static-allocation
  placeholders remain available for final layout. If a required lowering pass
  cannot complete, codegen reports the earlier phase instead of silently
  switching representations.
- `lower-abi` now preflights return fusion on clones. An unsupported dynamic
  return can no longer leave earlier functions partially mutated when the
  all-or-nothing phase transition bails.
- SROA and copy elision retain `alloc`. Its free-memory-pointer update,
  initialization, and failure behavior are effects in this IR; deleting it can
  change `mload(0x40)`, `msize`, gas observations, later allocation addresses,
  or a checked allocation panic.
- EVM CFG simplification no longer redirects an address-taken jump thunk.
  Pushed EVM block addresses are observable values and cannot be rewritten as
  if every use were a CFG edge. Adjacent `push label; jump/jumpi` pairs remain
  eligible because they are provably static control-flow uses.
- `lower-evm-shaped` classifies non-returning functions from reachable CFG
  terminators. An unreachable `return` no longer suppresses a legal tail call.
- Ordered constant switch evaluation stops at the first unknown case. It no
  longer skips an unknown earlier comparison and selects a later constant case.

### Conservative simplification and pass accounting

- MIR CFG simplification removes switch cases that target the default and folds
  an empty switch to a jump.
- EVM CFG simplification folds a same-target conditional jump to `POP` plus an
  unconditional jump, preserving consumption of the condition.
- DCE and ADCE now report unreachable-block and phi repair mutations, so cached
  analyses are invalidated after real CFG changes.
- MIR and EVM pass runners, pass mutation results, and reachability-phi repair
  results are `#[must_use]`. Call sites now either incorporate each result or
  name the intentional discard.
- PRE now enforces its documented maximum of two inserted predecessor
  computations per rewrite; the old predicate made that cap redundant with the
  path-profitability test.
- Static allocation discovery reaches a fixpoint instead of stopping after four
  propagation rounds.
- Terminal block deduplication preserves hotness when a hot duplicate is merged
  into an existing cold representative.

## MIR pass-by-pass comparison

| Pass | Closest references | Assessment and next action |
| --- | --- | --- |
| `inline` | LLVM inliner, rustc MIR inliner, solc `FullInliner` | The gas-mode cost model, recursion guards, frame relocation, multiple-return reconstruction, and caller continuation-phi repair are substantial; size mode is intentionally disabled from prior measurement. Callees that themselves contain phis are still rejected. Clone their SSA web and predecessor mapping before widening eligibility; richer assembled-size and hotness costing comes after legality. **P1 roadmap.** |
| `outline-reverts` | solc `ReasoningBasedSimplifier`, LLVM cold outlining, Sonatina late section merge | It recognizes identical constant revert shapes but not families that differ only in payload words. Sonatina’s late section pass groups terminal payload layouts, parameterizes up to four store values, and accepts only positive estimated savings. Keep MIR outlining conservative; that algorithm belongs in EVM IR after stack layout is explicit. **P2 roadmap.** |
| `function-dce` | LLVM global DCE, solc unused-function pruning | Reachability is module-local and adequate. The append-only instruction arena means stale calls remain validator-visible, so active-only call-graph filtering is unsafe without arena compaction. An attempted filter caused dangling function IDs and was **rejected**. Add explicit instruction compaction first. **P1 roadmap.** |
| `sccp` | LLVM SCCP, Venom SCCP, Sonatina and Plank SCCP | Solid constant lattice and executable-edge basis. It lacks known-bits or small-range facts that could fold EVM masks, comparisons, and overflow checks. Sonatina keeps those as shared scalar facts and a separate range environment; that is preferable to overloading the SCCP lattice. **P1 roadmap.** |
| `pure-eval` | LLVM constant evaluation/IPSCCP, solc expression simplification | This is a bounded interpreter for closed, no-argument, side-effect-free functions, not an interprocedural evaluator. It handles word operations, phis, and deterministic control flow. Ordered switches now refuse an unknown earlier case. Reachability-aware effect screening would admit functions with dead effects; evaluating pure internal calls would require a module-level recursion and memoization design. **P2 roadmap.** |
| `inst-simplify` | LLVM InstCombine/InstructionSimplify, Sonatina inst simplify | Good EVM-specific identities, including masks and `selfbalance`. Rules are distributed across passes and lack known-bits support. Build one canonical fold API used by SCCP, GVN, PRE, and jump threading. **P1 roadmap.** |
| `cse` | LLVM EarlyCSE, solc CSE, Sonatina GVN | This is well beyond local CSE: it has dominator inheritance, phi-expression sinking, canonical memory ranges and storage aliases, path clobber summaries through diamonds and loops, call summaries, and account-environment invalidation. Its keys still use operand identities, intentionally leaving transitive congruence to GVN. The next gain is shared expression canonicalization and memory state with GVN/load PRE, not another cache layer. **P2 roadmap.** |
| `pre` | LLVM GVN-PRE, Sonatina GVN | This is already an iterated, termination-bounded join PRE with dominator availability, phi construction, batched noninterfering rewrites, and critical-edge splitting. Its documented maximum of two inserted computations is now actually enforced. It does not solve global anticipatability; measure relaxing the cap before considering a full lazy-code-motion formulation. **P2 fixed; roadmap.** |
| `gvn` | LLVM NewGVN/GVN, Sonatina GVN | The iterated pessimistic RPO algorithm already finds transitive expression congruence, pairwise-congruent phis, commutative forms, and dominating leaders. Mutually recursive phi webs can remain artificially distinct because their initial classes differ; LLVM NewGVN’s optimistic equivalence classes or an SCC-local solver are the relevant extensions, not more local expression keys. **P2 roadmap.** |
| `storage-load-cse` | solc load resolver, Sonatina `LoadStoreSolver`, Venom `mem2var` | This is a cheap block-local forwarding pass with alias-aware invalidation. Cross-block storage reuse is already handled by the later, stronger `load-pre`; building a second CFG solver here would duplicate machinery. Share the canonical transfer state with `load-pre`, or keep this as the fast early cleanup and measure whether both pipeline positions earn their cost. **P2 roadmap.** |
| `storage-dse` | solc store eliminator, Venom store elimination, Sonatina load/store solver | Primarily local overwrite elimination. Add backward liveness over storage locations and remove stores dead on every reverting or overwriting exit, while treating external calls and observable returns conservatively. **P1 roadmap.** |
| `load-pre` | LLVM GVN-PRE/load PRE, Sonatina load/store solver, Plank critical-edge splitting | This is already a substantial greatest-fixpoint availability solver across storage, transient storage, memory, and keccak, with store forwarding, loop handling, alias barriers, gas/`msize` safety, phis, and a path-cost model. Partial insertions currently require jump-terminated predecessors and a locatable concrete value. Critical-edge splitting would widen that case; Plank is a compact structural reference, although MIR phi repair differs. **P2 roadmap.** |
| `loop-canonicalize` | LLVM LoopSimplify/LCSSA, Sonatina loop simplify | Creates useful preheaders but does not guarantee the full canonical contract expected by mature loop passes: single latch, dedicated exits, and LCSSA-like exit values. Establish those invariants before expanding LICM or IV transforms. **P1 roadmap.** |
| `indvar-simplify` | LLVM IndVarSimplify, ScalarEvolution | It already strength-reduces affine address expressions for one positive additive induction variable into a loop-carried pointer phi. Missing cases include multiple IVs, negative or otherwise legal recurrence steps, compare/exit normalization, and replacing values used after the loop. Extend those behind EVM cost estimates rather than broadening pattern matching ad hoc. **P1 roadmap.** |
| `storage-promotion` | LLVM mem2reg analogy, solc loop-aware store handling | Strong EVM-specific loop promotion with alias and call barriers, rollback-aware exits, multiple disjoint initialized slots, and loop-info recomputation after each promotion. Remaining extensions—non-isolated exits, selected analyzable calls, or loop-variant-but-provably-disjoint slots—raise the proof cost sharply and should be driven by measured corpus cases. **P3.** |
| `licm` | LLVM LICM, Sonatina LICM | Hoisting uses dependency closure, ScalarEvolution-backed affine range checks, memory/storage/transient alias barriers, a gas-observer bailout, a minimum gas saving, and an eight-instruction cap. Missing sinking and broader promotion are secondary to stronger loop canonicalization. Add them only with explicit gas profitability. **P2 roadmap.** |
| `check-elim` | LLVM LazyValueInfo/constraint elimination, Sonatina range analysis, rustc dataflow | It already carries unsigned intervals plus direct `lt`, `le`, equality, and inequality relations down the dominator tree, and recursively proves add/sub/mul check shapes. It lacks transitive relation closure, offset relations, and general joins of facts from multiple predecessors. Sonatina’s loop-aware range environments are a close model; a bounded difference-constraint domain would cover the next Solidity cases. **P1 roadmap.** |
| `jump-threading` | LLVM JumpThreading, rustc MIR jump threading | It path-compresses empty forwarders and threads predecessor edges through phi-only branch or switch blocks when that edge supplies a constant. It does not propagate conditions through intervening computations or predecessor chains. rustc’s two-phase condition graph is a good model: walk statements backward, then fulfill chains forward with cached, cost-bounded block duplication while refusing loop headers. **P1 roadmap.** |
| `cfg-simplify` | LLVM SimplifyCFG, rustc simplify-CFG, Sonatina CFG simplify | The fixpoint already handles reachability, safe forwarders, single-predecessor merging, trivial phis, and alpha-equivalent terminal-block deduplication. Default-target switch cleanup is now kept. Standalone constant branch/switch folding, branch hoist/sink, and profitable common-tail formation remain; SCCP currently supplies much of the first item in the default pipeline. **P1 roadmap.** |
| `frame-slot-promotion` | LLVM mem2reg/SROA, rustc destination propagation | This is a capable pruned-SSA mem2reg: it collects all candidate slots, computes live-in sets and iterated dominance-frontier phis, handles loops and internal calls, and applies each slot conservatively. Each slot still runs a separate SSA builder, and any `gas` or `msize` observation rejects the whole function. Shared multi-slot placement or path-sensitive observation barriers are possible but lower priority. **P3 roadmap.** |
| `memory-dse` | LLVM DSE/MemorySSA, solc load/store analysis, Venom `mem2var`/store elimination | This is one of the richer passes: local alias-aware forwarding is supplemented by immutable-copy reuse, frame-store cleanup, cross-block equal/overwritten stores, and backward liveness. The global liveness domain currently names only constant aligned words while richer object and affine locations stay in separate scans. A shared MemorySSA-like state keyed by canonical locations and mod/ref summaries would improve joins without making calls unsound. **P1 roadmap.** |
| `static-alloc` | LLVM stack allocation promotion, solc memory allocation analysis, Sonatina static arena allocation | Correctly recognizes bounded non-escaping allocations. Derived-address propagation now reaches a fixpoint. A worklist would be more efficient than rescanning. Sonatina’s liveness-conflict graph and exact/first-fit arena packing are a concrete model for safely overlapping compatible fixed allocations. **P2.** |
| `defer-alloc` | solc deferred memory allocation patterns | Pipeline-only mode of the static-allocation analysis. It has the same propagation strengths and limitations; keep its legality shared with `static-alloc`. **P2.** |
| `sroa` | LLVM SROA, rustc scalar replacement | Single-block aggregate scalarization is deliberately narrow. It now preserves allocation effects. The next safe expansion is to feed fields into frame-slot promotion or SSA construction rather than duplicating cross-block logic. **P0 fixed; P1 roadmap.** |
| `copy-elision` | LLVM DSE/memcpy optimization, rustc destination propagation | It already computes a whole-function closure of derived addresses and removes stores and several copy forms only when no read or escape exists. It now preserves allocation effects and starts from active allocation instructions. Range-aware partial-object liveness could remove only unread fields or copy ranges, but memory DSE/SROA are better homes for that complexity. **P0 fixed; P3 roadmap.** |
| `dce` | LLVM DCE, rustc dead-store/DCE | Conventional mark-and-sweep DCE. Mutation reporting is fixed. A def-use worklist would avoid repeated whole-function scans. **P2.** |
| `adce` | LLVM ADCE, Sonatina ADCE | It already removes pure, phi-free branch and switch regions that transparently reconverge, then runs DCE to a fixpoint. It is narrower than the liveness and control-dependence formulation: effects, escaping values, phis, or nonidentical reconvergence stop the search. Sonatina is a compact model using post-dominance frontiers, live-root marking, live-phi predecessor edges, and explicit divergent exits. Phi-repair reporting is fixed. **P1 roadmap.** |
| `lower-abi` | solc ABI generator, Vyper ABI lowering, Fe lowering | This is progressive lowering rather than optimization. Scalar head words stay lazy and dynamic calldata arguments remain logical slices; static word returns can be fused into `returndata`. Clone-based preflight fixes transactional bailout when any live return still needs dynamic encoding. Dynamic return encoding is the main missing ABI case. **P0 fixed; P2 roadmap.** |
| `lower-dispatch` | solc IR dispatch, Vyper module dispatch | This is progressive lowering. When a selector ends in zero, short calldata now takes fallback/revert before selector matching; other selector sets retain the equivalent smaller path. **P0 fixed.** |
| `lower-evm-shaped` | tail-call lowering in LLVM/Sonatina; EVM jump lowering | Reachable non-return analysis now exposes tail calls hidden by dead returns. Call-frame eligibility remains intentionally backend-driven. **P0 fixed.** |
| `lower-mapping-slots` | solc storage layout lowering, Vyper storage lowering | Word keys lower to canonical `keccak(key, slot)` scratch memory, while memory and calldata keys append the slot to their byte sequence before hashing. Keeping these semantic forms through CSE is the right boundary. No important legalization gap was found; later memory and storage passes should optimize repeated concrete operations. **P3.** |
| `lower-abi-encode` | solc `ABIFunctions`, Vyper ABI encoder | It already has a known-size static fast path, direct static tuple/aggregate encoding, and structured dynamic head/tail loops with bounded scratch state. A later optimization could encode directly into a known consuming return/call buffer and avoid an intermediate slice, but that needs use analysis and gas/size evidence rather than changes to this legalization pass. **P3 roadmap.** |
| `lower-aggregates` | LLVM aggregate legalization, Fe/Sonatina lowering | Straightforward decomposition. Large fixed aggregates may cause code growth; introduce an unroll threshold only with a loop-form fallback and corpus evidence. **P2 roadmap.** |
| `lower-memory-objects` | LLVM data layout/legalization, compiler-specific | Policy-driven legalization has no close peer pass because the semantic memory-object IR is project-specific. Keep it simple and move optimization before or after it, not into the policy layer. **P3.** |
| `lower-slices` | solc/Vyper ABI slice lowering | This lowering is already interprocedural: it infers compact calldata parameters to a fixed point, expands call signatures, splits slice-typed selects and phis into paired SSA words, and folds projections. Remaining aggregate uses are deliberately left intact instead of guessed into one word. No important optimizer gap was found here; extend it alongside new slice operations or slice-return support. **P3.** |
| `lower-alloc` | solc memory allocator lowering, Vyper memory allocator | Final physical allocation lowering is appropriately late. No standalone optimization belongs here beyond canonical code shapes consumed by EVM peepholes. **P3.** |

## EVM IR pass-by-pass comparison

| Pass | Closest references | Assessment and next action |
| --- | --- | --- |
| `peephole` | solc peephole optimizer, Sonatina EVM peepholes | The streaming matcher already covers constants, inverse stack operations, comparisons, stores/loads, and branch idioms. The next useful step is not a larger ad-hoc list: use a small symbolic stack window to prove multi-op rewrites, with strict stack-height and gas accounting, and import only remaining solc identities that win that model. **P1 roadmap.** |
| `share-reverts` | solc equivalent-function/block combining, Sonatina late block/section merge | The pass deliberately handles one adjacent empty-revert shape and already protects a frequently referenced shared revert at the `PUSH1` boundary. General sharing still needs hotness and exact post-layout label-width costing because the added jump can lose runtime gas. **P1 roadmap.** |
| `compact-pushes` | solc constant optimizer | Size wins are not always gas wins: for example, `PUSH32 max` costs less runtime gas than `PUSH0; NOT`. A blanket size-only gate was attempted and **rejected** because the full runtime-gas corpus was not available at the pinned revision. Introduce per-constant gas/size costing instead. **P1 roadmap.** |
| `cfg-simplify` | LLVM/rustc CFG simplify, Sonatina CFG simplify, Venom CFG passes | Same-target conditional folding is kept. Address-taken thunks are no longer redirected, while adjacent static jump-label pushes are still threaded. Next steps are constant `JUMPI`, path-compressed forwarders, and an explicit label-use kind so correctness does not depend on recognizing instruction adjacency. **P0 fixed; P1 roadmap.** |
| `outline` | LLVM machine outliner, solc common subexpression/function combining, Sonatina late section merge | It already finds repeated closed stack computations and large pushes, rejects overlaps, and applies a conservative lower-bound size test. That estimate assumes short pushes and fixed call-site widths, so exact assembled size, competing-group selection, and hotness should precede broader use in gas mode. Sonatina’s late section merger is the useful model for repeatedly selecting the best remaining group after lowering. **P1 roadmap.** |
| `terminal-dedup` | LLVM merge-functions/block dedup, solc equivalent-function combining | Exact terminal deduplication is low risk. Representative hotness is now preserved. Canonical selection should eventually minimize layout and label width, not depend on encounter order. **P2.** |
| `tail-merge` | LLVM TailMerge, Sonatina common-tail merge | It iterates to a fixed point, preserves hotness, and uses a lower-bound byte threshold, but greedily chooses one representative and suffix at a time. Suffix classes plus exact size and hotness costing would avoid locally profitable merges that lose globally or add gas. **P1 roadmap.** |
| `block-layout` | LLVM MachineBlockPlacement, Sonatina lowered block order | It already forms unconditional-jump traces, separates cold terminal traces, estimates encoded sizes, and packs small multiply referenced hot terminals below the `PUSH1` boundary. Missing pieces are conditional-edge trace formation, profile-derived edge frequencies, and a shared exact assembled-size model for layout, outlining, sharing, and tail merging. **P1 roadmap.** |

## Pipeline-level comparison

The phase boundary is a strength. Semantic mapping, ABI, aggregate, slice,
memory-object, dispatch, call-shape, and allocation lowering remain explicit
MIR-to-MIR transitions instead of leaking into the assembler. The EVM backend
now enforces that boundary instead of retaining a second selector-routing
implementation for earlier phases. This is closer to LLVM legalization and
Sonatina’s staged lowering than to solc’s more intertwined Yul rewrite
sequence.

The main pipeline weakness is repetition without a shared convergence policy.
solc can repeat bracketed optimizer subsequences until stable. Sonatina instead
uses small composite passes; for example, its SCCP step performs CFG cleanup,
SCCP, another cleanup, and ADCE while sharing invalidation rules. This pipeline
manually repeats longer scalar and CFG groups, while individual passes use
different local fixpoint strategies.

A `-Ztime-passes` sample over ten larger UI codegen files, producing twelve MIR
modules, illustrates the opportunity but is not enough evidence to remove a
pass:

- the second and later MIR invocations changed IR infrequently: GVN changed one
  of 24 runs, frame-slot promotion one of 24, and DCE one of 24;
- EVM `share-reverts` changed none of 48 runs, the two tail-merge positions
  changed nine of 48, and CFG simplification changed 13 of 120;
- terminal dedup changed 12 of 24 runs, showing that the first canonicalization
  remains useful even when later cleanup often does not.

The next pipeline experiment should group only mutually enabling passes into
small, bounded, change-driven composites. Each candidate still needs
baseline-versus-candidate output comparison on both the UI corpus and the
pinned CI corpus; a no-change result on this sample is not evidence that a pass
is globally redundant.

## Cross-pass priorities

The highest-value optimizer work is structural rather than another collection
of local identities:

1. Add an instruction-arena compaction step or make validation explicitly
   active-instruction based. Until then, stale instructions constrain
   interprocedural transforms and function DCE.
2. Establish stronger loop form: preheaders, single latches, dedicated exits,
   and exit-value SSA. Then extend induction simplification and LICM.
3. Build shared sparse data-flow machinery for storage and memory states. Use it
   for cross-block load CSE, load PRE, and DSE.
4. Centralize scalar folding and known-bits/range facts across instruction
   simplification, SCCP, GVN, check elimination, and jump threading.
5. Build one EVM cost model with optimization mode, block hotness, jump cost,
   label widths, and final assembled size. Use it in push compaction, revert
   sharing, outlining, tail merging, and layout.
6. Implement genuine ADCE only after post-dominators and control dependence are
   available.

## Validation and measurements

The codegen UI corpus was measured before and after the retained changes with
the same debug-build procedure and the same 119 successfully compiled test
IDs. Expected diagnostic fixtures were excluded from both totals.

| Mode | Revision | Deploy bytes | Runtime bytes | Delta |
| --- | --- | ---: | ---: | ---: |
| `-Ogas` | baseline | 72,157 | 63,903 | — |
| `-Ogas` | retained changes | 72,322 | 64,068 | +165 / +165 |
| `-Osize` | baseline | 72,312 | 64,058 | — |
| `-Osize` | retained changes | 72,529 | 64,275 | +217 / +217 |

These are regressions in byte count, not claimed performance improvements.
Serialized runtime bytecode changed for 11 fixtures in gas mode and 15 in size
mode. All 11 gas-mode changes and 14 of the size-mode changes grew; one
size-mode output changed at equal length. Disassembly of that output,
`lowering/recursive_memory_return.sol`, showed only the expected frame-address
shift from retaining its allocation. The largest per-file deploy-bytecode
increase was 41 bytes in
`lowering/storage_long_bytes_return.sol` in both modes; the next largest was 28
bytes in `lowering/recursive.sol`. The remaining increase is consistent with
required short-calldata guards and allocation effects that were previously
removed unsafely. Recomputing `calldatasize`
inside the guard block avoids carrying it across the CFG and reduced the
initial candidate by 803 deploy bytes in gas mode and 801 in size mode.
Distinguishing provably static jump-label pushes from observable address values
recovered another 2,234 and 2,331 deploy bytes, respectively. Finally, the
Vyper-style observation that zero padding can only match a selector whose last
byte is zero lets other selector tables omit the guard, recovering a further
928 and 932 deploy bytes. The changes are retained because the old output can
dispatch invalid short calldata or remove observable allocation behavior.

The speculative EVM gas-mode gating change was not retained. The local UI
corpus only measures generated size; accepting that change requires the pinned
CI corpus with hot runtime-gas calls and matching successful call labels.

Validation completed:

- all 788 workspace tests, with six skipped;
- focused MIR and EVM IR UI coverage for CFG simplification, PRE, pure
  evaluation, SROA, copy elision, ABI lowering, dispatch lowering, EVM-shape
  lowering, and static-allocation propagation;
- the complete default UI suite and the separately configured Standard JSON
  mode;
- the full Foundry harness, with 14 project tests passing and six ignored,
  including an eight-case receive/fallback differential suite compiled with
  both this compiler and solc; its short-calldata case uses a three-byte
  zero-padded prefix of a real selector;
- full workspace formatting and linting;
- baseline-versus-candidate codegen UI corpus comparisons in both optimization
  modes.
