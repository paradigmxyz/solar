//! Determines whether an artifact needs its initial free-memory-pointer word.
//!
//! Every explicit instruction read and MSIZE observation retains the ordinary initialization
//! requirement. A RETURN or REVERT that can read the word also requires initialization unless
//! constant MSTORE/MSTORE8 operations in that same block have already defined all 32 bytes.
//! Coverage uses a byte mask and stops at internal calls; it does not propagate across blocks or
//! infer definite writes from conservative memory-effect summaries. Unknown writes establish no
//! coverage. Reachable callees are checked separately, and dynamic frame protocols independently
//! force initialization in the caller. A separate bounded query may select one static runtime
//! tail entry for the existing initializer, rejecting observation and reentry rather than
//! constructing a general dominance or memory dataflow analysis. Both queries emit no IR.

use super::FunctionLayout;
use crate::{
    analysis::{AliasAnalysis, CallGraphInfo, ModRef},
    backend::evm::{
        spills,
        storage::{FrameAddress, FrameBase, FunctionStorage, ModulePlan},
    },
    memory::EvmMemoryLayout,
    mir,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

pub(super) fn requires_fmp(
    function: &mir::Function,
    alias: &AliasAnalysis,
    storage: &FunctionStorage,
    fixed_memory_end: u64,
) -> bool {
    let reads_fmp = |effects: ModRef| {
        effects.observes_memory_size()
            || spills::accesses_overlap(
                storage,
                fixed_memory_end,
                effects.reads(),
                FrameAddress::Absolute(EvmMemoryLayout::FMP_SLOT),
            )
    };
    function
        .instructions()
        .filter(|&inst| !matches!(function.inst(inst).kind, mir::InstKind::InternalCall { .. }))
        .map(|inst| alias.instruction_mod_ref(function, inst))
        .any(&reads_fmp)
        || function.blocks.iter().any(|block| {
            let Some(terminator) = &block.terminator else { return false };
            !matches!(terminator, mir::Terminator::TailCall { .. })
                && reads_fmp(alias.terminator_mod_ref(function, terminator))
                && !defines_fmp(function, &block.instructions)
        })
}

fn defines_fmp(function: &mir::Function, instructions: &[mir::InstId]) -> bool {
    let mut defined = 0;
    for &inst in instructions.iter().rev() {
        let (address, width) = match function.inst(inst).kind {
            mir::InstKind::MStore(address, _) => (address, 32),
            mir::InstKind::MStore8(address, _) => (address, 1),
            mir::InstKind::InternalCall { .. } => break,
            _ => continue,
        };
        if let Some(address) = function.value_u64(address) {
            defined |= written_bytes(address, width);
            if defined == u32::MAX {
                return true;
            }
        }
    }
    false
}

/// Selects one static runtime tail entry that contains every initial-FMP demand.
/// Returns none for anything beyond a memory-free dispatcher and one demanding closure.
pub(super) fn unique_frontier(
    module: &mir::Module,
    root: mir::FunctionId,
    plan: &ModulePlan,
    layouts: &FxHashMap<mir::FunctionId, FunctionLayout>,
    calls: &CallGraphInfo,
) -> Option<mir::FunctionId> {
    let dispatcher = module.function(root);
    if module.dispatch_entry() != Some(root)
        || dispatcher.attributes.is_constructor
        || !dispatcher.params.is_empty()
        || dispatcher
            .live_values()
            .any(|value| matches!(dispatcher.value(value), mir::Value::Arg(_)))
        || layouts[&root].returning
        || !layouts[&root].spills.homes.is_empty()
        || !matches!(plan.functions[root].base, FrameBase::Static(_))
        || dispatcher.instructions().any(|inst| {
            !matches!(
                dispatcher.inst(inst).kind,
                mir::InstKind::CallValue
                    | mir::InstKind::CalldataLoad(_)
                    | mir::InstKind::CalldataSize
                    | mir::InstKind::Shr(..)
                    | mir::InstKind::And(..)
                    | mir::InstKind::Eq(..)
                    | mir::InstKind::Lt(..)
                    | mir::InstKind::Gt(..)
                    | mir::InstKind::IsZero(_)
            )
        })
    {
        return None;
    }
    let mut targets = DenseBitSet::new_empty(module.functions.len());
    for block in &dispatcher.blocks {
        match block.terminator.as_ref()? {
            mir::Terminator::TailCall { function, args } if args.is_empty() => {
                targets.insert(*function);
            }
            mir::Terminator::Jump(_)
            | mir::Terminator::Branch { .. }
            | mir::Terminator::Switch { .. }
            | mir::Terminator::Stop => {}
            mir::Terminator::Revert { size, .. } if dispatcher.value_u64(*size) == Some(0) => {}
            _ => return None,
        }
    }
    let mut frontier = None;
    for target in targets.iter() {
        let mut closure = calls.reachable_callees_from([target]);
        closure.insert(target);
        if closure.iter().any(|id| {
            let storage = &plan.functions[id];
            (storage.base == FrameBase::Dynamic && !storage.stack_arguments)
                || requires_fmp(
                    module.function(id),
                    &layouts[&id].alias,
                    storage,
                    plan.fixed_memory_end,
                )
        }) && frontier.replace(target).is_some()
        {
            return None;
        }
    }
    let frontier = frontier?;
    if frontier == root
        || layouts[&frontier].returning
        || module.function(frontier).attributes.is_constructor
        || !module.function(frontier).params.is_empty()
        || !matches!(plan.functions[frontier].base, FrameBase::Static(_))
    {
        return None;
    }
    // Only dispatcher tail edges can enter the frontier. No emitted function can reenter root.
    for &id in layouts.keys() {
        let function = module.function(id);
        for inst in function.instructions() {
            if match function.inst(inst).kind {
                mir::InstKind::InternalCall { function, .. } => {
                    function == frontier || function == root
                }
                mir::InstKind::Gas
                | mir::InstKind::CodeSize
                | mir::InstKind::CodeCopy(..)
                | mir::InstKind::ExtCodeSize(_)
                | mir::InstKind::ExtCodeCopy(..)
                | mir::InstKind::ExtCodeHash(_)
                | mir::InstKind::DataCopy(..)
                | mir::InstKind::Call { .. }
                | mir::InstKind::CallCode { .. }
                | mir::InstKind::StaticCall { .. }
                | mir::InstKind::DelegateCall { .. }
                | mir::InstKind::ExtCall { .. }
                | mir::InstKind::ExtDelegateCall { .. }
                | mir::InstKind::ExtStaticCall { .. }
                | mir::InstKind::Create(..)
                | mir::InstKind::Create2(..) => true,
                _ => false,
            } {
                return None;
            }
        }
        if function.blocks.iter().any(|block| {
            matches!(&block.terminator,
            Some(mir::Terminator::TailCall { function, .. })
                if *function == root || (*function == frontier && id != root))
        }) {
            return None;
        }
    }
    Some(frontier)
}

/// Marks the FMP bytes defined by one successful, absolute primitive store.
fn written_bytes(address: u64, width: u64) -> u32 {
    let low = EvmMemoryLayout::FMP_SLOT;
    let high = low + EvmMemoryLayout::WORD_SIZE;
    let start = address.clamp(low, high) - low;
    let end = address.saturating_add(width).clamp(low, high) - low;
    ((1u64 << end) - (1u64 << start)) as u32
}

#[cfg(test)]
mod tests {
    use super::written_bytes;

    #[test]
    fn primitive_store_coverage_matches_each_byte() {
        for address in (0..160).chain([u64::MAX - 32, u64::MAX - 1, u64::MAX]) {
            for width in [1, 32] {
                let mask = written_bytes(address, width);
                for byte in 0u32..32 {
                    let location = 64 + u128::from(byte);
                    let covered = u128::from(address) <= location
                        && location < u128::from(address) + u128::from(width);
                    assert_eq!(mask & (1 << byte) != 0, covered, "{address}+{width}: {byte}");
                }
            }
        }
        assert_eq!(written_bytes(36, 32) | written_bytes(68, 32), u32::MAX);
        assert_eq!(written_bytes(64, 1), 1);
        assert_ne!(written_bytes(36, 32), u32::MAX);
    }
}
