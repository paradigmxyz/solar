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
//! crossed.
//!
//! Intervening accesses may legitimately target a later member's future
//! range: because every member keeps its pointer value and its initialization
//! at its original program point, the fused program stores and zeroes the
//! same addresses in the same order as the unfused one. Avoiding the
//! free-memory-pointer word alone is therefore never the safety argument.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AllocationAlignment, AllocationFailure, AllocationInitialization, AllocationKind,
        AllocationSemantics, BlockId, EffectKind, Function, FunctionBuilder, InstId, InstKind,
        MirType, Module, ValueId,
    },
    pass::MirPass,
};
use solar_sema::Gcx;

/// Fused groups keep their prefix sums far below the allocation panic bound so
/// the single overflow check on the summed size stays equivalent to the
/// member-by-member checks it replaces.
const MAX_MEMBER_SIZE: u64 = 1 << 32;

/// Module pass fusing consecutive constant-size allocations.
pub(crate) struct CoalesceAllocs;

impl MirPass for CoalesceAllocs {
    fn name(&self) -> &'static str {
        "coalesce-allocs"
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !func.blocks.is_empty() {
                changed |= coalesce_function(func);
            }
        }
        changed
    }
}

/// One fusable allocation: its instruction, result, and word-exact size.
struct Member {
    inst: InstId,
    result: ValueId,
    size: u64,
    semantics: AllocationSemantics,
}

fn coalesce_function(func: &mut Function) -> bool {
    let has_allocs =
        func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Alloc { .. }));
    if !has_allocs {
        return false;
    }

    let mut changed = false;
    let blocks = func.blocks.indices().collect::<Vec<_>>();
    for block in blocks {
        let instructions = func.blocks[block].instructions.clone();
        let mut group = Vec::<Member>::new();
        for inst_id in instructions {
            if let Some(member) = fusable_alloc(func, inst_id) {
                let compatible = group.last().is_none_or(|last| {
                    last.semantics.initialization == member.semantics.initialization
                });
                if !compatible {
                    changed |= flush_group(func, block, &mut group);
                }
                group.push(member);
                continue;
            }
            if preserves_group(func, inst_id) {
                continue;
            }
            changed |= flush_group(func, block, &mut group);
        }
        changed |= flush_group(func, block, &mut group);
    }
    changed
}

/// Returns the group member for a fusable constant-size heap allocation.
fn fusable_alloc(func: &Function, inst_id: InstId) -> Option<Member> {
    let inst = func.inst(inst_id);
    let InstKind::Alloc { size, kind: AllocationKind::Raw, semantics } = &inst.kind else {
        return None;
    };
    let semantics = *semantics;
    if inst.metadata.deferred_alloc() {
        return None;
    }
    let result = func.inst_result_value(inst_id)?;
    let size = func.value_u64(*size)?;
    let size = match semantics.alignment {
        AllocationAlignment::Exact => size,
        AllocationAlignment::Word => EvmMemoryLayout::align_word(size)?,
    };
    (size < MAX_MEMBER_SIZE).then_some(Member { inst: inst_id, result, size, semantics })
}

/// Returns whether an instruction can sit between two fused allocations.
///
/// True exactly when the instruction can neither observe nor modify the
/// free-memory pointer: every memory access it performs must provably avoid
/// the FMP word at `[0x40, 0x60)`.
fn preserves_group(func: &Function, inst_id: InstId) -> bool {
    let inst = func.inst(inst_id);
    match inst.kind.effect_kind() {
        EffectKind::Pure
        | EffectKind::StorageRead
        | EffectKind::StorageWrite
        | EffectKind::TransientRead
        | EffectKind::TransientWrite
        | EffectKind::EnvironmentRead => true,
        EffectKind::MemoryRead => match &inst.kind {
            InstKind::MLoad(addr) => {
                range_avoids_fmp(func, *addr, Some(EvmMemoryLayout::WORD_SIZE))
            }
            InstKind::Keccak256(addr, len) => range_avoids_fmp(func, *addr, func.value_u64(*len)),
            // `Fmp` and `MSize` observe the allocation frontier; the object
            // and mapping forms are gone by this point in the pipeline.
            _ => false,
        },
        EffectKind::MemoryWrite => match &inst.kind {
            InstKind::MStore(addr, _) => {
                range_avoids_fmp(func, *addr, Some(EvmMemoryLayout::WORD_SIZE))
            }
            InstKind::MStore8(addr, _) => range_avoids_fmp(func, *addr, Some(1)),
            InstKind::MemoryZero(addr, len)
            | InstKind::CalldataCopy(addr, _, len)
            | InstKind::CodeCopy(addr, _, len)
            | InstKind::ReturnDataCopy(addr, _, len)
            | InstKind::ExtCodeCopy(_, addr, _, len) => {
                range_avoids_fmp(func, *addr, func.value_u64(*len))
            }
            InstKind::MCopy(dest, src, len) => {
                let len = func.value_u64(*len);
                range_avoids_fmp(func, *dest, len) && range_avoids_fmp(func, *src, len)
            }
            // `SetFmp` moves the frontier; non-member `Alloc` shapes and the
            // remaining aggregate writes stay conservative barriers.
            _ => false,
        },
        EffectKind::ExternalCall
        | EffectKind::InternalCall
        | EffectKind::Create
        | EffectKind::Log
        | EffectKind::ImmutableRead
        | EffectKind::ImmutableWrite => false,
    }
}

/// Returns whether `[addr, addr + len)` provably avoids the FMP word.
///
/// `len` is `None` when the access length is not a compile-time constant.
fn range_avoids_fmp(func: &Function, addr: ValueId, len: Option<u64>) -> bool {
    if let Some(base) = func.value_u64(addr) {
        if base >= EvmMemoryLayout::ZERO_SLOT {
            return true;
        }
        return len
            .and_then(|len| base.checked_add(len))
            .is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT);
    }
    is_proven_heap_address(func, addr)
}

/// Returns whether a runtime address is itself a typed heap pointer.
///
/// Arbitrary integer arithmetic is not sufficient: EVM addition wraps and can turn an expression
/// containing a heap pointer into the FMP address. Derived raw addresses therefore remain barriers
/// unless a separate range analysis proves them safe.
fn is_proven_heap_address(func: &Function, value: ValueId) -> bool {
    matches!(func.value_ty(value), Some(MirType::MemPtr | MirType::MemoryObject(_)))
}

/// Rewrites a collected group into one fused allocation plus constant offsets.
fn flush_group(func: &mut Function, block: BlockId, group: &mut Vec<Member>) -> bool {
    let members = std::mem::take(group);
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
    let failure = if members.iter().any(|m| m.semantics.failure == AllocationFailure::Panic) {
        AllocationFailure::Panic
    } else {
        AllocationFailure::Infallible
    };
    let preserves_fmp =
        members.iter().any(|member| func.inst(member.inst).metadata.preserves_fmp());

    let base = members[0].result;
    let appended_start = func.blocks[block].instructions.len();
    let (total_size, offsets) = {
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        let total_size = builder.imm_u64(total);
        let mut offsets = Vec::with_capacity(members.len().saturating_sub(1));
        let mut offset = members[0].size;
        for member in &members[1..] {
            offsets.push(builder.imm_u64(offset));
            offset += member.size;
        }
        if zeroed {
            for member in &members {
                let size = builder.imm_u64(member.size);
                builder.memory_zero(member.result, size);
            }
        }
        (total_size, offsets)
    };
    let zero_insts = func.blocks[block].instructions.split_off(appended_start);

    // Member sizes are already component-aligned, so the fused reservation is
    // exact and the final frontier matches the member-by-member bumps.
    let instruction = func.inst_mut(members[0].inst);
    instruction.kind = InstKind::Alloc {
        size: total_size,
        kind: AllocationKind::Raw,
        semantics: AllocationSemantics {
            alignment: AllocationAlignment::Exact,
            initialization: if zeroed {
                AllocationInitialization::Uninitialized
            } else {
                initialization
            },
            failure,
        },
    };
    instruction.metadata.set_preserves_fmp(preserves_fmp);

    for (member, offset) in members[1..].iter().zip(offsets) {
        let instruction = func.inst_mut(member.inst);
        instruction.kind = InstKind::Add(base, offset);
        instruction.metadata.set_effect(None);
        instruction.metadata.set_memory_region(None);
    }
    for (member, zero_inst) in members.iter().zip(zero_insts) {
        let position = func.blocks[block]
            .instructions
            .iter()
            .position(|&inst| inst == member.inst)
            .expect("coalesced allocation disappeared from its block");
        func.blocks[block].instructions.insert(position + 1, zero_inst);
    }
    true
}
