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
//! The stack subsystem is split by responsibility:
//!
//! - [`model`] is the source of truth for the emitted physical stack. Slots are either a known MIR
//!   value or an anonymous word produced by low-level code.
//! - [`scheduler`] prepares the ordered operands for one instruction. It can consume dead values in
//!   place, preserve live values, duplicate or swap accessible values, and rematerialize
//!   immediates, arguments, or stored spills. Plans are replayable and are applied to the model
//!   only when chosen.
//! - [`shuffler`] canonicalizes complete layouts on selected CFG edges. Its greedy result is
//!   accepted only if replay reaches the exact target; a bounded exact search handles layouts the
//!   greedy path cannot arrange.
//! - [`spill`] assigns memory slots. Values visible across blocks receive function-stable
//!   reservations, while block-local slots are released and reused after the block is emitted.
//!
//! Local instruction scheduling and CFG-edge shuffling are intentionally
//! separate. The former optimizes a small operand head without imposing one
//! canonical layout on every block. The latter runs only where lowering has
//! selected a stack-resident edge; ordinary edges retain the conservative spill
//! and reload path.
//!
//! ## Operand planning
//!
//! Lowering supplies operands in EVM push order and a liveness-derived set of
//! values that must survive the instruction. The scheduler performs a bounded
//! best-first search over physical layouts. Its transitions are `DUP`, `SWAP`,
//! and sound materializations. A goal is valid only when the exact operand head
//! is present and every preserved value still has a copy below it.
//!
//! Plans are compared lexicographically by the selected optimization mode:
//! static gas first for `-O gas`, encoded bytes first for `-O size`, followed by
//! spill pressure and action count. Immediate width and `PUSH0` availability are
//! included in the estimate. `-O none` bypasses the planner and retains the
//! straightforward emission path.
//!
//! Binary lowering may also plan an equivalent reversed operand order. This
//! covers commutative instructions and comparison pairs such as `LT`/`GT`; the
//! cheaper complete plan wins.
//!
//! ## Correctness boundaries
//!
//! A plan never mutates the live model while it is being searched. The selected
//! action list is replayed once, emitted once, and followed by the instruction's
//! declared stack effect. Anonymous words are not treated as interchangeable
//! MIR values, and failed bounded searches fall back to the established emitter.
//! Complete edge shuffles are independently replay-verified and fail closed.
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
pub(crate) use scheduler::{OperandPlan, ScheduledOp, StackScheduler};
pub(crate) use shuffler::TargetSlot;
pub(crate) use spill::SpillSlot;
