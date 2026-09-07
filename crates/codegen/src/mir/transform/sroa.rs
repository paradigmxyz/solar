//! Scalar replacement of non-escaping memory-object allocations.
//!
//! A struct or fixed-array memory object that never escapes and is accessed
//! only through constant field/element addresses can be dissolved into SSA
//! values: each field store feeds the matching field load directly. The
//! backing allocation remains because its free-memory-pointer bump and failure
//! behavior are observable independently of accesses through its result.
//!
//! Eligible accesses may span branches and loops. The pruned SSA constructor
//! shared with frame promotion inserts phis only where a field is live-in and
//! rejects loads lacking a reaching store on any path. All fields are planned
//! together before mutation, including full-object zero fills. The allocation
//! stays in place; escapes, dynamic indices and partial/unknown accesses reject
//! promotion. Run before memory-object lowering, while field identities remain
//! explicit.

use crate::mir::{
    AllocationKind, Function, Immediate, InstId, InstKind, MemoryObjectLayout, Module,
    Value, ValueId,
    analysis::{AliasAnalysis, Location, LocationSize},
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    pass::{MirPass, run_function_pass},
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

fn fixed_aggregate_words(layout: MemoryObjectLayout) -> Option<u64> {
    match layout {
        MemoryObjectLayout::FixedArray { len, element_words } => {
            len.checked_mul(u64::from(element_words))
        }
        MemoryObjectLayout::Struct { fields } => Some(fields),
        MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. } => None,
    }
}

impl SroaCx {
    fn run(&mut self, func: &mut Function, alias: &AliasAnalysis) -> bool {
        let mut allocs: Vec<(ValueId, MemoryObjectLayout)> = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { kind: AllocationKind::Object(layout), .. } =
                    func.inst(inst_id).kind
                    && is_fixed_aggregate(layout)
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

        let words = fixed_aggregate_words(layout)?;
        let zero_size = words.checked_mul(EvmMemoryLayout::WORD_SIZE)?;

        // Map each field address value to its constant slot, and record the
        // address instructions. Every use of the object must be such an
        // address or a full-object zeroing operation.
        let mut slot_of: FxHashMap<ValueId, u64> = FxHashMap::default();
        let mut address_insts: FxHashSet<InstId> = FxHashSet::default();
        for inst_id in func.instructions() {
            let kind = &func.inst(inst_id).kind;
            if let InstKind::MemoryZero(base, size) = *kind
                && base == object
            {
                if func.value_u64(size) != Some(zero_size) {
                    return None;
                }
                continue;
            }
            let slot = match *kind {
                InstKind::MemoryObjectFieldAddr { object: base, field, layout: access }
                    if base == object =>
                {
                    if access != layout {
                        return None;
                    }
                    EvmMemoryLayout::field_offset(layout, field)
                        .map(|offset| offset / EvmMemoryLayout::WORD_SIZE)
                }
                InstKind::MemoryObjectElementAddr { object: base, index, layout: access }
                    if base == object =>
                {
                    if access != layout {
                        return None;
                    }
                    let offset = func
                        .value_u64(index)?
                        .checked_mul(EvmMemoryLayout::element_stride(layout)?)?;
                    Some(offset / EvmMemoryLayout::WORD_SIZE)
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
            if slot >= words {
                return None;
            }
            let addr = func.inst_result_value(inst_id)?;
            slot_of.insert(addr, slot);
            address_insts.insert(inst_id);
        }

        // Every field address must be used only as the address of an
        // `MStore`/`MLoad`.
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
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
            if !slot_of.contains_key(&addr)
                && kind.operands().iter().any(|op| slot_of.contains_key(op))
            {
                return None;
            }
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
                || matches!(*kind, InstKind::MStore(addr, _) | InstKind::MLoad(addr) if slot_of.contains_key(&addr))
                || matches!(*kind, InstKind::MemoryZero(base, _) if base == object)
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
        let zero = func.alloc_value(Value::Immediate(Immediate::uint256(U256::ZERO)));
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
                    InstKind::MStore(addr, value) if slot_of.contains_key(&addr) => {
                        let index = fields.binary_search(&slot_of[&addr]).ok()?;
                        slots[index].note_store(block, inst, value);
                    }
                    InstKind::MLoad(addr) if slot_of.contains_key(&addr) => {
                        let index = fields.binary_search(&slot_of[&addr]).ok()?;
                        slots[index].note_load(block, inst);
                    }
                    InstKind::MemoryZero(base, _) if base == object => {
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
        // field_addr object, index; memory_zero object, size => SSA field values
        for block in &mut func.blocks {
            block
                .instructions
                .retain(|inst| !address_insts.contains(inst) && !zeros.contains(inst));
        }
        Some(true)
    }
}
