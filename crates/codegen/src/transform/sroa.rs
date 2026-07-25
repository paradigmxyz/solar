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
    mir::{
        AllocationKind, BlockId, Function, InstId, InstKind, MemoryObjectLayout, Module, ValueId,
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};

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
        run_function_pass(module, analyses, |func, _| SroaCx::default().run(func))
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
    fn run(&mut self, func: &mut Function) -> bool {
        let mut allocs: Vec<(BlockId, ValueId)> = Vec::new();
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::Alloc { kind: AllocationKind::Object(layout), .. } =
                    func.inst(inst_id).kind
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

        let uses = ValueUses::new(func);
        let mut replacements = FxHashMap::default();
        let mut dead = FxHashSet::default();
        for (block_id, object) in allocs {
            if let Some(plan) = self.plan(func, &uses, block_id, object) {
                replacements.extend(plan.replacements);
                dead.extend(plan.dead);
                self.eliminated += 1;
            }
        }
        if self.eliminated == 0 {
            return false;
        }
        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !dead.contains(inst));
        }
        true
    }

    /// Verifies eligibility and computes the load replacements and dead
    /// instructions for one allocation, or `None` if it cannot be scalarized.
    fn plan(
        &self,
        func: &Function,
        uses: &ValueUses,
        block_id: BlockId,
        object: ValueId,
    ) -> Option<Plan> {
        if !uses.terminators[object].is_empty() {
            return None;
        }

        // Map each field address value to its constant slot, and record the
        // address instructions. Every use of the object must be such an
        // address, in this block.
        let mut slot_of: FxHashMap<ValueId, u64> = FxHashMap::default();
        let mut address_insts: FxHashSet<InstId> = FxHashSet::default();
        for &inst_id in &uses.instructions[object] {
            let kind = &func.inst(inst_id).kind;
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
            if uses.inst_blocks[inst_id] != Some(block_id) {
                return None;
            }
            let addr = func.inst_result_value(inst_id)?;
            slot_of.insert(addr, slot);
            address_insts.insert(inst_id);
        }
        if slot_of.is_empty() {
            return None;
        }

        // Every field address must be used only as the address of an
        // `MStore`/`MLoad` in this block.
        let mut accesses = Vec::new();
        for &address in slot_of.keys() {
            if !uses.terminators[address].is_empty() {
                return None;
            }
            for &inst_id in &uses.instructions[address] {
                if uses.inst_blocks[inst_id] != Some(block_id) {
                    return None;
                }
                match func.inst(inst_id).kind {
                    InstKind::MStore(addr, value)
                        if addr == address && !slot_of.contains_key(&value) =>
                    {
                        accesses.push(inst_id);
                    }
                    InstKind::MLoad(addr) if addr == address => accesses.push(inst_id),
                    _ => return None,
                }
            }
        }
        accesses.sort_by_key(|&inst_id| {
            uses.inst_positions[inst_id].expect("indexed instruction position")
        });

        // Walk the block, forwarding stores to loads per slot.
        let mut current: FxHashMap<u64, ValueId> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();
        for inst_id in accesses {
            match func.inst(inst_id).kind {
                InstKind::MStore(addr, value) if slot_of.contains_key(&addr) => {
                    current.insert(slot_of[&addr], value);
                    dead.insert(inst_id);
                }
                InstKind::MLoad(addr) if slot_of.contains_key(&addr) => {
                    // A load with no dominating store observes uninitialized or
                    // zeroed memory; keep the allocation rather than guess.
                    let value = *current.get(&slot_of[&addr])?;
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
}

struct ValueUses {
    instructions: IndexVec<ValueId, Vec<InstId>>,
    terminators: IndexVec<ValueId, Vec<BlockId>>,
    inst_blocks: IndexVec<InstId, Option<BlockId>>,
    inst_positions: IndexVec<InstId, Option<usize>>,
}

impl ValueUses {
    fn new(func: &Function) -> Self {
        let mut instructions = index_vec![Vec::new(); func.values.len()];
        let mut terminators = index_vec![Vec::new(); func.values.len()];
        let mut inst_blocks = index_vec![None; func.num_insts()];
        let mut inst_positions = index_vec![None; func.num_insts()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (position, &inst_id) in block.instructions.iter().enumerate() {
                inst_blocks[inst_id] = Some(block_id);
                inst_positions[inst_id] = Some(position);
                for operand in func.inst(inst_id).operands() {
                    instructions[operand].push(inst_id);
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    terminators[operand].push(block_id);
                }
            }
        }
        Self { instructions, terminators, inst_blocks, inst_positions }
    }
}

struct Plan {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: FxHashSet<InstId>,
}
