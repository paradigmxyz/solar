//! MIR instructions.

use super::{
    Function, InstKind, MemoryObjectKind, MemoryObjectLayout, MirType, SliceLocation, Value,
    ValueId,
};
use alloy_primitives::U256;
use smallvec::{Array, SmallVec};
use solar_interface::Span;
use solar_sema::hir;
use std::fmt;

/// Extra information attached to a MIR instruction by lowering or analysis passes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct InstructionMetadata {
    /// Proven storage alias key for `sload`/`sstore` instructions.
    storage_alias: Option<Box<StorageAlias>>,
    /// Source span that produced this instruction, when the lowerer can preserve it.
    source_span: Span,
    /// Legacy source-map modifier nesting depth for this instruction.
    modifier_depth: u32,
    /// HIR expression that produced this instruction, when the lowerer can preserve it.
    hir_expr: Option<hir::ExprId>,
    /// Loop nesting depth attached by loop-aware analyses.
    pub(crate) loop_depth: u16,
    /// Packed optional memory region, effect kind, and boolean flags.
    flags: MetadataFlags,
}

impl InstructionMetadata {
    /// Empty instruction metadata.
    pub(crate) const EMPTY: Self = Self {
        storage_alias: None,
        hir_expr: None,
        source_span: Span::DUMMY,
        modifier_depth: 0,
        loop_depth: 0,
        flags: MetadataFlags::EMPTY,
    };

    /// Returns the proven storage alias key.
    #[must_use]
    pub(crate) fn storage_alias(&self) -> Option<StorageAlias> {
        self.storage_alias.as_deref().copied()
    }

    /// Sets the proven storage alias key.
    pub(crate) fn set_storage_alias(&mut self, alias: Option<StorageAlias>) {
        self.storage_alias = alias.map(Box::new);
    }

    /// Returns the HIR expression that produced this instruction.
    #[must_use]
    pub(crate) fn hir_expr(&self) -> Option<hir::ExprId> {
        self.hir_expr
    }

    /// Sets the HIR expression that produced this instruction.
    pub(crate) fn set_hir_expr(&mut self, expr: Option<hir::ExprId>) {
        self.hir_expr = expr;
    }

    /// Returns the source span that produced this instruction.
    #[must_use]
    pub(crate) fn source_span(&self) -> Option<Span> {
        (!self.source_span.is_dummy()).then_some(self.source_span)
    }

    /// Sets the source span that produced this instruction.
    pub(crate) fn set_source_span(&mut self, span: Option<Span>) {
        let span = span.filter(|span| !span.is_dummy());
        self.source_span = span.unwrap_or(Span::DUMMY);
        self.flags.set_display_source_span(span.is_some());
        self.flags.set_debug_info_handled();
    }

    /// Sets source debug information without adding it to canonical MIR text.
    pub(crate) fn set_debug_source_span(&mut self, span: Option<Span>) {
        self.source_span = span.filter(|span| !span.is_dummy()).unwrap_or(Span::DUMMY);
        self.flags.set_debug_info_handled();
    }

    /// Returns the source-map modifier nesting depth for this instruction.
    #[must_use]
    pub(crate) const fn modifier_depth(&self) -> u32 {
        self.modifier_depth
    }

    /// Sets the source-map modifier nesting depth for this instruction.
    pub(crate) fn set_modifier_depth(&mut self, depth: u32) {
        self.modifier_depth = depth;
        self.flags.set_debug_info_handled();
    }

    /// Marks this instruction as intentionally having no source location.
    pub(crate) fn mark_debug_info_dropped(&mut self) {
        self.set_debug_source_span(None);
    }

    /// Returns whether source debug information was preserved or intentionally dropped.
    #[must_use]
    pub(crate) fn debug_info_is_handled(&self) -> bool {
        self.flags.debug_info_is_handled()
    }

    /// Returns whether canonical MIR text should include the source span.
    #[must_use]
    pub(crate) fn displays_source_span(&self) -> bool {
        self.flags.displays_source_span()
    }

    /// Returns the proven memory region.
    #[must_use]
    pub(crate) fn memory_region(&self) -> Option<MemoryRegion> {
        self.flags.memory_region()
    }

    /// Sets the proven memory region.
    pub(crate) fn set_memory_region(&mut self, region: Option<MemoryRegion>) {
        self.flags.set_memory_region(region);
    }

    /// Returns whether this instruction was lowered from an unchecked arithmetic context.
    #[must_use]
    pub(crate) fn unchecked(&self) -> bool {
        self.flags.unchecked()
    }

    /// Sets whether this instruction was lowered from an unchecked arithmetic context.
    pub(crate) fn set_unchecked(&mut self, unchecked: bool) {
        self.flags.set_unchecked(unchecked);
    }

    /// Returns the conservative effect classification attached by lowering or analysis.
    #[must_use]
    pub(crate) fn effect(&self) -> Option<EffectKind> {
        self.flags.effect()
    }

    /// Sets the conservative effect classification attached by lowering or analysis.
    pub(crate) fn set_effect(&mut self, effect: Option<EffectKind>) {
        self.flags.set_effect(effect);
    }

    /// Returns whether final placement of this allocation is deferred to the backend.
    #[must_use]
    pub(crate) fn deferred_alloc(&self) -> bool {
        self.flags.deferred_alloc()
    }

    /// Defers final placement of this allocation to the backend.
    pub(crate) fn set_deferred_alloc(&mut self) {
        self.flags.set_deferred_alloc();
    }

    /// Clears the deferred-allocation marker after an allocation is rewritten.
    pub(crate) fn clear_deferred_alloc(&mut self) {
        self.flags.clear_deferred_alloc();
    }

    /// Returns whether this instruction must survive optimization until ABI lowering.
    #[must_use]
    pub(crate) fn abi_validation(&self) -> bool {
        self.flags.abi_validation()
    }

    /// Marks this instruction as an ABI validation dependency.
    pub(crate) fn set_abi_validation(&mut self, value: bool) {
        self.flags.set_abi_validation(value);
    }

    /// Returns whether removing this allocation's FMP bump would change Solidity-visible state.
    #[must_use]
    pub(crate) fn preserves_fmp(&self) -> bool {
        self.flags.preserves_fmp()
    }

    /// Marks an allocation whose FMP bump is observable by Solidity source semantics.
    pub(crate) fn set_preserves_fmp(&mut self, value: bool) {
        self.flags.set_preserves_fmp(value);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct MetadataFlags(u16);

impl MetadataFlags {
    const EMPTY: Self = Self(0);
    const MEMORY_MASK: u16 = 0b0000_0111;
    const EFFECT_MASK: u16 = 0b0111_1000;
    const EFFECT_SHIFT: u16 = 3;
    const UNCHECKED: u16 = 0b1000_0000;
    const DEFERRED_ALLOC: u16 = 0b1_0000_0000;
    const ABI_VALIDATION: u16 = 0b10_0000_0000;
    const PRESERVES_FMP: u16 = 0b100_0000_0000;
    const DISPLAY_SOURCE_SPAN: u16 = 0b1000_0000_0000;
    const DEBUG_INFO_HANDLED: u16 = 0b1_0000_0000_0000;

    fn memory_region(self) -> Option<MemoryRegion> {
        match self.0 & Self::MEMORY_MASK {
            0 => None,
            1 => Some(MemoryRegion::Scratch),
            2 => Some(MemoryRegion::AbiReturn),
            3 => Some(MemoryRegion::Heap),
            4 => Some(MemoryRegion::InternalFrame),
            5 => Some(MemoryRegion::Unknown),
            _ => unreachable!("invalid packed memory region"),
        }
    }

    fn set_memory_region(&mut self, region: Option<MemoryRegion>) {
        let bits = match region {
            None => 0,
            Some(MemoryRegion::Scratch) => 1,
            Some(MemoryRegion::AbiReturn) => 2,
            Some(MemoryRegion::Heap) => 3,
            Some(MemoryRegion::InternalFrame) => 4,
            Some(MemoryRegion::Unknown) => 5,
        };
        self.0 = (self.0 & !Self::MEMORY_MASK) | bits;
    }

    fn unchecked(self) -> bool {
        self.0 & Self::UNCHECKED != 0
    }

    fn set_unchecked(&mut self, unchecked: bool) {
        if unchecked {
            self.0 |= Self::UNCHECKED;
        } else {
            self.0 &= !Self::UNCHECKED;
        }
    }

    fn deferred_alloc(self) -> bool {
        self.0 & Self::DEFERRED_ALLOC != 0
    }

    fn set_deferred_alloc(&mut self) {
        self.0 |= Self::DEFERRED_ALLOC;
    }

    fn clear_deferred_alloc(&mut self) {
        self.0 &= !Self::DEFERRED_ALLOC;
    }

    fn abi_validation(self) -> bool {
        self.0 & Self::ABI_VALIDATION != 0
    }

    fn set_abi_validation(&mut self, value: bool) {
        if value {
            self.0 |= Self::ABI_VALIDATION;
        } else {
            self.0 &= !Self::ABI_VALIDATION;
        }
    }

    fn preserves_fmp(self) -> bool {
        self.0 & Self::PRESERVES_FMP != 0
    }

    fn set_preserves_fmp(&mut self, value: bool) {
        if value {
            self.0 |= Self::PRESERVES_FMP;
        } else {
            self.0 &= !Self::PRESERVES_FMP;
        }
    }

    fn displays_source_span(self) -> bool {
        self.0 & Self::DISPLAY_SOURCE_SPAN != 0
    }

    fn set_display_source_span(&mut self, display: bool) {
        if display {
            self.0 |= Self::DISPLAY_SOURCE_SPAN;
        } else {
            self.0 &= !Self::DISPLAY_SOURCE_SPAN;
        }
    }

    fn debug_info_is_handled(self) -> bool {
        self.0 & Self::DEBUG_INFO_HANDLED != 0
    }

    fn set_debug_info_handled(&mut self) {
        self.0 |= Self::DEBUG_INFO_HANDLED;
    }

    fn effect(self) -> Option<EffectKind> {
        match (self.0 & Self::EFFECT_MASK) >> Self::EFFECT_SHIFT {
            0 => None,
            1 => Some(EffectKind::Pure),
            2 => Some(EffectKind::MemoryRead),
            3 => Some(EffectKind::MemoryWrite),
            4 => Some(EffectKind::StorageRead),
            5 => Some(EffectKind::StorageWrite),
            6 => Some(EffectKind::TransientRead),
            7 => Some(EffectKind::TransientWrite),
            8 => Some(EffectKind::EnvironmentRead),
            9 => Some(EffectKind::ExternalCall),
            10 => Some(EffectKind::InternalCall),
            11 => Some(EffectKind::Create),
            12 => Some(EffectKind::Log),
            13 => Some(EffectKind::ImmutableRead),
            14 => Some(EffectKind::ImmutableWrite),
            _ => unreachable!("invalid packed effect kind"),
        }
    }

    fn set_effect(&mut self, effect: Option<EffectKind>) {
        let bits = match effect {
            None => 0,
            Some(EffectKind::Pure) => 1,
            Some(EffectKind::MemoryRead) => 2,
            Some(EffectKind::MemoryWrite) => 3,
            Some(EffectKind::StorageRead) => 4,
            Some(EffectKind::StorageWrite) => 5,
            Some(EffectKind::TransientRead) => 6,
            Some(EffectKind::TransientWrite) => 7,
            Some(EffectKind::EnvironmentRead) => 8,
            Some(EffectKind::ExternalCall) => 9,
            Some(EffectKind::InternalCall) => 10,
            Some(EffectKind::Create) => 11,
            Some(EffectKind::Log) => 12,
            Some(EffectKind::ImmutableRead) => 13,
            Some(EffectKind::ImmutableWrite) => 14,
        } << Self::EFFECT_SHIFT;
        self.0 = (self.0 & !Self::EFFECT_MASK) | bits;
    }
}

/// A conservative storage alias key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StorageAlias {
    /// A known absolute storage slot.
    Slot(U256),
    /// A loop-invariant symbolic slot value.
    Symbolic(ValueId),
    /// A loop-invariant symbolic base plus a known constant offset.
    Offset {
        /// Symbolic base slot.
        base: ValueId,
        /// Constant offset added to the base.
        offset: U256,
    },
}

impl StorageAlias {
    /// Computes a conservative exact storage alias key for `value`.
    #[must_use]
    pub(crate) fn for_value(func: &Function, value: ValueId) -> Self {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256().map_or(Self::Symbolic(value), Self::Slot),
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Add(lhs, rhs) => {
                    if let Some(offset) = Self::immediate_u256(func, rhs) {
                        Self::add_offset(func, lhs, offset)
                    } else if let Some(offset) = Self::immediate_u256(func, lhs) {
                        Self::add_offset(func, rhs, offset)
                    } else {
                        Self::Symbolic(value)
                    }
                }
                InstKind::Sub(lhs, rhs) => {
                    if let Some(offset) = Self::immediate_u256(func, rhs) {
                        Self::add_offset(func, lhs, U256::ZERO.wrapping_sub(offset))
                    } else {
                        Self::Symbolic(value)
                    }
                }
                _ => Self::Symbolic(value),
            },
            Value::Arg(_) | Value::Undef(_) | Value::Error(_) => Self::Symbolic(value),
        }
    }

    /// Returns true if two alias keys may refer to the same storage slot.
    #[must_use]
    pub(crate) fn may_alias(self, other: Self) -> bool {
        match (self, other) {
            (Self::Slot(a), Self::Slot(b)) => a == b,
            (
                Self::Offset { base: a, offset: a_offset },
                Self::Offset { base: b, offset: b_offset },
            ) if a == b => a_offset == b_offset,
            (Self::Symbolic(_), Self::Symbolic(_)) => true,
            (Self::Symbolic(a), Self::Offset { base, offset })
            | (Self::Offset { base, offset }, Self::Symbolic(a))
                if a == base =>
            {
                offset.is_zero()
            }
            _ => true,
        }
    }

    /// Returns the symbolic base value, if this alias has one.
    #[must_use]
    pub(crate) const fn symbolic_base(self) -> Option<ValueId> {
        match self {
            Self::Symbolic(value) | Self::Offset { base: value, .. } => Some(value),
            Self::Slot(_) => None,
        }
    }

    /// Returns this alias advanced by a constant slot offset.
    #[must_use]
    pub(crate) fn offset_by(self, offset: U256) -> Self {
        match self {
            Self::Slot(slot) => Self::Slot(slot.wrapping_add(offset)),
            Self::Symbolic(base) if offset.is_zero() => Self::Symbolic(base),
            Self::Symbolic(base) => Self::Offset { base, offset },
            Self::Offset { base, offset: existing } => {
                let offset = existing.wrapping_add(offset);
                if offset.is_zero() { Self::Symbolic(base) } else { Self::Offset { base, offset } }
            }
        }
    }

    fn add_offset(func: &Function, value: ValueId, offset: U256) -> Self {
        Self::for_value(func, value).offset_by(offset)
    }

    fn immediate_u256(func: &Function, value: ValueId) -> Option<U256> {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256(),
            _ => None,
        }
    }
}

/// A coarse memory region understood by MIR analyses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemoryRegion {
    /// Compiler-owned low-memory scratch space.
    Scratch,
    /// External ABI return buffer.
    AbiReturn,
    /// Solidity free-memory heap.
    Heap,
    /// Internal-call frame memory.
    InternalFrame,
    /// Region is known to be memory, but not classified more precisely.
    Unknown,
}

impl MemoryRegion {
    /// Returns the stable textual name used in MIR metadata.
    #[must_use]
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Scratch => "scratch",
            Self::AbiReturn => "abi_return",
            Self::Heap => "heap",
            Self::InternalFrame => "internal_frame",
            Self::Unknown => "unknown",
        }
    }
}

/// Conservative side-effect class for an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EffectKind {
    /// Pure computation.
    Pure,
    /// Memory read.
    MemoryRead,
    /// Memory write.
    MemoryWrite,
    /// Persistent storage read.
    StorageRead,
    /// Persistent storage write.
    StorageWrite,
    /// Transient storage read.
    TransientRead,
    /// Transient storage write.
    TransientWrite,
    /// Read from calldata, code, return data, or block/account environment.
    EnvironmentRead,
    /// External call.
    ExternalCall,
    /// Internal MIR call.
    InternalCall,
    /// Contract creation.
    Create,
    /// Event emission.
    Log,
    /// Read from an immutable.
    ImmutableRead,
    /// Constructor assignment to an immutable.
    ImmutableWrite,
}

impl EffectKind {
    /// Returns the stable textual name used in MIR metadata.
    #[must_use]
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::MemoryRead => "memory_read",
            Self::MemoryWrite => "memory_write",
            Self::StorageRead => "storage_read",
            Self::StorageWrite => "storage_write",
            Self::TransientRead => "transient_read",
            Self::TransientWrite => "transient_write",
            Self::EnvironmentRead => "environment_read",
            Self::ExternalCall => "external_call",
            Self::InternalCall => "internal_call",
            Self::Create => "create",
            Self::Log => "log",
            Self::ImmutableRead => "immutable_read",
            Self::ImmutableWrite => "immutable_write",
        }
    }
}

/// Alignment applied to an abstract heap allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AllocationAlignment {
    /// Reserve exactly the requested byte count.
    Exact,
    /// Round the reservation up to an EVM word.
    Word,
}

/// Initialization performed for a newly reserved range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AllocationInitialization {
    /// Preserve the range's existing bytes until explicitly overwritten.
    Uninitialized,
    /// Initialize every reserved byte to zero.
    Zeroed,
}

/// Failure behavior attached to an allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AllocationFailure {
    /// The producer has already proved the bump valid.
    Infallible,
    /// Revert with the memory-allocation panic when the bump overflows.
    Panic,
}

/// Semantic shape produced by an allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AllocationKind {
    /// Untyped compiler scratch or ABI staging memory.
    Raw,
    /// A Solidity memory object whose layout is owned by the memory model.
    Object(MemoryObjectLayout),
}

/// Storage policy for an ABI-encoded result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiEncodeMode {
    /// Return a heap-backed raw memory slice.
    Slice,
    /// Return an owned Solidity bytes object.
    Bytes,
    /// Return a slice staged at the free-memory pointer without reserving it.
    Scratch,
}

impl AbiEncodeMode {
    /// Returns the MIR result type for this mode.
    #[must_use]
    pub(crate) const fn result_type(self) -> MirType {
        match self {
            Self::Slice | Self::Scratch => MirType::Slice(SliceLocation::Memory),
            Self::Bytes => MirType::MemoryObject(MemoryObjectKind::Bytes),
        }
    }
}

impl AllocationKind {
    /// Returns the MIR result type of this allocation.
    #[must_use]
    pub(crate) const fn result_type(self) -> MirType {
        match self {
            Self::Raw => MirType::MemPtr,
            Self::Object(layout) => MirType::MemoryObject(layout.kind()),
        }
    }
}

/// Semantic allocation policy carried by [`InstKind::Alloc`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AllocationSemantics {
    /// Requested alignment.
    pub alignment: AllocationAlignment,
    /// Requested initialization.
    pub initialization: AllocationInitialization,
    /// Requested failure behavior.
    pub failure: AllocationFailure,
}

impl AllocationSemantics {
    /// Exact-size, uninitialized allocation whose validity is already proven.
    pub(crate) const INTERNAL: Self = Self {
        alignment: AllocationAlignment::Exact,
        initialization: AllocationInitialization::Uninitialized,
        failure: AllocationFailure::Infallible,
    };

    /// Checked and zero-initialized Solidity object allocation.
    ///
    /// Object lowering includes the header and padding in `size`, so the
    /// allocation must preserve that already-aligned extent exactly.
    pub(crate) const SOLIDITY_ZEROED: Self = Self {
        alignment: AllocationAlignment::Exact,
        initialization: AllocationInitialization::Zeroed,
        failure: AllocationFailure::Panic,
    };

    /// Checked, exact-size allocation for objects initialized by their
    /// producer rather than by the allocator.
    pub(crate) const SOLIDITY_UNINITIALIZED: Self = Self {
        alignment: AllocationAlignment::Exact,
        initialization: AllocationInitialization::Uninitialized,
        failure: AllocationFailure::Panic,
    };
}

/// An instruction in the MIR.
#[derive(Clone, Debug)]
pub(crate) struct Instruction {
    /// The kind of instruction.
    pub(crate) kind: InstKind,
    /// The result type (if any).
    pub(crate) result_ty: Option<MirType>,
    /// The value allocated for this instruction's result.
    result: Option<ValueId>,
    /// Metadata produced by lowering or analysis.
    pub(crate) metadata: InstructionMetadata,
}

impl Instruction {
    /// Creates a new instruction.
    #[must_use]
    pub(crate) const fn new(kind: InstKind, result_ty: Option<MirType>) -> Self {
        Self { kind, result_ty, result: None, metadata: InstructionMetadata::EMPTY }
    }

    /// Marks this synthetic instruction as intentionally having no source location.
    #[must_use]
    pub(crate) fn with_debug_info_dropped(mut self) -> Self {
        self.metadata.mark_debug_info_dropped();
        self
    }

    /// Returns the value allocated for this instruction's result.
    #[must_use]
    pub(super) const fn result(&self) -> Option<ValueId> {
        self.result
    }

    /// Replaces the value allocated for this instruction's result.
    pub(super) fn set_result(&mut self, result: Option<ValueId>) -> Option<ValueId> {
        std::mem::replace(&mut self.result, result)
    }

    /// Returns the operands of this instruction.
    #[must_use]
    pub(crate) fn operands(&self) -> SmallVec<[ValueId; 8]> {
        self.kind.operands()
    }
}

// Operation-specific analysis helpers remain next to `Instruction`, while the
// complete `InstKind` declaration and its generated metadata live in `op_schema`.
impl InstKind {
    /// Returns binary operands whose evaluation order may be exchanged during EVM lowering.
    ///
    /// This includes commutative instructions and comparisons whose opcode can be reversed with
    /// their operands.
    pub(crate) const fn reorderable_binary_operands(&self) -> Option<(ValueId, ValueId)> {
        if !self.op_def().traits.contains(super::OpTraits::REORDERABLE) {
            return None;
        }
        match self {
            Self::DataCopy(_, a, b)
            | Self::Add(a, b)
            | Self::Mul(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b)
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::Gt(a, b)
            | Self::SLt(a, b)
            | Self::SGt(a, b) => Some((*a, *b)),
            _ => None,
        }
    }

    /// Collects all operands of this instruction into the provided vector.
    /// This is the canonical way to get all operands for liveness analysis.
    pub(crate) fn collect_operands<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        match self {
            // Binary operations
            Self::DataCopy(_, a, b)
            | Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::SDiv(a, b)
            | Self::Mod(a, b)
            | Self::SMod(a, b)
            | Self::Exp(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b)
            | Self::Shl(a, b)
            | Self::Shr(a, b)
            | Self::Sar(a, b)
            | Self::Byte(a, b)
            | Self::Lt(a, b)
            | Self::Gt(a, b)
            | Self::SLt(a, b)
            | Self::SGt(a, b)
            | Self::Eq(a, b)
            | Self::MStore(a, b)
            | Self::MStore8(a, b)
            | Self::MemoryZero(a, b)
            | Self::SStore(a, b)
            | Self::TStore(a, b)
            | Self::Keccak256(a, b)
            | Self::MappingSlot(a, b)
            | Self::MappingSlotMemory(a, b)
            | Self::MappingSlotCalldata(a, b)
            | Self::StorageArrayElementSlot { slot: a, index: b, .. }
            | Self::Log0(a, b)
            | Self::SignExtend(a, b) => {
                out.push(*a);
                out.push(*b);
            }

            Self::MakeSlice { ptr, len, .. } => {
                out.push(*ptr);
                out.push(*len);
            }

            Self::FrameStore { value, .. } => out.push(*value),

            Self::SetMemoryObjectLen(object, len, _)
            | Self::MemoryObjectElementAddr { object, index: len, .. }
            | Self::MemoryObjectLoadElement { object, index: len, .. }
            | Self::MemoryObjectLoadByte { object, index: len } => {
                out.push(*object);
                out.push(*len);
            }

            Self::MemoryObjectStoreField { object, value, .. } => {
                out.push(*object);
                out.push(*value);
            }

            Self::MemoryObjectStoreElement { object, index, value, .. } => {
                out.push(*object);
                out.push(*index);
                out.push(*value);
            }

            Self::MemoryObjectStoreByte { object, index, value } => {
                out.push(*object);
                out.push(*index);
                out.push(*value);
            }

            Self::MemoryObjectStoreWord { object, offset, value } => {
                out.push(*object);
                out.push(*offset);
                out.push(*value);
            }

            Self::MemorySliceLoadWord { slice, offset } => {
                out.push(*slice);
                out.push(*offset);
            }

            Self::CalldataSliceLoadWord { slice, offset } => {
                out.push(*slice);
                out.push(*offset);
            }

            Self::MemoryObjectCopyFromSlice { object, source, .. } => {
                out.push(*object);
                out.push(*source);
            }

            Self::MemoryObjectCopyFromSliceAt { object, offset, source, .. } => {
                out.push(*object);
                out.push(*offset);
                out.push(*source);
            }

            Self::MemoryObjectCopy { destination, source, length, .. } => {
                out.push(*destination);
                out.push(*source);
                out.push(*length);
            }

            Self::StorageToMemory { storage, memory, .. }
            | Self::MemoryToStorage { memory, storage, .. } => {
                out.push(*storage);
                out.push(*memory);
            }

            Self::AbiEncode { selector, args, .. } => {
                out.extend(selector.iter().chain(args).copied());
            }

            Self::AbiDecode { data, .. } => out.push(*data),

            // Unary operations
            Self::Not(a)
            | Self::Clz(a)
            | Self::IsZero(a)
            | Self::MLoad(a)
            | Self::SetFmp(a)
            | Self::SLoad(a)
            | Self::TLoad(a)
            | Self::CalldataLoad(a)
            | Self::ExtCodeSize(a)
            | Self::ExtCodeHash(a)
            | Self::Balance(a)
            | Self::BlockHash(a)
            | Self::BlobHash(a)
            | Self::StoreImmutable(_, a)
            | Self::Keccak256Bytes(a)
            | Self::StorageArrayDataSlot(a)
            | Self::MemoryObjectLen(a, _)
            | Self::MemoryObjectData(a, _)
            | Self::MemoryObjectFieldAddr { object: a, .. }
            | Self::MemoryObjectLoadField { object: a, .. } => {
                out.push(*a);
            }

            Self::Alloc { size, .. } => out.push(*size),

            Self::ClearStorage { storage, .. } => out.push(*storage),

            Self::SlicePtr(slice) | Self::SliceLen(slice) => out.push(*slice),

            // Ternary operations
            Self::MCopy(a, b, c)
            | Self::CalldataCopy(a, b, c)
            | Self::CodeCopy(a, b, c)
            | Self::ReturnDataCopy(a, b, c)
            | Self::AddMod(a, b, c)
            | Self::MulMod(a, b, c)
            | Self::Create(a, b, c)
            | Self::Log1(a, b, c)
            | Self::Select(a, b, c) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
            }

            // 4-operand operations
            Self::ExtCodeCopy(a, b, c, d) | Self::Create2(a, b, c, d) | Self::Log2(a, b, c, d) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
                out.push(*d);
            }

            // 5-operand operations
            Self::Log3(a, b, c, d, e) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
                out.push(*d);
                out.push(*e);
            }

            // 6-operand operations
            Self::Log4(a, b, c, d, e, f) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
                out.push(*d);
                out.push(*e);
                out.push(*f);
            }

            // Call operations
            Self::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size }
            | Self::CallCode { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                out.push(*gas);
                out.push(*addr);
                out.push(*value);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*ret_offset);
                out.push(*ret_size);
            }
            Self::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                out.push(*gas);
                out.push(*addr);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*ret_offset);
                out.push(*ret_size);
            }
            Self::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                out.push(*gas);
                out.push(*addr);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*ret_offset);
                out.push(*ret_size);
            }
            Self::ExtCall { addr, args_offset, args_size, value } => {
                out.push(*addr);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*value);
            }
            Self::ExtDelegateCall { addr, args_offset, args_size }
            | Self::ExtStaticCall { addr, args_offset, args_size } => {
                out.push(*addr);
                out.push(*args_offset);
                out.push(*args_size);
            }
            Self::InternalCall { args, .. } => {
                out.extend(args.iter().copied());
            }

            // Phi node - operands are the incoming values
            Self::Phi(incoming) => {
                for (_, val) in incoming {
                    out.push(*val);
                }
            }

            // Nullary operations - no operands
            Self::MSize
            | Self::Fmp
            | Self::CalldataSize
            | Self::InternalFrameAddr(_)
            | Self::FrameLoad { .. }
            | Self::ConstructorArgsBase
            | Self::ConstructorArgsEnd
            | Self::CodeSize
            | Self::LoadImmutable(_)
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::SlotNum
            | Self::ChainId
            | Self::Address
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee => {}
        }
    }

    /// Returns the operands of this instruction.
    #[must_use]
    pub(crate) fn operands(&self) -> SmallVec<[ValueId; 8]> {
        let mut out = SmallVec::new();
        self.collect_operands(&mut out);
        out
    }

    /// Visits every operand mutably.
    pub(crate) fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
        match self {
            Self::DataCopy(_, a, b)
            | Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::SDiv(a, b)
            | Self::Mod(a, b)
            | Self::SMod(a, b)
            | Self::Exp(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b)
            | Self::Shl(a, b)
            | Self::Shr(a, b)
            | Self::Sar(a, b)
            | Self::Byte(a, b)
            | Self::Lt(a, b)
            | Self::Gt(a, b)
            | Self::SLt(a, b)
            | Self::SGt(a, b)
            | Self::Eq(a, b)
            | Self::MStore(a, b)
            | Self::MStore8(a, b)
            | Self::MemoryZero(a, b)
            | Self::SStore(a, b)
            | Self::TStore(a, b)
            | Self::Keccak256(a, b)
            | Self::MappingSlot(a, b)
            | Self::MappingSlotMemory(a, b)
            | Self::MappingSlotCalldata(a, b)
            | Self::StorageArrayElementSlot { slot: a, index: b, .. }
            | Self::Log0(a, b)
            | Self::SignExtend(a, b) => {
                f(a);
                f(b);
            }

            Self::MakeSlice { ptr, len, .. } => {
                f(ptr);
                f(len);
            }

            Self::FrameStore { value, .. } => f(value),

            Self::SetMemoryObjectLen(object, len, _)
            | Self::MemoryObjectElementAddr { object, index: len, .. }
            | Self::MemoryObjectLoadElement { object, index: len, .. }
            | Self::MemoryObjectLoadByte { object, index: len } => {
                f(object);
                f(len);
            }

            Self::MemoryObjectStoreField { object, value, .. } => {
                f(object);
                f(value);
            }

            Self::MemoryObjectStoreElement { object, index, value, .. } => {
                f(object);
                f(index);
                f(value);
            }

            Self::MemoryObjectStoreByte { object, index, value } => {
                f(object);
                f(index);
                f(value);
            }

            Self::MemoryObjectStoreWord { object, offset, value } => {
                f(object);
                f(offset);
                f(value);
            }

            Self::MemorySliceLoadWord { slice, offset } => {
                f(slice);
                f(offset);
            }

            Self::CalldataSliceLoadWord { slice, offset } => {
                f(slice);
                f(offset);
            }

            Self::MemoryObjectCopyFromSlice { object, source, .. } => {
                f(object);
                f(source);
            }

            Self::MemoryObjectCopyFromSliceAt { object, offset, source, .. } => {
                f(object);
                f(offset);
                f(source);
            }

            Self::MemoryObjectCopy { destination, source, length, .. } => {
                f(destination);
                f(source);
                f(length);
            }

            Self::StorageToMemory { storage, memory, .. }
            | Self::MemoryToStorage { memory, storage, .. } => {
                f(storage);
                f(memory);
            }

            Self::AbiEncode { selector, args, .. } => {
                if let Some(selector) = selector {
                    f(selector);
                }
                for arg in args {
                    f(arg);
                }
            }

            Self::AbiDecode { data, .. } => f(data),

            Self::Not(a)
            | Self::Clz(a)
            | Self::IsZero(a)
            | Self::MLoad(a)
            | Self::SetFmp(a)
            | Self::SLoad(a)
            | Self::TLoad(a)
            | Self::CalldataLoad(a)
            | Self::ExtCodeSize(a)
            | Self::ExtCodeHash(a)
            | Self::Balance(a)
            | Self::BlockHash(a)
            | Self::BlobHash(a)
            | Self::StoreImmutable(_, a)
            | Self::SlicePtr(a)
            | Self::Keccak256Bytes(a)
            | Self::StorageArrayDataSlot(a)
            | Self::SliceLen(a)
            | Self::MemoryObjectLen(a, _)
            | Self::MemoryObjectData(a, _)
            | Self::MemoryObjectFieldAddr { object: a, .. }
            | Self::MemoryObjectLoadField { object: a, .. } => f(a),

            Self::Alloc { size, .. } => f(size),

            Self::ClearStorage { storage, .. } => f(storage),

            Self::MCopy(a, b, c)
            | Self::CalldataCopy(a, b, c)
            | Self::CodeCopy(a, b, c)
            | Self::ReturnDataCopy(a, b, c)
            | Self::AddMod(a, b, c)
            | Self::MulMod(a, b, c)
            | Self::Create(a, b, c)
            | Self::Log1(a, b, c)
            | Self::Select(a, b, c) => {
                f(a);
                f(b);
                f(c);
            }

            Self::ExtCodeCopy(a, b, c, d) | Self::Create2(a, b, c, d) | Self::Log2(a, b, c, d) => {
                f(a);
                f(b);
                f(c);
                f(d);
            }

            Self::Log3(a, b, c, d, e) => {
                f(a);
                f(b);
                f(c);
                f(d);
                f(e);
            }

            Self::Log4(a, b, c, d, e, g) => {
                f(a);
                f(b);
                f(c);
                f(d);
                f(e);
                f(g);
            }

            Self::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size }
            | Self::CallCode { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                f(gas);
                f(addr);
                f(value);
                f(args_offset);
                f(args_size);
                f(ret_offset);
                f(ret_size);
            }
            Self::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size }
            | Self::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                f(gas);
                f(addr);
                f(args_offset);
                f(args_size);
                f(ret_offset);
                f(ret_size);
            }
            Self::ExtCall { addr, args_offset, args_size, value } => {
                f(addr);
                f(args_offset);
                f(args_size);
                f(value);
            }
            Self::ExtDelegateCall { addr, args_offset, args_size }
            | Self::ExtStaticCall { addr, args_offset, args_size } => {
                f(addr);
                f(args_offset);
                f(args_size);
            }
            Self::InternalCall { args, .. } => {
                for arg in args {
                    f(arg);
                }
            }

            Self::Phi(incoming) => {
                for (_, value) in incoming {
                    f(value);
                }
            }

            Self::MSize
            | Self::Fmp
            | Self::CalldataSize
            | Self::InternalFrameAddr(_)
            | Self::FrameLoad { .. }
            | Self::ConstructorArgsBase
            | Self::ConstructorArgsEnd
            | Self::CodeSize
            | Self::LoadImmutable(_)
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::SlotNum
            | Self::ChainId
            | Self::Address
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee => {}
        }
    }

    /// Returns the mnemonic for this instruction.
    #[must_use]
    pub(crate) const fn mnemonic(&self) -> &'static str {
        self.op_def().mnemonic
    }

    /// Returns true if this instruction has side effects.
    /// Side-effect instructions must not be eliminated by DCE.
    #[must_use]
    pub(crate) const fn has_side_effects(&self) -> bool {
        self.op_def().has_side_effects
    }

    /// Returns whether this instruction still carries a semantic memory-object operation.
    #[must_use]
    pub(crate) const fn is_memory_object_op(&self) -> bool {
        matches!(self, Self::Alloc { kind: AllocationKind::Object(_), .. })
            || self.op_def().traits.contains(super::OpTraits::MEMORY_OBJECT)
    }

    /// Returns a conservative effect classification for this instruction.
    #[must_use]
    pub(crate) const fn effect_kind(&self) -> EffectKind {
        self.op_def().effect
    }

    /// Returns whether this is a stable, nullary environment read that is cheap
    /// enough to rematerialize at every use.
    ///
    /// `BlockNumber` is deliberately excluded: instrumented EVMs can update it
    /// across a call, so its MIR value must preserve the original evaluation.
    #[must_use]
    pub(crate) const fn is_always_rematerializable(&self) -> bool {
        self.op_def().traits.contains(super::OpTraits::REMATERIALIZABLE)
    }
}

impl fmt::Display for InstKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.mnemonic())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{BlockId, Function, Immediate, Value};
    use alloy_primitives::U256;
    use solar_interface::Ident;

    #[test]
    fn phi_operands_include_incoming_values() {
        let mut func = Function::new(Ident::DUMMY);
        let pred_a = BlockId::ENTRY;
        let pred_b = func.alloc_block();
        let a = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let b = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(2))));

        let phi = InstKind::Phi(vec![(pred_a, a), (pred_b, b)]);

        assert_eq!(phi.operands().as_slice(), &[a, b]);
    }

    #[test]
    #[cfg_attr(not(target_pointer_width = "64"), ignore = "64-bit only")]
    #[cfg_attr(feature = "nightly", ignore = "stable only")]
    fn instruction_layout_sizes() {
        use snapbox::{assert_data_eq, str};

        #[track_caller]
        fn assert_size<T>(size: impl snapbox::IntoData) {
            assert_size_(std::mem::size_of::<T>(), size.into_data());
        }

        #[track_caller]
        fn assert_size_(actual: usize, expected: snapbox::Data) {
            assert_data_eq!(actual.to_string(), expected);
        }

        assert_size::<InstKind>(str!["40"]);
        assert_size::<InstructionMetadata>(str!["32"]);
        assert_size::<Instruction>(str!["80"]);
    }
}
