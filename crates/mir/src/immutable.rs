//! Deployment-time immutable staging layout.
//!
//! Immutable assignments are lowered to ordinary MIR memory stores, while the
//! deployment postlude reads the same words to patch runtime `PUSH<N>`
//! placeholders. This module keeps both sides on one layout.

use crate::{ImmutableEncoding, ImmutableId, Module, TypeSize, memory::EvmMemoryLayout};
use solar_config::OptimizationMode;

/// The highest end of a constant range that moves the immutable staging words: expanding memory
/// to 4 MiB costs about 34 million gas.
const MAX_STAGING_RANGE_END: u64 = 1 << 22;

/// Returns the immediate width used to load an immutable at runtime.
pub fn immutable_push_type_size(
    encoding: ImmutableEncoding,
    optimization: OptimizationMode,
    has_bitwise_shifting: bool,
) -> TypeSize {
    let type_size = encoding.type_size();
    let can_emit_short = has_bitwise_shifting
        || (type_size.bytes() == 1 && !matches!(encoding, ImmutableEncoding::LeftAligned(_)));
    if type_size.bytes() < 32
        && (!can_emit_short
            || (encoding.needs_runtime_normalization()
                && (optimization.is_gas() || type_size.bytes() >= 29)))
    {
        TypeSize::new_int_bits(256)
    } else {
        type_size
    }
}

/// Returns the first byte after every constructor-local slot, and after the local region of each
/// function the constructor runs that accesses it at absolute addresses.
fn constructor_local_memory_end(module: &Module) -> u64 {
    let Some(constructor) = module.constructor() else { return EvmMemoryLayout::HEAP_START };
    let local_end = EvmMemoryLayout::HEAP_START
        .checked_add(module.function(constructor).internal_frame_size)
        .expect("constructor local-memory size overflow");
    module
        .constructor_reachable_functions()
        .iter()
        .filter_map(|func| module.function(func).absolute_local_end())
        .fold(local_end, u64::max)
}

/// Returns the first immutable staging word, above every constructor-owned
/// fixed memory word and clear of every constant range the constructor's code names.
///
/// `lower-immutables` fixes the base on the module, since its stores are themselves absolute
/// accesses that a later computation would count.
pub fn immutable_staging_base(module: &Module) -> u64 {
    module.immutable_staging_base().unwrap_or_else(|| {
        let fixed_end = constructor_local_memory_end(module)
            .max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE);
        let mut base = EvmMemoryLayout::align_word(fixed_end)
            .expect("constructor immutable staging address overflow");
        if module.immutable_count() == 0 {
            return base;
        }
        // A range that ends above `MAX_STAGING_RANGE_END` costs more gas to expand than a
        // transaction can pay, so it runs out of gas when executed and leaves the base where it
        // is. Raising the base to it would make every deployment pay, even when a branch that
        // never runs names the range.
        let ranges = module
            .constructor_reachable_functions()
            .iter()
            .flat_map(|func| module.function(func).constant_memory_ranges_outside_reverts())
            .filter(|&(_, end)| end <= MAX_STAGING_RANGE_END)
            .collect::<Vec<_>>();
        loop {
            let end = immutable_staging_end(base, module.immutable_count());
            let Some(raised) = ranges
                .iter()
                .filter(|&&(start, range_end)| start < end && range_end > base)
                .map(|&(_, range_end)| range_end)
                .max()
            else {
                return base;
            };
            base = raised.next_multiple_of(EvmMemoryLayout::WORD_SIZE);
        }
    })
}

/// Returns the constructor-memory address assigned to an immutable.
pub fn immutable_staging_addr(base: u64, id: ImmutableId) -> u64 {
    let offset = u64::try_from(id.index())
        .ok()
        .and_then(|index| index.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .expect("constructor immutable staging offset overflow");
    base.checked_add(offset).expect("constructor immutable staging address overflow")
}

/// Returns the first constructor-memory address after all immutable words.
pub fn immutable_staging_end(base: u64, count: usize) -> u64 {
    let size = u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .expect("constructor immutable staging size overflow");
    base.checked_add(size).expect("constructor immutable staging end overflow")
}
