//! Decides when an instruction deleted in favor of another may lend its source origins to it.
//!
//! A pass that deletes an instruction and redirects its uses to a surviving one loses the
//! deleted instruction's origins. When the survivor sits in the same block, before the deleted
//! instruction, with no instruction between them that has side effects, it executes exactly when
//! the deleted one would have, so the union of both origins is valid shared-code metadata. This
//! is the rule block dedup and tail merging already follow. Across blocks, or across an effect
//! such as a check, call, or store, the survivor can run on paths that never reach the deleted
//! expression, and a union would credit origins to paths that do not execute them.
//!
//! [`SameBlock`] numbers the effects of each block once, so every query is constant time.

use crate::mir::{BlockId, Function, InstId, Value, ValueId};
use solar_data_structures::index::IndexVec;

/// Position of an instruction: its block, its index in the block, and the number of
/// side-effecting instructions before it in the block.
#[derive(Clone, Copy)]
struct Position {
    block: BlockId,
    index: usize,
    effects: usize,
}

/// Effect-free reachability between instructions of one block.
pub(crate) struct SameBlock {
    positions: IndexVec<InstId, Option<Position>>,
}

impl SameBlock {
    /// Numbers every instruction of `func`, counting deleted-but-not-yet-removed ones too.
    pub(crate) fn new(func: &Function) -> Self {
        let mut positions = IndexVec::from_vec(vec![None; func.num_insts()]);
        for block in func.blocks.indices() {
            let mut effects = 0;
            for (index, &inst) in func.blocks[block].instructions.iter().enumerate() {
                positions[inst] = Some(Position { block, index, effects });
                effects += usize::from(func.inst(inst).kind.has_side_effects());
            }
        }
        Self { positions }
    }

    /// Returns the instruction defining `survivor` if it runs exactly when `deleted` would have.
    pub(crate) fn survivor(
        &self,
        func: &Function,
        survivor: ValueId,
        deleted: InstId,
    ) -> Option<InstId> {
        let Value::Inst(survivor) = *func.value(survivor) else { return None };
        let deleted_at = self.positions.get(deleted)?.as_ref()?;
        let survivor_at = self.positions.get(survivor)?.as_ref()?;
        (survivor_at.block == deleted_at.block
            && survivor_at.index < deleted_at.index
            && survivor_at.effects + usize::from(func.inst(survivor).kind.has_side_effects())
                == deleted_at.effects)
            .then_some(survivor)
    }
}

/// Unions the source origins of `deleted` into `survivor`.
pub(crate) fn merge_origins(func: &mut Function, survivor: InstId, deleted: InstId) {
    let context = func.inst(deleted).metadata.debug_context();
    func.inst_mut(survivor).metadata.merge_debug_context(&context);
}
