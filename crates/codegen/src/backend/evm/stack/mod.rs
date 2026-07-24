//! Physical EVM stack scheduling at the MIR-to-EVM lowering boundary.
//!
//! MIR names SSA values and leaves their physical placement unspecified. EVM
//! instructions instead consume an ordered stack head and can directly reach
//! only the top 16 values with `DUP1..16` and `SWAP1..16`. This module owns the
//! target-specific state needed to bridge those representations: the current
//! physical layout, local operand preparation, control-flow-edge layout
//! transitions, and memory-backed spill locations.
//!
//! ## Architecture
//!
//! In optimized builds, the late `evm-inst-schedule` MIR pass runs immediately before this
//! subsystem and orders movable, single-use expression trees in backend consumption order.
//! Effectful instructions, `gas`, `msize`, phis, and shared results constrain that order; so does
//! producer order for operations whose lowering already costs both equivalent operand
//! orientations. This shortens avoidable producer-to-consumer distances without putting physical
//! stack layouts into MIR; liveness is then recomputed over the selected order and remains the
//! scheduler's source of preservation requirements.
//!
//! The stack subsystem is split by responsibility:
//!
//! - [`model`] is the source of truth for the emitted physical stack. Slots are either a known MIR
//!   value or an anonymous word produced by low-level code.
//! - [`scheduler`] prepares the ordered operands for one instruction. It can consume dead values in
//!   place, preserve live values, duplicate or swap accessible values, and rematerialize
//!   immediates, arguments, or stored spills. Plans are replayable and are applied to the model
//!   only when chosen.
//! - [`shuffler`] canonicalizes complete layouts on selected CFG edges. Its greedy result is
//!   accepted only if the modeled stack reaches the exact target; a bounded exact search handles
//!   layouts the greedy path cannot arrange.
//! - [`spill`] assigns memory slots. Values visible across blocks receive function-stable
//!   reservations, while block-local slots are released and reused after the block is emitted.
//!
//! Local instruction scheduling and CFG-edge shuffling are intentionally
//! separate. The former optimizes a small operand head without imposing one
//! canonical layout on every block. The latter runs only where lowering has
//! selected a stack-resident edge; ordinary edges retain the conservative spill
//! and reload path.
//!
//! ## Design lineage
//!
//! The instruction-local boundary and last-use-aware operand preparation are adapted from
//! [Plank's intra-operation scheduler]. We keep the same useful separation between choosing an
//! instruction and preparing its operands, but not Plank's unique-value stack invariant, greedy
//! preparer, or scheduler-owned static allocator. Our representation permits repeated MIR values
//! and anonymous words, uses one-action and lower-bound-certified fast paths before bounded A*,
//! and fits the existing direct MIR-to-EVM lowering and memory conventions. This is an original
//! implementation rather than vendored Plank code.
//!
//! The preceding MIR ordering pass is adapted from [Venom's dependency-first traversal]. Venom
//! first applies [single-use expansion], then orders both data and effect dependencies with
//! inter-block stack-order feedback. Our pass is deliberately smaller: it operates within
//! barrier-delimited basic-block segments, does not introduce assignments, and leaves any segment
//! with a shared result unchanged.
//!
//! [solc's SSA stack layout generator] and [Sonatina's stackify allocator] were evaluated for
//! control-flow layouts and spill handling. They use whole-function layout machinery, fixed-point
//! spill discovery, and canonical block-entry stacks. Sonatina's current operand preparer also
//! supplies the verified one-action and unary fast-path pattern used here, while its normalized
//! search cache and packed state remain coupled to its symbolic stack allocator. We retain
//! conservative cross-block spills and add verified edge shuffles only where the current lowering
//! can keep values stack-resident; importing either whole allocator would require a separate
//! machine IR and a different calling and memory model. Fe delegates EVM code generation to
//! Sonatina through its [Sonatina integration], so it does not add another stack scheduler to
//! adapt.
//!
//! [Plank's intra-operation scheduler]: https://github.com/plankevm/plank-monorepo/blob/386cc0d725ee34df11565ededc81414ef495e05f/plankc/sir/crates/stack-scheduling/src/greedy_intra_op_scheduler/mod.rs
//! [Venom's dependency-first traversal]: https://github.com/vyperlang/vyper/blob/730a2d36f1fca90be059c75681de5c942560ce0b/vyper/venom/passes/dft.py
//! [single-use expansion]: https://github.com/vyperlang/vyper/blob/730a2d36f1fca90be059c75681de5c942560ce0b/vyper/venom/passes/single_use_expansion.py
//! [solc's SSA stack layout generator]: https://github.com/ethereum/solidity/blob/03fe7dd46c793ba556dad3302b0ba3fe4273760e/libyul/backends/evm/ssa/StackLayoutGenerator.cpp
//! [Sonatina's stackify allocator]: https://github.com/fe-lang/sonatina/blob/55ca888f1fc83077e5eee803c0619231e9b50998/crates/codegen/src/stackalloc/stackify/planner/operand_prep.rs
//! [Sonatina integration]: https://github.com/fe-lang/fe/blob/636607d1a859bb68d88460c5ee63dd9532791aa8/crates/codegen/src/sonatina/mod.rs
//!
//! ## Operand planning
//!
//! Lowering supplies operands in EVM push order and a liveness-derived set of
//! values that must survive the instruction. An exact-prefix check avoids all
//! search and allocation. A linear lower-bound proof handles the common shape
//! where one unique last-use operand is already on top and every other operand
//! must be materialized. Gas mode also uses verified one-action and unary fast
//! paths. Longer unambiguous plans use a deterministic walk only when its cost
//! reaches the admissible lower bound; ambiguous layouts use bounded A*. These
//! are tiers of the same planner, not sequential optimizers: every accepted
//! result satisfies the same exact goal and cost ordering, and a tier that
//! cannot prove its result falls through without mutating the stack.
//! Transitions are `DUP`, `SWAP`, safe redundant-copy `POP`, and sound
//! materializations. A goal is valid only when the exact operand head is
//! present, every preserved operand still has a copy below it, and no dead
//! operand copy remains below it. The final condition prevents a locally cheap
//! rematerialization such as `PUSH0` from deferring a more expensive cleanup
//! until immediately after the instruction. Debug builds replay every accepted
//! tier against that complete goal; exhaustive small-layout tests compare the
//! selected plan with a reference Dijkstra search under the full cost order.
//!
//! Size mode deliberately keeps the established deterministic/A* path after
//! the exact-prefix and single-resident proofs. A local one-action or unary plan
//! can tie the search's byte cost while leaving a different live or anonymous
//! residual layout, and that layout can cost more to arrange later. Until the
//! cost model extends beyond the current instruction, those two fast paths stay
//! disabled for size mode. Gas mode keeps them only when they improve its
//! primary local cost, with whole-corpus output and compile-time benchmarks
//! guarding the tradeoff. General `POP` exploration follows the same gas-only
//! rule; a one-action `POP` is accepted only by the gas fast path.
//!
//! Plans are compared lexicographically by the selected optimization mode:
//! static gas first for `-O gas` and encoded bytes first for `-O size`, followed
//! by the other metric and action count. Immediate width and `PUSH0`
//! availability are included in the estimate. Direct spill and argument loads
//! are priced as an address push plus a load; recursive internal functions use
//! the larger frame-pointer-load, offset-add, and value-load cost. Planning does
//! not allocate new spill slots. In gas and size modes, a reloadable live
//! operand that is already resident remains in the preservation set: consuming
//! it would merely move the cost into a later `MLOAD`. Values present only in
//! memory are not duplicated preemptively. `-O none` bypasses the planner and
//! retains the straightforward emission path.
//!
//! Binary lowering may also plan an equivalent reversed operand order. This
//! covers commutative instructions and comparison pairs such as `LT`/`GT`; the
//! cheaper complete plan wins. A free first orientation is already unbeatable,
//! so the second planner call is skipped.
//!
//! ## Correctness boundaries
//!
//! A plan never mutates the live model while it is being searched. The selected
//! action list is replayed once, emitted once, and followed by the instruction's
//! declared stack effect. Anonymous words are not treated as interchangeable
//! MIR values, and failed bounded local searches fall back to the established
//! emitter. Complete edge shuffles require an exact final layout; lowering checks
//! that an edge is preparable before committing it and treats a later shuffle
//! failure as an internal invariant violation.
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
