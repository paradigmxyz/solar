//! Coordinated lowering of semantic MIR intrinsics and aggregate representations.

use crate::{
    mir::{AllocationKind, InstKind, MirPhase, MirType, Module},
    pass::{MirPass, ModuleAnalyses},
    transform::{
        lower_abi_encode::lower_abi_encode, lower_aggregates::lower_aggregates,
        lower_mapping_slots::lower_mapping_slots, lower_memory_objects::lower_memory_objects,
        lower_slices::lower_slices,
    },
};
use solar_sema::Gcx;

/// Lowers semantic intrinsics behind one representation boundary.
pub(crate) struct LowerIntrinsics;

impl MirPass for LowerIntrinsics {
    fn name(&self) -> &'static str {
        "lower-intrinsics"
    }

    fn is_enabled(&self, _gcx: Gcx<'_>, module: &Module) -> bool {
        module.phase == MirPhase::Dispatch
    }

    fn is_required(&self) -> bool {
        true
    }

    fn output_phase(&self) -> Option<MirPhase> {
        Some(MirPhase::IntrinsicsLowered)
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if module.phase != MirPhase::Dispatch {
            return false;
        }

        let mut changed = false;
        if has_mapping_slots(module) {
            changed |= lower_mapping_slots(module, analyses);
        }
        let mut invalidates_analyses = false;
        if has_abi_encode(module) {
            invalidates_analyses |= lower_abi_encode(module);
        }
        if has_aggregates(module) {
            invalidates_analyses |= lower_aggregates(module);
        }
        if has_slices(module) {
            invalidates_analyses |= lower_slices(module);
        }
        if has_memory_objects(module) {
            invalidates_analyses |= lower_memory_objects(module);
        }
        if invalidates_analyses {
            analyses.invalidate();
            changed = true;
        }
        if intrinsics_are_lowered(module) {
            module.advance_phase(MirPhase::IntrinsicsLowered);
            changed = true;
        }
        changed
    }
}

fn has_mapping_slots(module: &Module) -> bool {
    has_instruction(module, |kind| {
        matches!(
            kind,
            InstKind::MappingSlot(..)
                | InstKind::MappingSlotMemory(..)
                | InstKind::MappingSlotCalldata(..)
        )
    })
}

fn has_abi_encode(module: &Module) -> bool {
    has_instruction(module, |kind| matches!(kind, InstKind::AbiEncode { .. }))
}

fn has_aggregates(module: &Module) -> bool {
    has_instruction(module, |kind| {
        matches!(
            kind,
            InstKind::StorageToMemory { .. }
                | InstKind::MemoryToStorage { .. }
                | InstKind::ClearStorage { .. }
        )
    })
}

fn has_slices(module: &Module) -> bool {
    module.functions.iter().any(|func| {
        func.arg_indices()
            .map(|index| func.arg_ty(index))
            .chain(func.returns.iter().copied())
            .any(|ty| matches!(ty, MirType::Slice(_)))
            || func.instructions().any(|inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_)
                )
            })
    })
}

fn has_memory_objects(module: &Module) -> bool {
    module.functions.iter().any(|func| {
        func.arg_indices()
            .map(|index| func.arg_ty(index))
            .chain(func.returns.iter().copied())
            .any(|ty| matches!(ty, MirType::MemoryObject(_)))
            || func.instructions().any(|inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::MemoryObjectLen(..)
                        | InstKind::SetMemoryObjectLen(..)
                        | InstKind::MemoryObjectData(..)
                        | InstKind::MemoryObjectFieldAddr { .. }
                        | InstKind::MemoryObjectElementAddr { .. }
                        | InstKind::Alloc { kind: AllocationKind::Object(_), .. }
                )
            })
    })
}

fn has_instruction(module: &Module, mut predicate: impl FnMut(&InstKind) -> bool) -> bool {
    module
        .functions
        .iter()
        .any(|func| func.instructions().any(|inst_id| predicate(&func.inst(inst_id).kind)))
}

/// Returns whether no semantic intrinsic or aggregate representation remains.
pub(crate) fn intrinsics_are_lowered(module: &Module) -> bool {
    module.functions.iter().all(|func| {
        let signature_is_lowered = func
            .arg_indices()
            .map(|index| func.arg_ty(index))
            .chain(func.returns.iter().copied())
            .all(type_is_lowered);
        let values_are_lowered =
            func.live_values().filter_map(|value| func.value_ty(value)).all(type_is_lowered);
        signature_is_lowered
            && values_are_lowered
            && func.instructions().all(|inst_id| {
                let inst = func.inst(inst_id);
                inst.result_ty.is_none_or(type_is_lowered)
                    && !matches!(
                        inst.kind,
                        InstKind::MappingSlot(..)
                            | InstKind::MappingSlotMemory(..)
                            | InstKind::MappingSlotCalldata(..)
                            | InstKind::Keccak256Bytes(_)
                            | InstKind::AbiEncode { .. }
                            | InstKind::StorageToMemory { .. }
                            | InstKind::MemoryToStorage { .. }
                            | InstKind::ClearStorage { .. }
                            | InstKind::MakeSlice { .. }
                            | InstKind::SlicePtr(_)
                            | InstKind::SliceLen(_)
                            | InstKind::MemoryObjectLen(..)
                            | InstKind::SetMemoryObjectLen(..)
                            | InstKind::MemoryObjectData(..)
                            | InstKind::MemoryObjectFieldAddr { .. }
                            | InstKind::MemoryObjectElementAddr { .. }
                            | InstKind::Alloc { kind: AllocationKind::Object(_), .. }
                    )
            })
    })
}

fn type_is_lowered(ty: MirType) -> bool {
    !matches!(ty, MirType::MemoryObject(_) | MirType::Slice(_))
}
