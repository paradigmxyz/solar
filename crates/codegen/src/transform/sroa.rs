//! Scalar replacement of non-escaping memory-object allocations.
//!
//! A struct or fixed-array memory object that never escapes and is accessed
//! only through constant field/element addresses can be dissolved into SSA
//! values: each field store feeds the matching field load directly. The
//! backing allocation remains because its free-memory-pointer bump and failure
//! behavior are observable independently of accesses through its result.
//!
//! This runs conservatively within a single block, where store-to-load
//! ordering is explicit and no phi reconstruction is required:
//! - the allocation is an `Object(Struct | FixedArray)` whose result does not escape;
//! - every use of the object is a `MemoryObjectFieldAddr`/ `MemoryObjectElementAddr` with a
//!   constant field/index, in the same block;
//! - every field address is used only as the address of an `MStore`/`MLoad` in that block;
//! - every load is dominated by a store to the same field, so no uninitialized slot is observed.
//!
//! When all of these hold, loads are replaced by the last stored value and the
//! stores and addresses are removed.

use crate::{
    analysis::AliasAnalysis,
    mir::{
        AllocationKind, BlockId, Function, InstId, InstKind, MemoryObjectLayout, Module, ValueId,
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Scalar-replacement-of-aggregates pass for memory objects.
pub(crate) struct Sroa;

impl MirPass for Sroa {
    fn name(&self) -> &'static str {
        "sroa"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            SroaCx::default().run(func, &analyses.alias)
        })
    }
}

#[derive(Debug, Default)]
struct SroaCx {
    /// Number of allocations dissolved.
    eliminated: usize,
}

/// Whether a memory-object layout is a fixed-shape aggregate whose slots are
/// one word each (a struct or a fixed array). Bytes and dynamic arrays carry
/// length words and variable data, so they are not scalar-replaced here.
fn is_fixed_aggregate(layout: MemoryObjectLayout) -> bool {
    matches!(layout, MemoryObjectLayout::Struct { .. } | MemoryObjectLayout::FixedArray { .. })
}

impl SroaCx {
    fn run(&mut self, func: &mut Function, alias: &AliasAnalysis) -> bool {
        let mut allocs: Vec<(BlockId, ValueId)> = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { kind: AllocationKind::Object(layout), .. } =
                    func.instructions[inst_id].kind
                    && is_fixed_aggregate(layout)
                    && let Some(object) = func.inst_result_value(inst_id)
                {
                    allocs.push((block_id, object));
                }
            }
        }
        if allocs.is_empty() {
            return false;
        }

        let inst_results = func.inst_results();
        let mut changed = false;
        for (block_id, object) in allocs {
            if let Some(plan) = self.plan(func, alias, &inst_results, block_id, object) {
                self.apply(func, block_id, plan);
                self.eliminated += 1;
                changed = true;
            }
        }
        changed
    }

    /// Verifies eligibility and computes the load replacements and dead
    /// instructions for one allocation, or `None` if it cannot be scalarized.
    fn plan(
        &self,
        func: &Function,
        alias: &AliasAnalysis,
        inst_results: &FxHashMap<InstId, ValueId>,
        block_id: BlockId,
        object: ValueId,
    ) -> Option<Plan> {
        if alias.value_escapes(func, object) {
            return None;
        }

        // Map each field address value to its constant slot, and record the
        // address instructions. Every use of the object must be such an
        // address, in this block.
        let mut slot_of: FxHashMap<ValueId, u64> = FxHashMap::default();
        let mut address_insts: FxHashSet<InstId> = FxHashSet::default();
        for (inst_id, kind) in func.instructions.iter_enumerated().map(|(i, inst)| (i, &inst.kind))
        {
            let slot = match *kind {
                InstKind::MemoryObjectFieldAddr { object: base, field, .. } if base == object => {
                    Some(field)
                }
                InstKind::MemoryObjectElementAddr { object: base, index, .. } if base == object => {
                    func.value_u64(index)
                }
                _ => {
                    // Any other use of the object (data pointer, length,
                    // dynamic-index address, a store of the pointer) blocks
                    // scalarization.
                    if kind.operands().contains(&object) {
                        return None;
                    }
                    continue;
                }
            };
            let slot = slot?;
            let addr = inst_results.get(&inst_id).copied()?;
            slot_of.insert(addr, slot);
            address_insts.insert(inst_id);
        }

        // Every field address must be used only as the address of an
        // `MStore`/`MLoad` in this block.
        let block = &func.blocks[block_id];
        let block_insts: FxHashSet<InstId> = block.instructions.iter().copied().collect();
        for (inst_id, inst) in func.instructions.iter_enumerated() {
            let kind = &inst.kind;
            let addr = match *kind {
                InstKind::MStore(addr, value) => {
                    // The address may be a field address; the stored value must
                    // not be one (that would leak the interior pointer).
                    if slot_of.contains_key(&value) {
                        return None;
                    }
                    addr
                }
                InstKind::MLoad(addr) => addr,
                _ => {
                    if kind.operands().iter().any(|op| slot_of.contains_key(op)) {
                        return None;
                    }
                    continue;
                }
            };
            if slot_of.contains_key(&addr) {
                if !block_insts.contains(&inst_id) {
                    return None;
                }
            } else if kind.operands().iter().any(|op| slot_of.contains_key(op)) {
                return None;
            }
        }

        // Walk the block, forwarding stores to loads per slot.
        let mut current: FxHashMap<u64, ValueId> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();
        for &inst_id in &block.instructions {
            match func.instructions[inst_id].kind {
                InstKind::MStore(addr, value) if slot_of.contains_key(&addr) => {
                    current.insert(slot_of[&addr], value);
                    dead.insert(inst_id);
                }
                InstKind::MLoad(addr) if slot_of.contains_key(&addr) => {
                    // A load with no dominating store observes uninitialized or
                    // zeroed memory; keep the allocation rather than guess.
                    let value = *current.get(&slot_of[&addr])?;
                    if let Some(result) = inst_results.get(&inst_id) {
                        replacements.insert(*result, value);
                    }
                    dead.insert(inst_id);
                }
                _ => {}
            }
        }

        dead.extend(address_insts);
        Some(Plan { replacements, dead })
    }

    fn apply(&self, func: &mut Function, block_id: BlockId, plan: Plan) {
        func.replace_uses_canonicalized(&plan.replacements);
        func.blocks[block_id].instructions.retain(|inst| !plan.dead.contains(inst));
        // Address instructions live in the same block; remove any that ended up
        // elsewhere defensively.
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !plan.dead.contains(inst));
        }
    }
}

struct Plan {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: FxHashSet<InstId>,
}
