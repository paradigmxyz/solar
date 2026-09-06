//! Determines whether an artifact needs its initial free-memory-pointer word.
//!
//! Every explicit instruction read and MSIZE observation retains the ordinary initialization
//! requirement. A RETURN or REVERT that can read the word also requires initialization unless
//! constant MSTORE/MSTORE8 operations in that same block have already defined all 32 bytes.
//! Coverage uses a byte mask and stops at internal calls; it does not propagate across blocks or
//! infer definite writes from conservative memory-effect summaries. Unknown writes establish no
//! coverage. Reachable callees are checked separately, and dynamic frame protocols independently
//! force initialization in the caller. This query emits no IR and does not move initialization.

use crate::{
    analysis::{AliasAnalysis, ModRef},
    backend::evm::{
        spills,
        storage::{FrameAddress, FunctionStorage},
    },
    memory::EvmMemoryLayout,
    mir,
};

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
