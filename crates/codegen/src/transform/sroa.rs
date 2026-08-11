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
//!   constant field/index or a full-object `memory_zero`, in the same block;
//! - every field address is used only as the address of an `MStore`/`MLoad` in that block;
//! - every load is dominated by a store to the same field or a full-object zero, so no
//!   uninitialized slot is observed.
//!
//! When all of these hold, loads are replaced by the last stored value and the
//! stores, zero fills, and addresses are removed.

use crate::{
    analysis::AliasAnalysis,
    memory::EvmMemoryLayout,
    mir::{
        AllocationKind, BlockId, Function, Immediate, InstId, InstKind, MemoryObjectLayout, Module,
        Value, ValueId,
    },
    pass::{MirPass, run_function_pass},
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
        let mut allocs: Vec<(BlockId, ValueId, MemoryObjectLayout)> = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { kind: AllocationKind::Object(layout), .. } =
                    func.inst(inst_id).kind
                    && is_fixed_aggregate(layout)
                    && let Some(object) = func.inst_result_value(inst_id)
                {
                    allocs.push((block_id, object, layout));
                }
            }
        }
        if allocs.is_empty() {
            return false;
        }

        let mut changed = false;
        for (block_id, object, layout) in allocs {
            if let Some(plan) = self.plan(func, alias, block_id, object, layout) {
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
        block_id: BlockId,
        object: ValueId,
        layout: MemoryObjectLayout,
    ) -> Option<Plan> {
        if alias.value_escapes(func, object) {
            return None;
        }

        let words = fixed_aggregate_words(layout)?;
        let zero_size = words.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
        let block = &func.blocks[block_id];
        let block_insts: FxHashSet<InstId> = block.instructions.iter().copied().collect();

        // Map each field address value to its constant slot, and record the
        // address instructions. Every use of the object must be such an
        // address or a full-object zeroing operation in this block.
        let mut slot_of: FxHashMap<ValueId, u64> = FxHashMap::default();
        let mut address_insts: FxHashSet<InstId> = FxHashSet::default();
        for inst_id in func.instructions() {
            let kind = &func.inst(inst_id).kind;
            if let InstKind::MemoryZero(base, size) = *kind
                && base == object
            {
                if !block_insts.contains(&inst_id) || func.value_u64(size) != Some(zero_size) {
                    return None;
                }
                continue;
            }
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
            let addr = func.inst_result_value(inst_id)?;
            slot_of.insert(addr, slot);
            address_insts.insert(inst_id);
        }

        // Every field address must be used only as the address of an
        // `MStore`/`MLoad` in this block.
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
            if slot_of.contains_key(&addr) {
                if !block_insts.contains(&inst_id) {
                    return None;
                }
            } else if kind.operands().iter().any(|op| slot_of.contains_key(op)) {
                return None;
            }
        }

        // Walk the block, forwarding stores to loads per slot.
        let mut current: FxHashMap<u64, Option<ValueId>> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, Option<ValueId>> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();
        for &inst_id in &block.instructions {
            match func.inst(inst_id).kind {
                InstKind::MStore(addr, value) if slot_of.contains_key(&addr) => {
                    current.insert(slot_of[&addr], Some(value));
                    dead.insert(inst_id);
                }
                InstKind::MemoryZero(base, _) if base == object => {
                    current.clear();
                    current.extend((0..words).map(|slot| (slot, None)));
                    dead.insert(inst_id);
                }
                InstKind::MLoad(addr) if slot_of.contains_key(&addr) => {
                    // A load with no dominating store observes uninitialized or
                    // zeroed memory; keep the allocation rather than guess.
                    let value = current.get(&slot_of[&addr]).copied()?;
                    if let Some(result) = func.inst_result_value(inst_id) {
                        replacements.insert(result, value);
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
        let zero = plan
            .replacements
            .values()
            .any(Option::is_none)
            .then(|| func.alloc_value(Value::Immediate(Immediate::uint256(U256::ZERO))));
        let replacements = plan
            .replacements
            .into_iter()
            .map(|(result, value)| (result, value.or(zero).expect("zero replacement allocated")))
            .collect();
        func.replace_uses_canonicalized(&replacements);
        func.blocks[block_id].instructions.retain(|inst| !plan.dead.contains(inst));
        // Address instructions live in the same block; remove any that ended up
        // elsewhere defensively.
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !plan.dead.contains(inst));
        }
    }
}

struct Plan {
    replacements: FxHashMap<ValueId, Option<ValueId>>,
    dead: FxHashSet<InstId>,
}
