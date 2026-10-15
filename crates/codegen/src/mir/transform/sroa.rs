//! Scalar replacement of non-escaping memory-object allocations.
//!
//! A constant-size memory allocation that never escapes and is accessed
//! only by word loads and stores at constant offsets can be dissolved into
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
//! Run before memory-object lowering, while `ptradd` keeps object offsets explicit.

use crate::mir::{
    BlockId, Function, Immediate, InstKind, Module, PtrLocation, Value, ValueId,
    analysis::{AliasAnalysis, CfgInfo, Location, LocationSize},
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
        let mut allocs = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { size, .. } = func.inst(inst_id).kind
                    && let Some(size) = func.value_u64(size)
                    && EvmMemoryLayout::word_slot(size).is_some()
                    && let Some(object) = func.inst_result_value(inst_id)
                {
                    allocs.push((object, size));
                }
            }
        }
        if allocs.is_empty()
            || func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize))
        {
            return false;
        }

        // Promotion adds phis but no blocks, so one block order serves every allocation.
        let order = CfgInfo::new(func).rpo().to_vec();
        let mut changed = false;
        for (object, size) in allocs {
            if self.try_promote(func, alias, &order, object, size).unwrap_or(false) {
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
        order: &[BlockId],
        object: ValueId,
        zero_size: u64,
    ) -> Option<bool> {
        if alias.value_escapes(func, object) {
            return None;
        }

        let words = zero_size / EvmMemoryLayout::WORD_SIZE;

        // Pointer casts and constant `ptradd`s of the object address its words; record each
        // derived address with its byte offset. A derivation is no phi, so its operand comes
        // first in reverse postorder.
        let mut addresses = FxHashMap::from_iter([(object, 0_u64)]);
        let mut address_insts = FxHashSet::default();
        for &block in order {
            for &id in &func.blocks[block].instructions {
                let derived = match func.inst(id).kind {
                    InstKind::PtrToInt(base, 256) | InstKind::IntToPtr(base) => {
                        addresses.get(&base).copied()
                    }
                    InstKind::PtrAdd(base, offset) => addresses
                        .get(&base)
                        .and_then(|&base| base.checked_add(func.value_u64(offset)?)),
                    _ => None,
                };
                if let Some(offset) = derived
                    && let Some(result) = func.inst_result_value(id)
                {
                    addresses.insert(result, offset);
                    address_insts.insert(id);
                }
            }
        }

        // Derived addresses may only feed full-object zero fills and word accesses inside the
        // object; a store must not leak one.
        let mut slot_of = FxHashMap::default();
        for inst_id in func.instructions() {
            if address_insts.contains(&inst_id) {
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            let address = match *kind {
                InstKind::MemoryZero(base, size) if addresses.get(&base) == Some(&0) => {
                    if func.value_u64(size) != Some(zero_size) {
                        return None;
                    }
                    continue;
                }
                InstKind::Load(address, PtrLocation::Memory) => address,
                InstKind::Store(address, value, PtrLocation::Memory)
                    if !addresses.contains_key(&value) =>
                {
                    address
                }
                _ => {
                    // Any other use of the object (a store of the pointer, a raw address) blocks
                    // scalarization.
                    if kind.any_operand(|value| addresses.contains_key(&value)) {
                        return None;
                    }
                    continue;
                }
            };
            let Some(&offset) = addresses.get(&address) else { continue };
            let slot = EvmMemoryLayout::word_slot(offset).filter(|&slot| slot < words)?;
            slot_of.insert(inst_id, slot);
        }

        // Non-capturing terminators can still read the object's memory.
        if func.blocks.iter().any(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                terminator.any_operand(|value| addresses.contains_key(&value))
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
                || matches!(*kind, InstKind::MemoryZero(base, _) if addresses.contains_key(&base))
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
                    InstKind::Store(_, value, PtrLocation::Memory)
                        if slot_of.contains_key(&inst) =>
                    {
                        let index = fields.binary_search(&slot_of[&inst]).ok()?;
                        slots[index].note_store(block, inst, value);
                    }
                    InstKind::Load(..) if slot_of.contains_key(&inst) => {
                        let index = fields.binary_search(&slot_of[&inst]).ok()?;
                        slots[index].note_load(block, inst);
                    }
                    InstKind::MemoryZero(base, _) if addresses.contains_key(&base) => {
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
        // preserving_cast object; ptradd object, offset; memory_zero object, size
        // => SSA field values
        for block in &mut func.blocks {
            block
                .instructions
                .retain(|inst| !address_insts.contains(inst) && !zeros.contains(inst));
        }
        Some(true)
    }
}
