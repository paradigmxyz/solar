//! Physical EVM stack scheduling at the MIR-to-EVM lowering boundary.
//!
//! MIR names SSA values and leaves their physical placement unspecified. EVM
//! instructions instead consume an ordered stack head. `DUP1..16` can read the
//! top 16 words, while `SWAP1..16` can exchange the top with any of the next 16.
//! This module owns the target-specific state needed to bridge those
//! representations: the current physical layout, local operand preparation,
//! control-flow-edge layout transitions, and memory-backed spill locations.
//!
//! ## Architecture
//!
//! Late MIR lowering is followed by dead-code elimination and then `evm-inst-schedule`, the final
//! pass in the default MIR lowering pipeline. DCE removes complete unused dependency chains before
//! they become physical operations ending in `POP`; instruction scheduling then orders movable,
//! single-use expression trees in backend consumption order immediately before this subsystem.
//! Effectful instructions, `gas`, `msize`, phis, and shared results constrain that order. This
//! shortens producer-to-consumer distances without putting physical stack layouts into MIR;
//! liveness is recomputed over the selected order and remains the scheduler's source of
//! preservation requirements.
//!
//! The stack subsystem is split by responsibility:
//!
//! - [`model`] is the source of truth for the emitted physical stack. Slots are either a known MIR
//!   value or an anonymous word produced by low-level code.
//! - [`scheduler`] prepares the ordered operands for one instruction. It can consume dead values in
//!   place, preserve live values, duplicate or swap accessible values, and rematerialize
//!   immediates, arguments, or stored spills. Under size optimization, equal immediate operands
//!   share one backend value identity. Plans are replayable and are applied to the model only when
//!   chosen.
//! - [`shuffler`] canonicalizes complete layouts on selected CFG edges. Layouts of up to four words
//!   compare nontrivial greedy sequences with bounded shortest-action search and accept only Pareto
//!   improvements in action count and static gas; larger layouts use the verified greedy result and
//!   reserve the search for failures.
//! - [`spill`] assigns memory slots. Values visible across blocks receive function-stable
//!   reservations, while a separate allocation list lets block-local slots be released and reused
//!   after the block is emitted without rescanning every stable reservation. In gas mode,
//!   non-overlapping cross-block live ranges share reservations to reduce frame size and memory
//!   expansion; dependency-only and size-mode reservations remain separate. Free-memory-pointer
//!   loads are stored at their definitions because later pointer updates make them impossible to
//!   rematerialize. Gas and unoptimized lowering reserve stable slots only for those that cross a
//!   block; size lowering keeps all of their slots stable to avoid downstream layout growth.
//!
//! Local instruction scheduling and CFG-edge shuffling are intentionally
//! separate. The former optimizes a small operand head without imposing one
//! canonical layout on every block. The latter runs only where lowering has
//! selected a stack-resident edge; other edges use the existing
//! fallthrough/single-predecessor preservation or conservative spill/reload
//! paths.
//!
//! Lowering selects those edges with two narrow policies. Natural loops with one preheader and one
//! latch, plus direct-jump multi-predecessor joins, may carry a canonical phi layout of up to eight
//! words. Sufficiently large selector functions with two or three used decoded arguments may
//! instead carry the live subset through compatible joins; switch components and joins owned by the
//! phi policy are excluded. Every incoming edge must agree on the same layout, and values outside a
//! selected layout keep their stable spill homes. A cold private successor keeps the physical union
//! layout but anonymizes words used only by its hot sibling; hot successors retain exact identities
//! to avoid replacing loop-carried stack hits with reloads. These are not independent schedulers
//! applied in sequence: MIR ordering changes producer order, one local planner tier prepares each
//! instruction, and an edge policy invokes the shuffler only when its complete-layout preconditions
//! hold.
//!
//! ## Design lineage
//!
//! Plank's [scheduler pipeline] at the pinned revision derives block layout sets, builds an
//! effect-aware operation graph, selects ready operations, and lets [Plank's intra-operation
//! scheduler] and [greedy edge shuffler] drive a tracked stack with scheduler-owned static
//! allocation. We adapt its instruction-local boundary and last-use-aware operand preparation, not
//! the whole pipeline. In particular, we do not inherit Plank's unique-value stack invariant,
//! greedy preparer, operation graph, or static allocator. Our representation permits repeated MIR
//! values and anonymous words, uses one-action and lower-bound-certified fast paths before bounded
//! A*, and fits the existing direct MIR-to-EVM lowering and memory conventions. This is an original
//! implementation rather than vendored Plank code.
//!
//! The preceding MIR ordering pass is adapted from [Venom's dependency-first traversal]. Venom
//! expands single-use expressions before ordering data and effect dependencies, while
//! [solx's single-use-expression pass] uses live intervals to move one-use definitions nearer their
//! users. Our pass is deliberately smaller: it operates within barrier-delimited basic-block
//! segments, does not introduce assignments, pins shared-result producers, and schedules the
//! single-use islands between them.
//!
//! [solc's SSA stack layout generator] and [Sonatina's stackify allocator] were evaluated for
//! control-flow layouts and spill handling. They use whole-function layout machinery, fixed-point
//! spill discovery, and canonical block-entry stacks. [Sonatina's operand preparer] also supplies
//! the verified one-action and unary fast-path pattern used here, while its [normalized search]
//! cache and packed state remain coupled to its symbolic stack allocator. We retain conservative
//! cross-block spills and add verified edge shuffles only where the current lowering can keep
//! values stack-resident; importing either whole allocator would require a separate machine IR and
//! a different calling and memory model. Fe delegates EVM code generation to Sonatina through its
//! [Sonatina integration], so it does not add another stack scheduler to adapt.
//! The [LLVM EVM stack solver] used by solx always rematerializes zero and small literals, but
//! retains a sufficiently wide literal when another use remains in the machine block. Its
//! whole-function backward propagation, iterative spilling, and register allocation are not a fit
//! for this direct MIR lowering. We benchmarked analogous cross-instruction retention, but it added
//! no size win beyond equal-immediate canonicalization and regressed gas, so it is not enabled.
//!
//! Cross-block spill-slot coloring follows the live-range reuse principle of LLVM's
//! [stack-slot coloring], used by the [solx LLVM EVM target]. solx also represents physical words
//! absent from one conditional successor as [unused stack slots]; our private-successor path uses
//! the same distinction between a physical word and a live identity, restricted to cold successors.
//!
//! [scheduler pipeline]: https://github.com/plankevm/plank-monorepo/blob/386cc0d725ee34df11565ededc81414ef495e05f/plankc/sir/crates/stack-scheduling/src/lib.rs
//! [Plank's intra-operation scheduler]: https://github.com/plankevm/plank-monorepo/blob/386cc0d725ee34df11565ededc81414ef495e05f/plankc/sir/crates/stack-scheduling/src/greedy_intra_op_scheduler/mod.rs
//! [greedy edge shuffler]: https://github.com/plankevm/plank-monorepo/blob/386cc0d725ee34df11565ededc81414ef495e05f/plankc/sir/crates/stack-scheduling/src/greedy_shuffler/mod.rs
//! [Venom's dependency-first traversal]: https://github.com/vyperlang/vyper/blob/730a2d36f1fca90be059c75681de5c942560ce0b/vyper/venom/passes/dft.py
//! [solx's single-use-expression pass]: https://github.com/NomicFoundation/solx-llvm/blob/a2a603232892c9824f8783b55b49d5655d77a62c/llvm/lib/Target/EVM/EVMSingleUseExpression.cpp
//! [solc's SSA stack layout generator]: https://github.com/ethereum/solidity/blob/d3ac579fe752189a9f2c365707b0f1cfc66b1437/libyul/backends/evm/ssa/StackLayoutGenerator.cpp
//! [Sonatina's stackify allocator]: https://github.com/fe-lang/sonatina/blob/55ca888f1fc83077e5eee803c0619231e9b50998/crates/codegen/src/stackalloc/stackify/mod.rs
//! [Sonatina's operand preparer]: https://github.com/fe-lang/sonatina/blob/55ca888f1fc83077e5eee803c0619231e9b50998/crates/codegen/src/stackalloc/stackify/planner/operand_prep.rs
//! [normalized search]: https://github.com/fe-lang/sonatina/blob/55ca888f1fc83077e5eee803c0619231e9b50998/crates/codegen/src/stackalloc/stackify/planner/normalize_search.rs
//! [Sonatina integration]: https://github.com/fe-lang/fe/blob/636607d1a859bb68d88460c5ee63dd9532791aa8/crates/codegen/src/sonatina/mod.rs
//! [LLVM EVM stack solver]: https://github.com/NomicFoundation/solx-llvm/blob/c7d7ec5b23366238d6201a6672c25eeee79bdcf1/llvm/lib/Target/EVM/EVMStackSolver.cpp
//! [stack-slot coloring]: https://github.com/NomicFoundation/solx-llvm/blob/a2a603232892c9824f8783b55b49d5655d77a62c/llvm/lib/CodeGen/StackSlotColoring.cpp
//! [solx LLVM EVM target]: https://github.com/NomicFoundation/solx-llvm/tree/a2a603232892c9824f8783b55b49d5655d77a62c/llvm/lib/Target/EVM
//! [unused stack slots]: https://github.com/NomicFoundation/solx-llvm/blob/a2a603232892c9824f8783b55b49d5655d77a62c/llvm/lib/Target/EVM/EVMStackSolver.cpp
//!
//! ## Operand planning
//!
//! Lowering supplies operands in EVM push order and a liveness-derived set of
//! values that must survive the instruction. An exact-prefix check avoids all
//! search. Linear proofs handle three common shapes: all
//! distinct operands require materialization; one unique last-use operand is
//! already on top and every other operand must be materialized; or a binary
//! operation must retain its sole resident operand while materializing the
//! other. Gas mode also uses verified one-action and unary fast paths. Longer
//! unambiguous plans use a deterministic walk only when its cost reaches the
//! admissible lower bound; ambiguous layouts use bounded A*. Before A*, a required value with no
//! reload route below `SWAP16` sends control directly to the spilling fallback. The same preflight
//! rejects every dead operand copy below `SWAP16` in size mode because that action set cannot
//! shorten the stack. Gas mode may search only when an accessible surplus copy can be popped to
//! expose the buried copy. The search stores parent links instead of cloned action histories and
//! independently caps expansions, created states, visited states, the open frontier, and estimated
//! retained bytes. A function-wide expansion budget bounds the sum of otherwise independent A*
//! searches, and repeated capped failures disable later A* calls while leaving the exact and linear
//! tiers available. These are tiers of the same planner, not sequential optimizers: every accepted
//! result satisfies the same exact goal and cost ordering, and a tier that cannot prove its result
//! falls through without mutating the stack.
//! Transitions are `DUP`, `SWAP`, safe redundant-copy `POP`, and sound
//! materializations. A goal is valid only when the exact operand head is
//! present, every preserved operand still has a copy below it, and no dead
//! operand copy remains below it. The final condition prevents a locally cheap
//! rematerialization such as `PUSH0` from deferring a more expensive cleanup
//! until immediately after the instruction. Every build replays an accepted
//! tier against that complete goal, rejects stack operations outside `DUP1..16` and `SWAP1..16`,
//! and verifies that each push or load materializes its claimed MIR value. Validation failure
//! falls back without applying the plan. Exhaustive
//! small-layout tests compare the selected plan with a reference Dijkstra search
//! under the full cost order and separately check that the heuristic never
//! exceeds exact remaining cost when anonymous slots are present.
//!
//! Size mode deliberately keeps the established deterministic/A* path after
//! the exact-prefix and linear proofs. A local one-action or unary plan can tie
//! the search's byte cost while leaving a different live or anonymous residual
//! layout, and that layout can cost more to arrange later. Until the cost model
//! extends beyond the current instruction, those two fast paths stay disabled
//! for size mode. Gas mode keeps their validated gas-first choices, with
//! whole-corpus output and compile-time benchmarks guarding the tradeoff.
//! General `POP` exploration follows the same gas-only rule; a one-action
//! `POP` is accepted only by the gas fast path.
//!
//! Plans are compared lexicographically by the selected optimization mode:
//! static gas first for `-O gas` and encoded bytes first for `-O size`, followed
//! by the other metric and action count. Immediate width and `PUSH0`
//! availability are included in the estimate. Direct spill and argument loads
//! use one context-independent four-byte estimate for the address push and
//! load; recursive internal functions use a seven-byte estimate for the
//! frame-pointer load, offset addition, and value load. Planning does not
//! allocate new spill slots. In gas and size modes, a reloadable live
//! operand that is already resident remains in the preservation set: consuming
//! it would merely move the cost into a later `MLOAD`. Values present only in
//! memory are not duplicated preemptively. A reloadable slot may be valid before
//! its defining block is emitted because runtime control flow still executes the
//! store first. Cheap arithmetic live-ins without an emitted store are never
//! planned as loads. They are recomputed only when their complete dependency
//! tree ends in stable inputs such as arguments, immediates, calldata, and
//! transaction/block context; constructor-staged immutable loads are memory-backed
//! and therefore excluded. When another block must reload such a value, the
//! arithmetic result itself receives a mandatory stable store at its definition
//! before local planning can consume it. A value used only as a phi-edge source
//! does not require that mandatory store solely for the edge; a successfully
//! preserved phi edge carries it on the stack.
//! `-O none` bypasses the planner and retains the straightforward emission path.
//!
//! Active equal immediates are canonicalized immediately before EVM lowering so `DUP` decisions
//! see physical word reuse instead of allocation accidents in MIR. This runs only under size
//! optimization; gas-mode canonicalization regressed runtime gas and is deliberately not enabled.
//! Immediates remain rematerializable and never own spill slots.
//!
//! Binary lowering may also plan an equivalent reversed operand order. This
//! covers commutative instructions and comparison pairs such as `LT`/`GT`; the
//! cheaper complete plan wins. A free first orientation is already unbeatable,
//! so the second planner call is skipped.
//!
//! ## Correctness boundaries
//!
//! A plan never mutates the live model while it is being searched. The selected
//! action list is validated once on scratch state, applied once to the live
//! model, emitted once, and followed by the instruction's declared stack effect.
//! Anonymous words are not treated as interchangeable MIR values, and failed
//! bounded local searches fall back to the established emitter. Complete edge
//! shuffles require an exact final layout. Phi lowering checks that an edge is
//! preparable before mutating it, and the shuffler itself changes the live model
//! only after finding and validating that layout. Global-edge preparation may
//! trim or materialize a safe intermediate stack before shuffling; a bounded
//! failure continues through conservative spill/reload lowering from that valid
//! state.
//! The edge shuffler stops adding states at its cap but drains states that were already queued, so
//! reaching the cap cannot hide an already-discovered target.
//!
//! The local planner is currently used only when operand identities remain
//! stable during preparation. Memory stores and copies, contract creation, and
//! external calls retain their freshness-aware lowering paths. Extending the
//! planner to those instructions requires the model to represent value epochs;
//! reusing the same MIR `ValueId` across a memory mutation is not sufficient to
//! distinguish an old stack copy from a newly loaded value.

mod model;
mod scheduler;
pub(crate) mod shuffler;
mod spill;

pub(crate) use model::{MAX_STACK_ACCESS, MAX_STACK_DEPTH, StackModel, StackOp};
pub(crate) use scheduler::{OperandCostModel, OperandPlan, ScheduledOp, StackScheduler};
pub(crate) use shuffler::TargetSlot;
pub(crate) use spill::SpillSlot;
