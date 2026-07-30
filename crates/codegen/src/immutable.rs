//! Deployment-time immutable staging layout.
//!
//! Immutable assignments are lowered to ordinary MIR memory stores, while the
//! deployment postlude reads the same words to patch runtime `PUSH<N>`
//! placeholders. This module keeps both sides on one layout.

use crate::{
    memory::EvmMemoryLayout,
    mir::{ImmutableEncoding, ImmutableId, Module, TypeSize},
};
use solar_config::OptimizationMode;

/// Returns the immediate width used to load an immutable at runtime.
pub(crate) fn immutable_push_type_size(
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

/// Returns the first byte after every constructor-local slot.
fn constructor_local_memory_end(module: &Module) -> u64 {
    module.functions.iter().find(|func| func.attributes.is_constructor).map_or(
        EvmMemoryLayout::HEAP_START,
        |func| {
            EvmMemoryLayout::HEAP_START
                .checked_add(func.internal_frame_size)
                .expect("constructor local-memory size overflow")
        },
    )
}

/// Returns the first immutable staging word, above every constructor-owned
/// fixed memory word.
pub(crate) fn immutable_staging_base(module: &Module) -> u64 {
    let fixed_end = constructor_local_memory_end(module)
        .max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE);
    EvmMemoryLayout::align_word(fixed_end).expect("constructor immutable staging address overflow")
}

/// Returns the constructor-memory address assigned to an immutable.
pub(crate) fn immutable_staging_addr(base: u64, id: ImmutableId) -> u64 {
    let offset = u64::try_from(id.index())
        .ok()
        .and_then(|index| index.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .expect("constructor immutable staging offset overflow");
    base.checked_add(offset).expect("constructor immutable staging address overflow")
}

/// Returns the first constructor-memory address after all immutable words.
pub(crate) fn immutable_staging_end(base: u64, count: usize) -> u64 {
    let size = u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .expect("constructor immutable staging size overflow");
    base.checked_add(size).expect("constructor immutable staging end overflow")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_width_respects_codegen_objective() {
        let byte = TypeSize::new_int_bits(8);
        let signed = ImmutableEncoding::Signed(byte);
        let unsigned = ImmutableEncoding::Unsigned(byte);
        let fixed = ImmutableEncoding::LeftAligned(byte);
        let int224 = ImmutableEncoding::Signed(TypeSize::new_int_bits(224));
        let int232 = ImmutableEncoding::Signed(TypeSize::new_int_bits(232));
        let bytes28 = ImmutableEncoding::LeftAligned(TypeSize::new_fb_bytes(28));
        let bytes29 = ImmutableEncoding::LeftAligned(TypeSize::new_fb_bytes(29));

        assert_eq!(immutable_push_type_size(unsigned, OptimizationMode::Gas, true).bytes(), 1);
        assert_eq!(immutable_push_type_size(signed, OptimizationMode::Gas, true).bytes(), 32);
        assert_eq!(immutable_push_type_size(signed, OptimizationMode::Size, true).bytes(), 1);
        assert_eq!(immutable_push_type_size(int224, OptimizationMode::Size, true).bytes(), 28);
        assert_eq!(immutable_push_type_size(int232, OptimizationMode::Size, true).bytes(), 32);
        assert_eq!(immutable_push_type_size(bytes28, OptimizationMode::Size, true).bytes(), 28);
        assert_eq!(immutable_push_type_size(bytes29, OptimizationMode::Size, true).bytes(), 32);
        assert_eq!(immutable_push_type_size(unsigned, OptimizationMode::Gas, false).bytes(), 1);
        assert_eq!(immutable_push_type_size(signed, OptimizationMode::Size, false).bytes(), 1);
        assert_eq!(immutable_push_type_size(fixed, OptimizationMode::Size, false).bytes(), 32);
    }
}
