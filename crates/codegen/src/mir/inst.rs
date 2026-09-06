//! MIR instructions.

use super::{
    AbiLayoutRef, AbiParamLayoutRef, BlockId, DataRef, FrameMode, FrameSlotKind, Function,
    FunctionId, ImmutableId, MemoryObjectKind, MemoryObjectLayout, MirType, SliceLocation,
    StorageLayoutRef, Value, ValueId,
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
    /// Additional origins of shared code, allocated only after a debug-origin merge.
    additional_source_spans: Option<Box<SmallVec<[Span; 2]>>>,
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
        additional_source_spans: None,
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

    /// Returns every retained origin, in deterministic merge order.
    pub(crate) fn source_spans(&self) -> impl Iterator<Item = Span> + '_ {
        self.source_span()
            .into_iter()
            .chain(self.additional_source_spans.iter().flat_map(|spans| spans.iter().copied()))
    }

    /// Sets explicit textual origins, bounding metadata growth in repeated merges.
    pub(crate) fn set_source_spans(&mut self, spans: impl IntoIterator<Item = Span>) {
        self.set_source_span(None);
        for span in spans {
            self.insert_source_span(span);
        }
        self.flags.set_display_source_span(self.source_span().is_some());
    }

    fn insert_source_span(&mut self, span: Span) {
        // Keep the same bounded ambiguity representation in both IR layers.
        if span.is_dummy() || self.source_spans().any(|old| old == span) {
            return;
        }
        if self.source_span.is_dummy() {
            self.source_span = span;
        } else if self.source_spans().count() < crate::source_info::MAX_DEBUG_SPANS {
            self.additional_source_spans.get_or_insert_default().push(span);
        }
    }

    /// Unions origins without importing instruction-specific analysis facts.
    pub(crate) fn merge_debug_context(&mut self, other: &Self) {
        for span in other.source_spans() {
            self.insert_source_span(span);
        }
        if self.modifier_depth != other.modifier_depth {
            self.modifier_depth = 0;
        }
        self.flags
            .set_display_source_span(self.displays_source_span() || other.displays_source_span());
        self.flags.set_debug_info_handled();
    }

    /// Copies all debug fields without copying semantic or analysis metadata.
    pub(crate) fn copy_debug_context(&mut self, other: &Self) {
        self.source_span = other.source_span;
        self.additional_source_spans = other.additional_source_spans.clone();
        self.modifier_depth = other.modifier_depth;
        self.flags.set_display_source_span(other.displays_source_span());
        self.flags.set_debug_info_handled();
    }

    /// Sets the source span that produced this instruction.
    pub(crate) fn set_source_span(&mut self, span: Option<Span>) {
        let span = span.filter(|span| !span.is_dummy());
        self.source_span = span.unwrap_or(Span::DUMMY);
        self.additional_source_spans = None;
        self.flags.set_display_source_span(span.is_some());
        self.flags.set_debug_info_handled();
    }

    /// Sets source debug information without adding it to canonical MIR text.
    pub(crate) fn set_debug_source_span(&mut self, span: Option<Span>) {
        self.source_span = span.filter(|span| !span.is_dummy()).unwrap_or(Span::DUMMY);
        self.additional_source_spans = None;
        self.flags.set_display_source_span(false);
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
        self.modifier_depth = 0;
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

    /// Copies source context without carrying instruction-specific analysis facts.
    pub(crate) fn debug_context(&self) -> Self {
        let mut metadata = Self::EMPTY;
        metadata.copy_debug_context(self);
        metadata
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
            10 => Some(EffectKind::ICall),
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
            Some(EffectKind::ICall) => 10,
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
    ICall,
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
            Self::ICall => "icall",
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

/// The kind of an instruction.
///
/// TODO(codegen): Consider separating opcode and operands once the MIR shape stabilizes, e.g.
/// `Instruction { opcode: Opcode, operands: SmallVec<[ValueId; 4]>, ... }`. That would make generic
/// operand visitors and rewrites less variant-heavy.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InstKind {
    // Arithmetic operations
    /// Addition: `a + b`
    Add(ValueId, ValueId),
    /// Subtraction: `a - b`
    Sub(ValueId, ValueId),
    /// Multiplication: `a * b`
    Mul(ValueId, ValueId),
    /// Unsigned division: `a / b`
    Div(ValueId, ValueId),
    /// Signed division: `a / b`
    SDiv(ValueId, ValueId),
    /// Unsigned modulo: `a % b`
    Mod(ValueId, ValueId),
    /// Signed modulo: `a % b`
    SMod(ValueId, ValueId),
    /// Exponentiation: `a ** b`
    Exp(ValueId, ValueId),
    /// Add modulo: `(a + b) % n`
    AddMod(ValueId, ValueId, ValueId),
    /// Multiply modulo: `(a * b) % n`
    MulMod(ValueId, ValueId, ValueId),

    // Bitwise operations
    /// Bitwise AND: `a & b`
    And(ValueId, ValueId),
    /// Bitwise OR: `a | b`
    Or(ValueId, ValueId),
    /// Bitwise XOR: `a ^ b`
    Xor(ValueId, ValueId),
    /// Bitwise NOT: `~a`
    Not(ValueId),
    /// Count leading zero bits.
    Clz(ValueId),
    /// Left shift: `a << b`
    Shl(ValueId, ValueId),
    /// Logical right shift: `a >> b`
    Shr(ValueId, ValueId),
    /// Arithmetic right shift: `a >> b` (signed)
    Sar(ValueId, ValueId),
    /// Extract a byte: `byte(i, x)`
    Byte(ValueId, ValueId),

    // Comparison operations
    /// Less than (unsigned): `a < b`
    Lt(ValueId, ValueId),
    /// Greater than (unsigned): `a > b`
    Gt(ValueId, ValueId),
    /// Less than (signed): `a < b`
    SLt(ValueId, ValueId),
    /// Greater than (signed): `a > b`
    SGt(ValueId, ValueId),
    /// Equality: `a == b`
    Eq(ValueId, ValueId),
    /// Check if zero: `a == 0`
    IsZero(ValueId),

    // Memory operations
    /// Load from memory: `mload(offset)`
    MLoad(ValueId),
    /// Store to memory: `mstore(offset, value)`
    MStore(ValueId, ValueId),
    /// Store a single byte: `mstore8(offset, value)`
    MStore8(ValueId, ValueId),
    /// Set a contiguous memory range to zero: `memory_zero(offset, size)`
    MemoryZero(ValueId, ValueId),
    /// Get memory size: `msize()`
    MSize,
    /// Read the free-memory pointer.
    Fmp,
    /// Set the free-memory pointer.
    SetFmp(ValueId),
    /// Reserve memory and return the previous free-memory pointer.
    Alloc {
        /// Requested byte count.
        size: ValueId,
        /// Semantic shape of the returned reference.
        kind: AllocationKind,
        /// Alignment, initialization, and failure behavior.
        semantics: AllocationSemantics,
    },
    /// Read the logical length of a dynamic memory object.
    MemoryObjectLen(ValueId, MemoryObjectKind),
    /// Set the logical length of a dynamic memory object.
    SetMemoryObjectLen(ValueId, ValueId, MemoryObjectKind),
    /// Project the address of the first payload byte from an object.
    MemoryObjectData(ValueId, MemoryObjectKind),
    /// Address a direct field of a struct object.
    MemoryObjectFieldAddr {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Address an array element under the semantic object layout.
    MemoryObjectElementAddr {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one direct struct field without exposing its physical address.
    MemoryObjectLoadField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Store one direct struct field without exposing its physical address.
    MemoryObjectStoreField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
        /// Value to store.
        value: ValueId,
    },
    /// Load one array element without exposing its physical address.
    MemoryObjectLoadElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one byte from a bytes object without exposing its physical address.
    MemoryObjectLoadByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
    },
    /// Store one array element without exposing its physical address.
    MemoryObjectStoreElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
        /// Value to store.
        value: ValueId,
    },
    /// Store one byte in a bytes object without exposing its physical address.
    MemoryObjectStoreByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
        /// Low byte to store.
        value: ValueId,
    },
    /// Store one word at a byte offset in a bytes object without exposing its
    /// physical address.
    MemoryObjectStoreWord {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte offset from the payload start.
        offset: ValueId,
        /// Word to store.
        value: ValueId,
    },
    /// Load one word from a memory slice at a byte offset without exposing its
    /// physical address.
    MemorySliceLoadWord {
        /// Memory slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Load one word from a calldata slice at a byte offset without exposing
    /// the physical calldata address.
    CalldataSliceLoadWord {
        /// Calldata slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Copy a typed slice into the payload of a dynamic memory object.
    MemoryObjectCopyFromSlice {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a typed slice into a byte offset in a dynamic memory object.
    MemoryObjectCopyFromSliceAt {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Byte offset from the destination payload start.
        offset: ValueId,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a byte range between two dynamic memory objects.
    MemoryObjectCopy {
        /// Destination memory object reference.
        destination: ValueId,
        /// Destination memory object kind.
        destination_kind: MemoryObjectKind,
        /// Source memory object reference.
        source: ValueId,
        /// Source memory object kind.
        source_kind: MemoryObjectKind,
        /// Number of bytes to copy.
        length: ValueId,
    },
    /// ABI-encode values into memory.
    AbiEncode {
        /// Storage policy for the encoded result.
        mode: AbiEncodeMode,
        /// Optional left-aligned four-byte selector prefix.
        selector: Option<ValueId>,
        /// Values corresponding to the tuple layout.
        args: Box<[ValueId]>,
        /// Interned semantic ABI layout.
        layout: AbiLayoutRef,
    },
    /// Decode a memory-backed ABI tuple into semantic MIR values.
    ///
    /// The instruction result is the first tuple value. Additional values are
    /// published through the multi-return buffer, matching ordinary MIR calls.
    AbiDecode {
        /// ABI-encoded bytes object.
        data: ValueId,
        /// Interned ABI input layout, including scalar validation types.
        layout: AbiParamLayoutRef,
    },
    /// Copy a statically shaped aggregate from storage into an existing memory allocation.
    StorageToMemory {
        /// Base storage slot.
        storage: ValueId,
        /// Destination memory pointer.
        memory: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy a statically shaped aggregate from memory into storage.
    MemoryToStorage {
        /// Source memory pointer.
        memory: ValueId,
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Clear every storage slot occupied by a statically shaped aggregate.
    ClearStorage {
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy memory: `mcopy(dest, src, len)`
    MCopy(ValueId, ValueId, ValueId),

    // Storage operations
    /// Load from storage: `sload(slot)`
    SLoad(ValueId),
    /// Store to storage: `sstore(slot, value)`
    SStore(ValueId, ValueId),
    /// Transient load: `tload(slot)`
    TLoad(ValueId),
    /// Transient store: `tstore(slot, value)`
    TStore(ValueId, ValueId),

    // Calldata operations
    /// Load from calldata: `calldataload(offset)`
    CalldataLoad(ValueId),
    /// Copy calldata to memory: `calldatacopy(destOffset, offset, size)`
    CalldataCopy(ValueId, ValueId, ValueId),
    /// Get calldata size: `calldatasize()`
    CalldataSize,
    /// Construct a logical `(pointer, length, location)` slice.
    MakeSlice {
        /// Address of the first element or byte.
        ptr: ValueId,
        /// Logical element or byte length.
        len: ValueId,
        /// Address space containing the slice data.
        location: SliceLocation,
    },
    /// Project the data pointer from a slice.
    SlicePtr(ValueId),
    /// Project the logical length from a slice.
    SliceLen(ValueId),
    /// Address inside the current internal-call frame.
    InternalFrameAddr(u64),
    /// Load a mutable local through its logical frame slot.
    ///
    /// A plain memory read: deletable when its result is dead. Ordering
    /// against frame stores, calls, and other frame traffic is carried by
    /// effect kinds and the alias model's `frame_location`.
    FrameLoad {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
    },
    /// Store a mutable local through its logical frame slot.
    FrameStore {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
        /// Value to store.
        value: ValueId,
    },
    /// Base address of the constructor's copied ABI argument blob.
    ConstructorArgsBase,
    /// End address of the constructor's copied ABI argument blob.
    ConstructorArgsEnd,

    // Code operations
    /// Copy constant module data to memory.
    DataCopy(DataRef, ValueId, ValueId),
    /// Get code size: `codesize()`
    CodeSize,
    /// Copy code to memory: `codecopy(destOffset, offset, size)`
    CodeCopy(ValueId, ValueId, ValueId),
    /// Get external code size: `extcodesize(addr)`
    ExtCodeSize(ValueId),
    /// Copy external code to memory: `extcodecopy(addr, destOffset, offset, size)`
    ExtCodeCopy(ValueId, ValueId, ValueId, ValueId),
    /// Get external code hash: `extcodehash(addr)`
    ExtCodeHash(ValueId),
    /// Assign an immutable during construction: `storeimmutable <name>, value`.
    /// Lowered to constructor staging memory after MIR optimization.
    StoreImmutable(ImmutableId, ValueId),
    /// Read an immutable declared by the module: `loadimmutable <name>`.
    ///
    /// In runtime code this assembles to a typed `PUSH<N>` placeholder that the
    /// constructor patches with the staged value before returning the runtime
    /// code. In constructor code it reads the staging word instead.
    LoadImmutable(ImmutableId),

    // Return data operations
    /// Get the current call's return data size: `returndatasize()`.
    ///
    /// Raw volatile query used by Yul and high-level call lowering.
    ReturnDataSize,
    /// Copy return data to memory: `returndatacopy(destOffset, offset, size)`
    ReturnDataCopy(ValueId, ValueId, ValueId),

    // Environment operations
    /// Get caller address: `caller()`
    Caller,
    /// Get call value: `callvalue()`
    CallValue,
    /// Get origin address: `origin()`
    Origin,
    /// Get gas price: `gasprice()`
    GasPrice,
    /// Get block hash: `blockhash(blockNum)`
    BlockHash(ValueId),
    /// Get coinbase address: `coinbase()`
    Coinbase,
    /// Get block timestamp: `timestamp()`
    Timestamp,
    /// Get block number: `number()`
    BlockNumber,
    /// Get previous randao: `prevrandao()`
    PrevRandao,
    /// Get gas limit: `gaslimit()`
    GasLimit,
    /// Get beacon chain slot number: `slotnum()`
    SlotNum,
    /// Get chain ID: `chainid()`
    ChainId,
    /// Get this contract's address: `address()`
    Address,
    /// Get balance: `balance(addr)`
    Balance(ValueId),
    /// Get self balance: `selfbalance()`
    SelfBalance,
    /// Get remaining gas: `gas()`
    Gas,
    /// Get base fee: `basefee()`
    BaseFee,
    /// Get blob base fee: `blobbasefee()`
    BlobBaseFee,
    /// Get blob hash: `blobhash(index)`
    BlobHash(ValueId),

    // Hashing
    /// Keccak256 hash: `keccak256(offset, size)`
    Keccak256(ValueId, ValueId),
    /// Keccak256 hash of a `memorybytes` object's contents:
    /// `keccak256_bytes(object)`.
    ///
    /// Consumes the object reference directly, so the optimizer sees one
    /// whole-object read instead of separate length and data-pointer
    /// projections. `lower-memory-objects` expands it into those projections
    /// and a physical `keccak256`.
    Keccak256Bytes(ValueId),
    /// Hash a fixed-width mapping key and its parent slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    MappingSlot(ValueId, ValueId),
    /// Hash a `[length][data...]` memory value and its parent mapping slot.
    MappingSlotMemory(ValueId, ValueId),
    /// Hash a dynamically-sized calldata value and its parent mapping slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    MappingSlotCalldata(ValueId, ValueId),
    /// Hash the slot of a dynamically-sized storage array to find its data.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    StorageArrayDataSlot(ValueId),
    /// Resolve one element slot in a dynamic storage array.
    ///
    /// The array's base slot, element index, and logical slot stride stay
    /// semantic until the mapping-slot lowering pass expands the hash and
    /// offset calculation.
    StorageArrayElementSlot { slot: ValueId, index: ValueId, element_slots: u64 },

    // Call operations
    // TODO(codegen): Consider unifying external calls as one instruction with a call-kind enum
    // and shared operands once the MIR shape stabilizes.
    /// External call: `call(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    Call {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Call code: `callcode(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    CallCode {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Static call: `staticcall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    StaticCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Delegate call: `delegatecall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    DelegateCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// EOF external call: `extcall(addr, argsOffset, argsSize, value)`.
    ExtCall { addr: ValueId, args_offset: ValueId, args_size: ValueId, value: ValueId },
    /// EOF external delegate call: `extdelegatecall(addr, argsOffset, argsSize)`.
    ExtDelegateCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// EOF external static call: `extstaticcall(addr, argsOffset, argsSize)`.
    ExtStaticCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// Internal function call lowered to a direct jump.
    ICall { function: FunctionId, args: Box<[ValueId]>, returns: u32 },

    // Contract creation
    /// Create contract: `create(value, offset, size)`
    Create(ValueId, ValueId, ValueId),
    /// Create2 contract: `create2(value, offset, size, salt)`
    Create2(ValueId, ValueId, ValueId, ValueId),

    // Log operations
    // TODO(codegen): Consider unifying log0..log4 as one instruction with a topic list.
    /// Log with no topics: `log0(offset, size)`
    Log0(ValueId, ValueId),
    /// Log with 1 topic: `log1(offset, size, topic1)`
    Log1(ValueId, ValueId, ValueId),
    /// Log with 2 topics: `log2(offset, size, topic1, topic2)`
    Log2(ValueId, ValueId, ValueId, ValueId),
    /// Log with 3 topics: `log3(offset, size, topic1, topic2, topic3)`
    Log3(ValueId, ValueId, ValueId, ValueId, ValueId),
    /// Log with 4 topics: `log4(offset, size, topic1, topic2, topic3, topic4)`
    Log4(ValueId, ValueId, ValueId, ValueId, ValueId, ValueId),

    // SSA operations
    /// Phi node: merge values from different predecessors.
    Phi(Vec<(BlockId, ValueId)>),
    /// Select: `select(cond, true_val, false_val)`
    Select(ValueId, ValueId, ValueId),

    // Sign extension
    /// Sign extend: `signextend(b, x)` - extends the sign bit from byte position b
    SignExtend(ValueId, ValueId),
}

impl InstKind {
    /// Returns binary operands whose evaluation order may be exchanged during EVM lowering.
    ///
    /// This includes commutative instructions and comparisons whose opcode can be reversed with
    /// their operands.
    pub(crate) const fn reorderable_binary_operands(&self) -> Option<(ValueId, ValueId)> {
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
            Self::ICall { args, .. } => {
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
            Self::ICall { args, .. } => {
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
        match self {
            Self::Add(_, _) => "add",
            Self::Sub(_, _) => "sub",
            Self::Mul(_, _) => "mul",
            Self::Div(_, _) => "div",
            Self::SDiv(_, _) => "sdiv",
            Self::Mod(_, _) => "mod",
            Self::SMod(_, _) => "smod",
            Self::Exp(_, _) => "exp",
            Self::AddMod(_, _, _) => "addmod",
            Self::MulMod(_, _, _) => "mulmod",
            Self::And(_, _) => "and",
            Self::Or(_, _) => "or",
            Self::Xor(_, _) => "xor",
            Self::Not(_) => "not",
            Self::Clz(_) => "clz",
            Self::Shl(_, _) => "shl",
            Self::Shr(_, _) => "shr",
            Self::Sar(_, _) => "sar",
            Self::Byte(_, _) => "byte",
            Self::Lt(_, _) => "lt",
            Self::Gt(_, _) => "gt",
            Self::SLt(_, _) => "slt",
            Self::SGt(_, _) => "sgt",
            Self::Eq(_, _) => "eq",
            Self::IsZero(_) => "iszero",
            Self::MLoad(_) => "mload",
            Self::MStore(_, _) => "mstore",
            Self::MStore8(_, _) => "mstore8",
            Self::MemoryZero(_, _) => "memory_zero",
            Self::MSize => "msize",
            Self::Fmp => "fmp",
            Self::SetFmp(_) => "set_fmp",
            Self::Alloc { .. } => "alloc",
            Self::MemoryObjectLen(_, _) => "memory_object_len",
            Self::SetMemoryObjectLen(_, _, _) => "set_memory_object_len",
            Self::MemoryObjectData(_, _) => "memory_object_data",
            Self::MemoryObjectFieldAddr { .. } => "memory_object_field_addr",
            Self::MemoryObjectElementAddr { .. } => "memory_object_element_addr",
            Self::MemoryObjectLoadField { .. } => "memory_object_load_field",
            Self::MemoryObjectStoreField { .. } => "memory_object_store_field",
            Self::MemoryObjectLoadElement { .. } => "memory_object_load_element",
            Self::MemoryObjectLoadByte { .. } => "memory_object_load_byte",
            Self::MemoryObjectStoreElement { .. } => "memory_object_store_element",
            Self::MemoryObjectStoreByte { .. } => "memory_object_store_byte",
            Self::MemoryObjectStoreWord { .. } => "memory_object_store_word",
            Self::MemorySliceLoadWord { .. } => "memory_slice_load_word",
            Self::CalldataSliceLoadWord { .. } => "calldata_slice_load_word",
            Self::MemoryObjectCopyFromSlice { .. } => "memory_object_copy_from_slice",
            Self::MemoryObjectCopyFromSliceAt { .. } => "memory_object_copy_from_slice_at",
            Self::MemoryObjectCopy { .. } => "memory_object_copy",
            Self::AbiEncode { .. } => "abi_encode",
            Self::AbiDecode { .. } => "abi_decode",
            Self::StorageToMemory { .. } => "storage_to_memory",
            Self::MemoryToStorage { .. } => "memory_to_storage",
            Self::ClearStorage { .. } => "clear_storage",
            Self::MCopy(_, _, _) => "mcopy",
            Self::SLoad(_) => "sload",
            Self::SStore(_, _) => "sstore",
            Self::TLoad(_) => "tload",
            Self::TStore(_, _) => "tstore",
            Self::CalldataLoad(_) => "calldataload",
            Self::CalldataCopy(_, _, _) => "calldatacopy",
            Self::CalldataSize => "calldatasize",
            Self::MakeSlice { location: SliceLocation::Memory, .. } => "make_memory_slice",
            Self::MakeSlice { location: SliceLocation::Calldata, .. } => "make_calldata_slice",
            Self::MakeSlice { location: SliceLocation::Returndata, .. } => "make_returndata_slice",
            Self::SlicePtr(_) => "slice_ptr",
            Self::SliceLen(_) => "slice_len",
            Self::ConstructorArgsBase => "constructor_args_base",
            Self::ConstructorArgsEnd => "constructor_args_end",
            Self::DataCopy(..) => "data_copy",
            Self::CodeSize => "codesize",
            Self::CodeCopy(_, _, _) => "codecopy",
            Self::StoreImmutable(..) => "storeimmutable",
            Self::LoadImmutable(_) => "loadimmutable",
            Self::ExtCodeSize(_) => "extcodesize",
            Self::ExtCodeCopy(_, _, _, _) => "extcodecopy",
            Self::ExtCodeHash(_) => "extcodehash",
            Self::ReturnDataSize => "returndatasize",
            Self::ReturnDataCopy(_, _, _) => "returndatacopy",
            Self::InternalFrameAddr(_) => "internal_frame_addr",
            Self::FrameLoad { .. } => "frame_load",
            Self::FrameStore { .. } => "frame_store",
            Self::Caller => "caller",
            Self::CallValue => "callvalue",
            Self::Origin => "origin",
            Self::GasPrice => "gasprice",
            Self::BlockHash(_) => "blockhash",
            Self::Coinbase => "coinbase",
            Self::Timestamp => "timestamp",
            Self::BlockNumber => "number",
            Self::PrevRandao => "prevrandao",
            Self::GasLimit => "gaslimit",
            Self::SlotNum => "slotnum",
            Self::ChainId => "chainid",
            Self::Address => "address",
            Self::Balance(_) => "balance",
            Self::SelfBalance => "selfbalance",
            Self::Gas => "gas",
            Self::BaseFee => "basefee",
            Self::BlobBaseFee => "blobbasefee",
            Self::BlobHash(_) => "blobhash",
            Self::Keccak256(_, _) => "keccak256",
            Self::Keccak256Bytes(_) => "keccak256_bytes",
            Self::MappingSlot(_, _) => "mapping_slot",
            Self::MappingSlotMemory(_, _) => "mapping_slot_memory",
            Self::MappingSlotCalldata(_, _) => "mapping_slot_calldata",
            Self::StorageArrayDataSlot(_) => "storage_array_data_slot",
            Self::StorageArrayElementSlot { .. } => "storage_array_element_slot",
            Self::Call { .. } => "call",
            Self::CallCode { .. } => "callcode",
            Self::StaticCall { .. } => "staticcall",
            Self::DelegateCall { .. } => "delegatecall",
            Self::ExtCall { .. } => "extcall",
            Self::ExtDelegateCall { .. } => "extdelegatecall",
            Self::ExtStaticCall { .. } => "extstaticcall",
            Self::ICall { .. } => "icall",
            Self::Create(_, _, _) => "create",
            Self::Create2(_, _, _, _) => "create2",
            Self::Log0(_, _) => "log0",
            Self::Log1(_, _, _) => "log1",
            Self::Log2(_, _, _, _) => "log2",
            Self::Log3(_, _, _, _, _) => "log3",
            Self::Log4(_, _, _, _, _, _) => "log4",
            Self::Phi(_) => "phi",
            Self::Select(_, _, _) => "select",
            Self::SignExtend(_, _) => "signextend",
        }
    }

    /// Returns true if this instruction has side effects.
    /// Side-effect instructions must not be eliminated by DCE.
    #[must_use]
    pub(crate) const fn has_side_effects(&self) -> bool {
        matches!(
            self,
            // Storage writes
            Self::SStore(_, _)
            | Self::MemoryToStorage { .. }
            | Self::ClearStorage { .. }
            | Self::TStore(_, _)
            // Memory writes (may affect external calls)
            | Self::MStore(_, _)
            | Self::MStore8(_, _)
            | Self::MemoryZero(_, _)
            | Self::SetFmp(_)
            | Self::Alloc { .. }
            | Self::SetMemoryObjectLen(_, _, _)
            | Self::FrameStore { .. }
            | Self::MemoryObjectStoreField { .. }
            | Self::MemoryObjectStoreElement { .. }
            | Self::MemoryObjectStoreByte { .. }
            | Self::MemoryObjectStoreWord { .. }
            | Self::MemoryObjectCopyFromSlice { .. }
            | Self::MemoryObjectCopyFromSliceAt { .. }
            | Self::MemoryObjectCopy { .. }
            | Self::AbiEncode { .. }
            | Self::AbiDecode { .. }
            | Self::StorageToMemory { .. }
            | Self::MCopy(_, _, _)
            // External calls
            | Self::Call { .. }
            | Self::CallCode { .. }
            | Self::StaticCall { .. }
            | Self::DelegateCall { .. }
            | Self::ExtCall { .. }
            | Self::ExtDelegateCall { .. }
            | Self::ExtStaticCall { .. }
            | Self::ICall { .. }
            // Contract creation
            | Self::Create(_, _, _)
            | Self::Create2(_, _, _, _)
            // Event emission
            | Self::Log0(_, _)
            | Self::Log1(_, _, _)
            | Self::Log2(_, _, _, _)
            | Self::Log3(_, _, _, _, _)
            | Self::Log4(_, _, _, _, _, _)
            // Data copy operations (write to memory)
            | Self::CalldataCopy(_, _, _)
            | Self::DataCopy(_, _, _)
            | Self::CodeCopy(_, _, _)
            | Self::ExtCodeCopy(_, _, _, _)
            | Self::ReturnDataCopy(_, _, _)
            // Immutable assignment.
            | Self::StoreImmutable(..)
        )
    }

    /// Returns whether this instruction still carries a semantic memory-object operation.
    #[must_use]
    pub(crate) const fn is_memory_object_op(&self) -> bool {
        matches!(
            self,
            Self::Alloc { kind: AllocationKind::Object(_), .. }
                | Self::MemoryObjectLen(_, _)
                | Self::SetMemoryObjectLen(_, _, _)
                | Self::MemoryObjectData(_, _)
                | Self::MemoryObjectFieldAddr { .. }
                | Self::MemoryObjectElementAddr { .. }
                | Self::MemoryObjectLoadField { .. }
                | Self::MemoryObjectStoreField { .. }
                | Self::MemoryObjectLoadElement { .. }
                | Self::MemoryObjectLoadByte { .. }
                | Self::MemoryObjectStoreElement { .. }
                | Self::MemoryObjectStoreByte { .. }
                | Self::MemoryObjectStoreWord { .. }
                | Self::MemorySliceLoadWord { .. }
                | Self::CalldataSliceLoadWord { .. }
                | Self::MemoryObjectCopyFromSlice { .. }
                | Self::MemoryObjectCopyFromSliceAt { .. }
                | Self::MemoryObjectCopy { .. }
                | Self::Keccak256Bytes(_)
        )
    }

    /// Returns a conservative effect classification for this instruction.
    #[must_use]
    pub(crate) const fn effect_kind(&self) -> EffectKind {
        match self {
            Self::MStore(_, _)
            | Self::MStore8(_, _)
            | Self::MemoryZero(_, _)
            | Self::SetFmp(_)
            | Self::Alloc { .. }
            | Self::SetMemoryObjectLen(_, _, _)
            | Self::FrameStore { .. }
            | Self::MemoryObjectStoreField { .. }
            | Self::MemoryObjectStoreElement { .. }
            | Self::MemoryObjectStoreByte { .. }
            | Self::MemoryObjectStoreWord { .. }
            | Self::MemoryObjectCopyFromSlice { .. }
            | Self::MemoryObjectCopyFromSliceAt { .. }
            | Self::MemoryObjectCopy { .. }
            | Self::AbiEncode { .. }
            | Self::AbiDecode { .. }
            | Self::StorageToMemory { .. }
            | Self::MCopy(_, _, _)
            | Self::CalldataCopy(_, _, _)
            | Self::DataCopy(_, _, _)
            | Self::CodeCopy(_, _, _)
            | Self::ExtCodeCopy(_, _, _, _)
            | Self::ReturnDataCopy(_, _, _) => EffectKind::MemoryWrite,
            Self::StoreImmutable(..) => EffectKind::ImmutableWrite,
            Self::MLoad(_)
            | Self::MemorySliceLoadWord { .. }
            | Self::FrameLoad { .. }
            | Self::MemoryObjectLen(_, _)
            | Self::MemoryObjectLoadField { .. }
            | Self::MemoryObjectLoadElement { .. }
            | Self::MemoryObjectLoadByte { .. }
            | Self::Fmp
            | Self::MSize
            | Self::Keccak256(_, _)
            | Self::Keccak256Bytes(_)
            | Self::MappingSlot(_, _)
            | Self::MappingSlotMemory(_, _) => EffectKind::MemoryRead,
            Self::SLoad(_) => EffectKind::StorageRead,
            Self::SStore(_, _) | Self::MemoryToStorage { .. } | Self::ClearStorage { .. } => {
                EffectKind::StorageWrite
            }
            Self::TLoad(_) => EffectKind::TransientRead,
            Self::TStore(_, _) => EffectKind::TransientWrite,
            Self::Call { .. }
            | Self::CallCode { .. }
            | Self::StaticCall { .. }
            | Self::DelegateCall { .. }
            | Self::ExtCall { .. }
            | Self::ExtDelegateCall { .. }
            | Self::ExtStaticCall { .. } => EffectKind::ExternalCall,
            Self::ICall { .. } => EffectKind::ICall,
            Self::Create(_, _, _) | Self::Create2(_, _, _, _) => EffectKind::Create,
            Self::Log0(_, _)
            | Self::Log1(_, _, _)
            | Self::Log2(_, _, _, _)
            | Self::Log3(_, _, _, _, _)
            | Self::Log4(_, _, _, _, _, _) => EffectKind::Log,
            Self::CalldataLoad(_)
            | Self::CalldataSliceLoadWord { .. }
            | Self::MappingSlotCalldata(_, _)
            | Self::CalldataSize
            | Self::ConstructorArgsBase
            | Self::ConstructorArgsEnd
            | Self::CodeSize
            | Self::ExtCodeSize(_)
            | Self::ExtCodeHash(_)
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::BlockHash(_)
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::SlotNum
            | Self::ChainId
            | Self::Address
            | Self::Balance(_)
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee
            | Self::BlobHash(_) => EffectKind::EnvironmentRead,
            Self::LoadImmutable(_) => EffectKind::ImmutableRead,
            Self::Add(_, _)
            | Self::StorageArrayDataSlot(_)
            | Self::StorageArrayElementSlot { .. }
            | Self::Sub(_, _)
            | Self::Mul(_, _)
            | Self::Div(_, _)
            | Self::SDiv(_, _)
            | Self::Mod(_, _)
            | Self::SMod(_, _)
            | Self::Exp(_, _)
            | Self::AddMod(_, _, _)
            | Self::MulMod(_, _, _)
            | Self::And(_, _)
            | Self::Or(_, _)
            | Self::Xor(_, _)
            | Self::Not(_)
            | Self::Clz(_)
            | Self::Shl(_, _)
            | Self::Shr(_, _)
            | Self::Sar(_, _)
            | Self::Byte(_, _)
            | Self::Lt(_, _)
            | Self::Gt(_, _)
            | Self::SLt(_, _)
            | Self::SGt(_, _)
            | Self::Eq(_, _)
            | Self::IsZero(_)
            | Self::MakeSlice { .. }
            | Self::SlicePtr(_)
            | Self::SliceLen(_)
            | Self::MemoryObjectData(_, _)
            | Self::MemoryObjectFieldAddr { .. }
            | Self::MemoryObjectElementAddr { .. }
            | Self::InternalFrameAddr(_)
            | Self::Phi(_)
            | Self::Select(_, _, _)
            | Self::SignExtend(_, _) => EffectKind::Pure,
        }
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
    use crate::mir::{Function, Immediate, Value};
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
        assert_size::<InstructionMetadata>(str!["40"]);
        assert_size::<Instruction>(str!["88"]);
    }
}
