//! Coalesce consecutive constant-size heap allocations into a single bump.
//!
//! Bump allocation makes the pointer of every allocation in a straight-line
//! group a fixed offset from the group's first pointer, so fusing the group
//! into one `alloc` of the summed size preserves every pointer value exactly
//! and needs no escape or use analysis. Each member after the first becomes a
//! constant `add` off the fused base, eliminating that member's free-memory
//! pointer load, bump, and store. Zero-initialized members retain explicit
//! zeroing at their original program points. Only instructions that could
//! observe or move the free-memory pointer between two member allocations
//! invalidate the grouping; those end the current group instead of being
//! crossed. Checked allocations also remain separate: fusing their panic checks
//! could move a later failure ahead of an earlier check or memory expansion.
//!
//! Intervening accesses may legitimately target a later member's future
//! range: because every member keeps its pointer value and its initialization
//! at its original program point, the fused program stores and zeroes the
//! same addresses in the same order as the unfused one. Avoiding the
//! free-memory-pointer word alone is therefore never the safety argument.
//! Successful accesses earlier in the same block bound raw addresses, so constant offsets from
//! those addresses cannot wrap onto the FMP. Group discovery uses one immutable proof snapshot;
//! rewriting starts only after every group has been selected.
//! Within a group, a successful access to one allocation also bounds every later allocation:
//! their bases differ only by the intervening constant bumps. This lets nested objects initialize
//! a later member's fields before its first word without losing the nonwrapping proof.
//! These additional fusions require the intervening field offset to keep its immediate width;
//! growing it can defeat compact initialization sequences and their later outlining.
//! The additional proof stops when a member pointer is stored outside the group. Extending
//! escaping pointer live ranges changes stack and outlining costs beyond this local estimate;
//! existing per-access proofs still permit the original coalescing in that case.

use crate::{
    mir::{
        AllocationFailure, AllocationInitialization, AllocationSemantics, BlockId, EffectKind,
        Function, FunctionBuilder, InstId, InstKind, MirModuleTarget, MirType, Module, PtrLocation,
        RawMemoryAccess, Value, ValueId,
        analysis::{AliasAnalysis, ReadBounds},
        memory::EvmMemoryLayout,
        pass::MirPass,
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

/// Bounds each member and the fused size so offsets fit within four bytes.
const MAX_MEMBER_SIZE: u64 = 1 << 32;

/// Module pass fusing consecutive constant-size allocations.
pub(crate) struct CoalesceAllocs;

impl MirPass for CoalesceAllocs {
    fn name(&self) -> &'static str {
        "coalesce-allocs"
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        let target = module.target(gcx);
        for func in module.functions.iter_mut() {
            if !func.blocks.is_empty() && coalesce_function(func, &target) {
                super::lower_memory_objects::normalize_pointer_operands(func);
                changed = true;
            }
        }
        changed
    }
}

/// One fusable allocation: its instruction, result, and constant size.
struct Member {
    inst: InstId,
    result: ValueId,
    size: u64,
    semantics: AllocationSemantics,
}

fn coalesce_function(func: &mut Function, target: &Target) -> bool {
    let proofs = ReadBounds::default();
    let mut groups = Vec::new();
    let mut bounds = GroupBounds::default();
    for block in func.blocks.indices() {
        let mut group = Vec::<Member>::new();
        for &inst_id in &func.blocks[block].instructions {
            if let Some(member) = fusable_alloc(func, inst_id) {
                let compatible = group.last().is_none_or(|last| {
                    last.semantics.initialization == member.semantics.initialization
                });
                if !compatible || !bounds.push(&member) {
                    groups.push((block, std::mem::take(&mut group)));
                    bounds.clear();
                    assert!(bounds.push(&member));
                }
                group.push(member);
                continue;
            }
            if group.is_empty() {
                continue;
            }
            if preserves_group(func, inst_id, &proofs, &bounds, target) {
                bounds.observe_access(func, inst_id);
                continue;
            }
            groups.push((block, std::mem::take(&mut group)));
            bounds.clear();
        }
        if !group.is_empty() {
            groups.push((block, group));
            bounds.clear();
        }
    }
    let mut changed = false;
    for (block, group) in groups {
        changed |= flush_group(func, block, group);
    }
    changed
}

/// Returns the group member for a fusable constant-size heap allocation.
fn fusable_alloc(func: &Function, inst_id: InstId) -> Option<Member> {
    let inst = func.inst(inst_id);
    let InstKind::Alloc { size, semantics } = &inst.kind else {
        return None;
    };
    let semantics = *semantics;
    if inst.metadata.deferred_alloc() || semantics.failure != AllocationFailure::Infallible {
        return None;
    }
    let result = func.inst_result_value(inst_id)?;
    let size = func.value_u64(*size)?;
    (size < MAX_MEMBER_SIZE).then_some(Member { inst: inst_id, result, size, semantics })
}

/// Returns whether an instruction can sit between two fused allocations.
///
/// The instruction must have no control effects and cannot observe or modify
/// the free-memory pointer: every memory access must provably avoid the FMP
/// word at `[0x40, 0x60)`.
fn preserves_group(
    func: &Function,
    inst_id: InstId,
    proofs: &ReadBounds,
    bounds: &GroupBounds,
    target: &Target,
) -> bool {
    let inst = func.inst(inst_id);
    if inst.kind.effects().control.any() {
        return false;
    }
    match inst.kind.effect_kind() {
        EffectKind::Pure
        | EffectKind::StorageRead
        | EffectKind::StorageWrite
        | EffectKind::TransientRead
        | EffectKind::TransientWrite
        | EffectKind::EnvironmentRead
        // Immutable access does not observe or move the allocation frontier.
        // Lowering places constructor homes outside the FMP word.
        | EffectKind::ImmutableRead
        | EffectKind::ImmutableWrite => true,
        // `Fmp`, `SetFmp`, non-member `Alloc` shapes, and the remaining aggregate accesses have no
        // raw memory range and stay conservative barriers. The object and mapping forms are gone
        // by this point in the pipeline.
        EffectKind::MemoryRead | EffectKind::MemoryWrite => {
            let mut avoids_fmp = None;
            inst.kind.visit_raw_memory(|_, addr, size| {
                *avoids_fmp.get_or_insert(true) &=
                    bounds.address_avoids_fmp(func, addr, target)
                        || range_avoids_fmp(func, addr, size.constant(func), inst_id, proofs);
            });
            avoids_fmp.unwrap_or(false)
        }
        EffectKind::ExternalCall
        | EffectKind::ICall
        | EffectKind::Create
        | EffectKind::Log => false,
    }
}

/// Returns whether `[addr, addr + len)` provably avoids the FMP word.
///
/// `len` is `None` when the access length is not a compile-time constant.
fn range_avoids_fmp(
    func: &Function,
    addr: ValueId,
    len: Option<u64>,
    user: InstId,
    proofs: &ReadBounds,
) -> bool {
    proofs
        .read_lower_bound_at(func, addr, &|_| None, false, user)
        .is_some_and(|bound| bound >= EvmMemoryLayout::HEAP_START)
        || !AliasAnalysis::range_may_overlap_fmp(func, addr, len)
}

/// Rewrites a collected group into one fused allocation plus constant offsets.
fn flush_group(func: &mut Function, block: BlockId, members: Vec<Member>) -> bool {
    if members.len() < 2 {
        return false;
    }
    let Some(total) = members.iter().try_fold(0u64, |sum, member| {
        sum.checked_add(member.size).filter(|&total| total < MAX_MEMBER_SIZE)
    }) else {
        return false;
    };

    let initialization = members[0].semantics.initialization;
    let zeroed = initialization == AllocationInitialization::Zeroed;
    let preserves_fmp =
        members.iter().any(|member| func.inst(member.inst).metadata.preserves_fmp());

    let base = members[0].result;
    let (total_size, offsets, zero_insts) = {
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        let total_size = builder.imm(total);
        let mut offsets = Vec::with_capacity(members.len().saturating_sub(1));
        let mut offset = members[0].size;
        for member in &members[1..] {
            offsets.push(builder.imm(offset));
            offset += member.size;
        }
        let mut zero_insts = Vec::new();
        if zeroed {
            for member in &members {
                let start = builder.func().blocks[block].instructions.len();
                let metadata = builder.func().inst(member.inst).metadata.clone();
                builder.set_debug_context(&metadata);
                // word = ptrtoint member to i256
                // memory_zero word, size
                let size = builder.imm(member.size);
                builder.memory_zero(member.result, size);
                zero_insts.push(builder.func_mut().blocks[block].instructions.split_off(start));
            }
        }
        (total_size, offsets, zero_insts)
    };

    // The fused size sums the member sizes, so the final frontier matches the
    // member-by-member bumps.
    // base = alloc total_size
    let instruction = func.inst_mut(members[0].inst);
    instruction.kind =
        InstKind::Alloc { size: total_size, semantics: AllocationSemantics::INTERNAL };
    instruction.metadata.set_preserves_fmp(preserves_fmp);

    // member = add base, offset
    for (member, offset) in members[1..].iter().zip(offsets) {
        let instruction = func.inst_mut(member.inst);
        instruction.kind = InstKind::Add(base, offset);
        instruction.metadata.set_effect(None);
        instruction.metadata.set_memory_region(None);
    }
    // member = alloc/add ...
    // word = ptrtoint member to i256
    // memory_zero word, size
    if zeroed {
        let mut zeros = members.iter().map(|member| member.inst).zip(zero_insts).peekable();
        let old = std::mem::take(&mut func.blocks[block].instructions);
        let instructions = &mut func.blocks[block].instructions;
        instructions.reserve(old.len() + members.len());
        for inst in old {
            instructions.push(inst);
            if let Some((_, zeroing)) = zeros.next_if(|&(member, _)| member == inst) {
                instructions.extend(zeroing);
            }
        }
        assert!(zeros.next().is_none(), "coalesced allocation disappeared from its block");
    }
    true
}

/// Allocation offsets in the current straight-line group, whose members are sparse among values.
/// An accessed member bounds only itself and later members, never an earlier wrapping bump.
#[derive(Default)]
struct GroupBounds {
    offsets: FxHashMap<ValueId, u64>,
    size: u64,
    bounded: Option<u64>,
    escaped: bool,
}

impl GroupBounds {
    fn clear(&mut self) {
        self.offsets.clear();
        self.size = 0;
        self.bounded = None;
        self.escaped = false;
    }

    fn push(&mut self, member: &Member) -> bool {
        let Some(size) = self.size.checked_add(member.size) else { return false };
        self.offsets.insert(member.result, self.size);
        self.size = size;
        true
    }

    fn observe_access(&mut self, func: &Function, inst: InstId) {
        if let InstKind::Store(address, value, PtrLocation::Memory) = func.inst(inst).kind
            && self.member_address(func, value).is_some()
            && self.member_address(func, address).is_none_or(|(member, offset)| {
                member.checked_add(offset).is_none_or(|address| address >= self.size)
            })
        {
            self.escaped = true;
        }
        func.inst(inst).kind.visit_raw_memory(|access, address, size| {
            if access != RawMemoryAccess::PartialWrite
                && size.constant(func).is_some_and(|size| size != 0)
                && let Some(&offset) =
                    self.offsets.get(&AliasAnalysis::memory_address_value(func, address))
            {
                self.bounded = Some(self.bounded.map_or(offset, |bound| bound.min(offset)));
            }
        });
    }

    fn address_avoids_fmp(&self, func: &Function, address: ValueId, target: &Target) -> bool {
        if self.escaped {
            return false;
        }
        let Some(bounded) = self.bounded else { return false };
        let Some((member, offset)) = self.member_address(func, address) else { return false };
        // A successful access bounds its member below 2^64. The group's total bumps
        // and this address's accumulated offset each fit u64, so their sum cannot
        // wrap i256. All members start at or above the heap.
        let Some(fused_offset) = member.checked_add(offset) else { return false };
        member >= bounded
            && (member == 0
                || target.push(U256::from(fused_offset)).bytes
                    <= target.push(U256::from(offset)).bytes)
    }

    fn member_address(&self, func: &Function, mut address: ValueId) -> Option<(u64, u64)> {
        let mut offset = 0u64;
        for _ in 0..8 {
            address = AliasAnalysis::memory_address_value(func, address);
            if let Some(&member) = self.offsets.get(&address) {
                return Some((member, offset));
            }
            let Value::Inst(inst) = *func.value(address) else { return None };
            let InstKind::Add(first, second) = func.inst(inst).kind else { return None };
            let (base, step) = if let Some(step) = func.value_u64(second) {
                (first, step)
            } else {
                (second, func.value_u64(first)?)
            };
            if func.value_ty(address) != Some(MirType::I256) {
                return None;
            }
            offset = offset.checked_add(step)?;
            address = base;
        }
        None
    }
}
