//! Scalar replacement of non-escaping memory-object allocations.
//!
//! A struct or fixed-array memory object that never escapes and is accessed
//! only at constant indices through views of its words can be dissolved into
//! SSA values: each field store feeds the matching field load directly. The
//! backing allocation remains because its free-memory-pointer bump and failure
//! behavior are observable independently of accesses through its result.
//!
//! Eligible accesses may span branches and loops. The pruned SSA constructor
//! shared with frame promotion inserts phis only where a field is live-in and
//! rejects loads lacking a reaching store on any path. All fields are planned
//! together before mutation, including full-object zero fills. The allocation
//! stays in place. Full-width pointer casts retain the same object identity;
//! escapes, narrowing casts, dynamic indices and partial/unknown accesses reject promotion.
//! Run before memory-object lowering, while element accesses remain explicit.

use crate::mir::{
    AllocationKind, Function, Immediate, InstKind, MemoryObjectLayout, Module, SliceLocation,
    Value, ValueId,
    analysis::{AliasAnalysis, Location, LocationSize},
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass_with_alias},
    transform::frame_promotion::{SlotAccessInfo, promote_object_slots},
};
use alloy_primitives::U256;
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
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_with_alias(module, analyses, |func, analyses| {
            SroaCx::default().run(func, analyses.alias())
        })
    }
}

#[derive(Debug, Default)]
struct SroaCx {
    /// Number of allocations dissolved.
    eliminated: usize,
}

impl SroaCx {
    fn run(&mut self, func: &mut Function, alias: &AliasAnalysis) -> bool {
        let mut allocs: Vec<(ValueId, MemoryObjectLayout)> = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { kind: AllocationKind::Object(layout), .. } =
                    func.inst(inst_id).kind
                    && layout.fixed_words().is_some()
                    && let Some(object) = func.inst_result_value(inst_id)
                {
                    allocs.push((object, layout));
                }
            }
        }
        if allocs.is_empty() {
            return false;
        }

        let mut changed = false;
        for (object, layout) in allocs {
            if self.try_promote(func, alias, object, layout).unwrap_or(false) {
                alias.clear_cached_addresses();
                self.eliminated += 1;
                changed = true;
            }
        }
        changed
    }

    /// Verifies eligibility and scalarizes every field in one transaction.
    fn try_promote(
        &self,
        func: &mut Function,
        alias: &AliasAnalysis,
        object: ValueId,
        layout: MemoryObjectLayout,
    ) -> Option<bool> {
        if alias.value_escapes(func, object)
            || func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize))
        {
            return None;
        }

        let words = layout.fixed_words()?;
        let zero_size = words.checked_mul(EvmMemoryLayout::WORD_SIZE)?;

        let mut object_aliases = FxHashSet::from_iter([object]);
        let mut address_insts = FxHashSet::default();
        loop {
            let mut changed = false;
            for id in func.instructions() {
                if let InstKind::PtrToInt(base, 256) | InstKind::IntToPtr(base) = func.inst(id).kind
                    && object_aliases.contains(&base)
                    && let Some(result) = func.inst_result_value(id)
                    && object_aliases.insert(result)
                {
                    address_insts.insert(id);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // Object aliases may only feed preserving casts, full-object zero fills, and fixed
        // views of the object.
        let mut views = FxHashSet::default();
        for inst_id in func.instructions() {
            if address_insts.contains(&inst_id) {
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            match *kind {
                InstKind::MemoryZero(base, size) if object_aliases.contains(&base) => {
                    if func.value_u64(size) != Some(zero_size) {
                        return None;
                    }
                    continue;
                }
                InstKind::MakeSlice { ptr, len, location: SliceLocation::Memory }
                    if object_aliases.contains(&ptr) && !object_aliases.contains(&len) =>
                {
                    views.insert(func.inst_result_value(inst_id)?);
                    address_insts.insert(inst_id);
                    continue;
                }
                _ => {}
            }
            // Any other use of the object (a store of the pointer, a raw address) blocks
            // scalarization.
            if kind.any_operand(|value| object_aliases.contains(&value)) {
                return None;
            }
        }

        // Map each element access through a view to its constant slot. Views may only feed
        // those accesses, and a store must not leak a view.
        let mut slot_of = FxHashMap::default();
        for inst_id in func.instructions() {
            if address_insts.contains(&inst_id) {
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            let (slice, offset) = match *kind {
                InstKind::SliceLoadWord { slice, offset } => (slice, offset),
                InstKind::SliceStoreWord { slice, offset, value } if !views.contains(&value) => {
                    (slice, offset)
                }
                _ => {
                    if kind.any_operand(|value| views.contains(&value)) {
                        return None;
                    }
                    continue;
                }
            };
            if !views.contains(&slice) {
                continue;
            }
            let offset = func.value_u64(offset)?;
            let slot = Some(offset / EvmMemoryLayout::WORD_SIZE).filter(|&slot| {
                offset.is_multiple_of(EvmMemoryLayout::WORD_SIZE) && slot < words
            })?;
            slot_of.insert(inst_id, slot);
        }

        // Non-capturing terminators can still read the object's memory.
        if func.blocks.iter().any(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                terminator
                    .any_operand(|value| object_aliases.contains(&value) || views.contains(&value))
            })
        }) {
            return None;
        }

        let location = Location::Memory(alias.bare_memory_location(
            func,
            object,
            LocationSize::Const(zero_size),
        )?);
        for inst in func.instructions() {
            let kind = &func.inst(inst).kind;
            if func.inst_result_value(inst) == Some(object)
                || address_insts.contains(&inst)
                || slot_of.contains_key(&inst)
                || matches!(*kind, InstKind::MemoryZero(base, _) if object_aliases.contains(&base))
            {
                continue;
            }
            let effects = alias.instruction_mod_ref(func, inst);
            if effects.may_read(alias, location) || effects.may_write(alias, location) {
                return None;
            }
        }
        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                let effects = alias.terminator_mod_ref(func, term);
                if effects.may_read(alias, location) || effects.may_write(alias, location) {
                    return None;
                }
            }
        }

        let mut fields: Vec<_> = slot_of.values().copied().collect();
        fields.sort_unstable();
        fields.dedup();
        if fields.is_empty() {
            return Some(false);
        }
        let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        let mut slots: Vec<_> = fields
            .iter()
            .map(|&slot| SlotAccessInfo::object(object, slot, func.blocks.len()))
            .collect();
        let mut zeros = FxHashSet::default();
        for (block, data) in func.blocks.iter_enumerated() {
            for &inst in &data.instructions {
                if func.inst_result_value(inst) == Some(object) {
                    for slot in &mut slots {
                        slot.note_reset(block, inst);
                    }
                }
                match func.inst(inst).kind {
                    InstKind::SliceStoreWord { value, .. } if slot_of.contains_key(&inst) => {
                        let index = fields.binary_search(&slot_of[&inst]).ok()?;
                        slots[index].note_store(block, inst, value);
                    }
                    InstKind::SliceLoadWord { .. } if slot_of.contains_key(&inst) => {
                        let index = fields.binary_search(&slot_of[&inst]).ok()?;
                        slots[index].note_load(block, inst);
                    }
                    InstKind::MemoryZero(base, _) if object_aliases.contains(&base) => {
                        zeros.insert(inst);
                        for slot in &mut slots {
                            slot.note_store(block, inst, zero);
                        }
                    }
                    _ => {}
                }
            }
        }
        if !promote_object_slots(func, &slots) {
            return Some(false);
        }
        // preserving_cast object; make_memory_slice object, len; memory_zero object, size
        // => SSA field values
        for block in &mut func.blocks {
            block
                .instructions
                .retain(|inst| !address_insts.contains(inst) && !zeros.contains(inst));
        }
        Some(true)
    }
}
