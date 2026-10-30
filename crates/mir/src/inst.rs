//! MIR instructions.

use super::{
    Function, HirExprId, InstKind, MirPhase, MirType, PhaseCategory, PtrLocation, Value, ValueId,
};
use crate::{Builtin, Callee};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_interface::Span;
use std::fmt;

/// Extra information attached to a MIR instruction by lowering or analysis passes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct InstructionMetadata {
    /// Proven storage alias key for `sload`/`sstore` instructions.
    storage_alias: Option<Box<StorageAlias>>,
    /// Source span that produced this instruction, when the lowerer can preserve it.
    source_span: Span,
    /// Additional origins of shared code, allocated only after a debug-origin merge.
    additional_source_spans: Option<Box<SmallVec<[Span; 2]>>>,
    /// Legacy source-map modifier nesting depth for this instruction.
    modifier_depth: u32,
    /// HIR expression that produced this instruction, when the lowerer can preserve it.
    hir_expr: Option<HirExprId>,
    /// Loop nesting depth attached by loop-aware analyses.
    pub loop_depth: u16,
    /// Packed optional memory region, effect kind, and boolean flags.
    flags: MetadataFlags,
}

impl InstructionMetadata {
    /// Empty instruction metadata.
    pub const EMPTY: Self = Self {
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
    pub fn storage_alias(&self) -> Option<StorageAlias> {
        self.storage_alias.as_deref().copied()
    }

    /// Sets the proven storage alias key.
    pub fn set_storage_alias(&mut self, alias: Option<StorageAlias>) {
        self.storage_alias = alias.map(Box::new);
    }

    /// Returns the HIR expression that produced this instruction.
    #[must_use]
    pub(crate) fn hir_expr(&self) -> Option<HirExprId> {
        self.hir_expr
    }

    /// Sets the HIR expression that produced this instruction.
    pub fn set_hir_expr(&mut self, expr: Option<HirExprId>) {
        self.hir_expr = expr;
    }

    /// Returns the source span that produced this instruction.
    #[must_use]
    pub fn source_span(&self) -> Option<Span> {
        (!self.source_span.is_dummy()).then_some(self.source_span)
    }

    /// Returns every retained origin, in deterministic merge order.
    pub fn source_spans(&self) -> impl Iterator<Item = Span> + '_ {
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
        } else if self.source_spans().count() < solar_ir::debug::MAX_DEBUG_SPANS {
            self.additional_source_spans.get_or_insert_default().push(span);
        }
    }

    /// Unions origins without importing instruction-specific analysis facts.
    pub fn merge_debug_context(&mut self, other: &Self) {
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
    pub fn copy_debug_context(&mut self, other: &Self) {
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
    pub const fn modifier_depth(&self) -> u32 {
        self.modifier_depth
    }

    /// Sets the source-map modifier nesting depth for this instruction.
    pub(crate) fn set_modifier_depth(&mut self, depth: u32) {
        self.modifier_depth = depth;
        self.flags.set_debug_info_handled();
    }

    /// Marks this instruction as intentionally having no source location.
    pub fn mark_debug_info_dropped(&mut self) {
        self.set_debug_source_span(None);
        self.modifier_depth = 0;
    }

    /// Returns whether source debug information was preserved or intentionally dropped.
    #[must_use]
    pub fn debug_info_is_handled(&self) -> bool {
        self.flags.debug_info_is_handled()
    }

    /// Returns whether canonical MIR text should include the source span.
    #[must_use]
    pub(crate) fn displays_source_span(&self) -> bool {
        self.flags.displays_source_span()
    }

    /// Copies source context without carrying instruction-specific analysis facts.
    pub fn debug_context(&self) -> Self {
        let mut metadata = Self::EMPTY;
        metadata.copy_debug_context(self);
        metadata
    }

    /// Returns the proven memory region.
    #[must_use]
    pub fn memory_region(&self) -> Option<MemoryRegion> {
        self.flags.memory_region()
    }

    /// Sets the proven memory region.
    pub fn set_memory_region(&mut self, region: Option<MemoryRegion>) {
        self.flags.set_memory_region(region);
    }

    /// Returns whether this instruction was lowered from an unchecked arithmetic context.
    #[must_use]
    pub fn unchecked(&self) -> bool {
        self.flags.unchecked()
    }

    /// Sets whether this instruction was lowered from an unchecked arithmetic context.
    pub(crate) fn set_unchecked(&mut self, unchecked: bool) {
        self.flags.set_unchecked(unchecked);
    }

    /// Returns the conservative effect classification attached by lowering or analysis.
    #[must_use]
    pub fn effect(&self) -> Option<EffectKind> {
        self.flags.effect()
    }

    /// Sets the conservative effect classification attached by lowering or analysis.
    pub fn set_effect(&mut self, effect: Option<EffectKind>) {
        self.flags.set_effect(effect);
    }

    /// Returns whether final placement of this allocation is deferred to the backend.
    #[must_use]
    pub fn deferred_alloc(&self) -> bool {
        self.flags.deferred_alloc()
    }

    /// Defers final placement of this allocation to the backend.
    pub fn set_deferred_alloc(&mut self) {
        self.flags.set_deferred_alloc();
    }

    /// Clears the deferred-allocation marker after an allocation is rewritten.
    pub fn clear_deferred_alloc(&mut self) {
        self.flags.clear_deferred_alloc();
    }

    /// Returns whether this word addition came from pointer arithmetic on a memory object, whose
    /// offset the compiler checked against the object's bounds.
    #[must_use]
    pub fn checked_object_offset(&self) -> bool {
        self.flags.checked_object_offset()
    }

    /// Marks a word addition that came from pointer arithmetic on a memory object.
    pub fn set_checked_object_offset(&mut self) {
        self.flags.set_checked_object_offset();
    }

    /// Returns whether this allocation must keep its free-memory-pointer address and bump.
    #[must_use]
    pub fn preserves_fmp(&self) -> bool {
        self.flags.preserves_fmp()
    }

    /// Marks an allocation whose address or bump is already observable before allocation.
    pub fn set_preserves_fmp(&mut self, value: bool) {
        self.flags.set_preserves_fmp(value);
    }

    /// Returns whether this sum is an allocator bump that does not wrap.
    #[must_use]
    pub fn alloc_ptr(&self) -> bool {
        self.flags.alloc_ptr()
    }

    /// Marks the sum of an allocation's pointer and size, which cannot wrap in a program that
    /// completes, so the sum is not below the pointer.
    pub fn set_alloc_ptr(&mut self, value: bool) {
        self.flags.set_alloc_ptr(value);
    }

    /// Returns whether this copy's source and destination ranges never overlap.
    #[must_use]
    pub fn disjoint(&self) -> bool {
        self.flags.disjoint()
    }

    /// Marks a copy whose source and destination ranges never overlap.
    pub fn set_disjoint(&mut self, value: bool) {
        self.flags.set_disjoint(value);
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
    const DISJOINT: u16 = 0b10_0000_0000;
    const PRESERVES_FMP: u16 = 0b100_0000_0000;
    const DISPLAY_SOURCE_SPAN: u16 = 0b1000_0000_0000;
    const DEBUG_INFO_HANDLED: u16 = 0b1_0000_0000_0000;
    const CHECKED_OBJECT_OFFSET: u16 = 0b10_0000_0000_0000;
    const ALLOC_PTR: u16 = 0b100_0000_0000_0000;

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

    fn disjoint(self) -> bool {
        self.0 & Self::DISJOINT != 0
    }

    fn set_disjoint(&mut self, value: bool) {
        if value {
            self.0 |= Self::DISJOINT;
        } else {
            self.0 &= !Self::DISJOINT;
        }
    }

    fn checked_object_offset(self) -> bool {
        self.0 & Self::CHECKED_OBJECT_OFFSET != 0
    }

    fn set_checked_object_offset(&mut self) {
        self.0 |= Self::CHECKED_OBJECT_OFFSET;
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

    fn alloc_ptr(self) -> bool {
        self.0 & Self::ALLOC_PTR != 0
    }

    fn set_alloc_ptr(&mut self, value: bool) {
        if value {
            self.0 |= Self::ALLOC_PTR;
        } else {
            self.0 &= !Self::ALLOC_PTR;
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
pub enum StorageAlias {
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
    pub fn for_value(func: &Function, value: ValueId) -> Self {
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
    pub fn may_alias(self, other: Self) -> bool {
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
    pub const fn symbolic_base(self) -> Option<ValueId> {
        match self {
            Self::Symbolic(value) | Self::Offset { base: value, .. } => Some(value),
            Self::Slot(_) => None,
        }
    }

    /// Returns this alias with its symbolic base renamed by `f`.
    #[must_use]
    pub(crate) fn map_base(self, f: impl FnOnce(&mut ValueId)) -> Self {
        match self {
            Self::Slot(_) => self,
            Self::Symbolic(mut value) => {
                f(&mut value);
                Self::Symbolic(value)
            }
            Self::Offset { mut base, offset } => {
                f(&mut base);
                Self::Offset { base, offset }
            }
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

/// A coarse memory layout annotation, not a proof of disjointness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryRegion {
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
pub enum EffectKind {
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

    /// Whether this is pure computation or a read of memory, storage, transient storage or the
    /// environment.
    #[must_use]
    pub const fn is_read_only(self) -> bool {
        matches!(
            self,
            Self::Pure
                | Self::MemoryRead
                | Self::StorageRead
                | Self::TransientRead
                | Self::EnvironmentRead
        )
    }

    /// Whether this reads or writes memory.
    #[must_use]
    pub const fn is_memory(self) -> bool {
        matches!(self, Self::MemoryRead | Self::MemoryWrite)
    }
}

/// Initialization performed for a newly reserved range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AllocationInitialization {
    /// Preserve the range's existing bytes until explicitly overwritten.
    Uninitialized,
    /// Initialize every reserved byte to zero.
    Zeroed,
}

/// Failure behavior attached to an allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AllocationFailure {
    /// The producer has already proved the bump valid.
    Infallible,
    /// Revert with the memory-allocation panic when the bump overflows. Only Solidity objects
    /// allocate this way, and each reserves at least one word: its length header or its first
    /// field.
    Checked,
}

/// Storage policy for an ABI-encoded result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbiEncodeMode {
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
    pub const fn result_type(self) -> MirType {
        match self {
            Self::Slice | Self::Scratch => MirType::Slice(PtrLocation::Memory),
            Self::Bytes => MirType::MEMPTR,
        }
    }
}

/// Semantic allocation policy carried by [`InstKind::Alloc`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocationSemantics {
    /// Requested initialization.
    pub initialization: AllocationInitialization,
    /// Requested failure behavior.
    pub failure: AllocationFailure,
}

impl AllocationSemantics {
    /// Uninitialized allocation whose validity is already proven.
    pub const INTERNAL: Self = Self {
        initialization: AllocationInitialization::Uninitialized,
        failure: AllocationFailure::Infallible,
    };

    /// Checked and zero-initialized Solidity object allocation. Object lowering includes the
    /// header and padding in `size`.
    pub const SOLIDITY_ZEROED: Self = Self {
        initialization: AllocationInitialization::Zeroed,
        failure: AllocationFailure::Checked,
    };

    /// Checked allocation for objects initialized by their
    /// producer rather than by the allocator.
    pub const SOLIDITY_UNINITIALIZED: Self = Self {
        initialization: AllocationInitialization::Uninitialized,
        failure: AllocationFailure::Checked,
    };
}

/// An instruction in the MIR.
#[derive(Clone, Debug)]
pub struct Instruction {
    /// The kind of instruction.
    pub kind: InstKind,
    /// The result type (if any).
    pub result_ty: Option<MirType>,
    /// The value allocated for this instruction's result.
    result: Option<ValueId>,
    /// Metadata produced by lowering or analysis.
    pub metadata: InstructionMetadata,
}

impl Instruction {
    /// Returns the semantic operation that still needs representation lowering.
    pub fn unlowered_reason(&self, func: &Function) -> Option<PhaseCategory> {
        if self.result_ty == Some(MirType::I1)
            && self.kind.op_def().result == super::ResultKind::Integer
            && !matches!(self.kind, InstKind::And(..) | InstKind::Or(..) | InstKind::Xor(..))
            || matches!(self.kind, InstKind::SLt(a, _) | InstKind::SGt(a, _)
                if func.value_ty(a) == Some(MirType::I1))
        {
            return Some(PhaseCategory::NarrowIntegerArithmetic);
        }
        self.kind.phase_violation(MirPhase::Lowered, &self.metadata)
    }

    /// Creates a new instruction.
    #[must_use]
    pub const fn new(kind: InstKind, result_ty: Option<MirType>) -> Self {
        Self { kind, result_ty, result: None, metadata: InstructionMetadata::EMPTY }
    }

    /// Marks this synthetic instruction as intentionally having no source location.
    #[must_use]
    pub fn with_debug_info_dropped(mut self) -> Self {
        self.metadata.mark_debug_info_dropped();
        self
    }

    /// Returns the value allocated for this instruction's result.
    #[must_use]
    pub const fn result(&self) -> Option<ValueId> {
        self.result
    }

    /// Replaces the value allocated for this instruction's result.
    pub(crate) fn set_result(&mut self, result: Option<ValueId>) -> Option<ValueId> {
        std::mem::replace(&mut self.result, result)
    }

    /// Returns the operands of this instruction.
    #[must_use]
    pub fn operands(&self) -> SmallVec<[ValueId; 8]> {
        self.kind.operands()
    }

    /// Visits the operands of this instruction in canonical order, without collecting them.
    pub fn visit_operands(&self, f: impl FnMut(ValueId)) {
        self.kind.visit_operands(f);
    }

    /// Replaces an operation with an equivalent one, preserving its result and provenance.
    ///
    /// Memory regions, storage aliases, and effect overrides describe the old
    /// operation and must be recomputed. ABI validation dependencies and
    /// allocation obligations are semantic, so they survive an equivalent
    /// rewrite; allocation markers are dropped only when allocation is lowered away.
    /// The caller must prove equivalence and preserve the result representation.
    pub fn replace_kind(&mut self, kind: InstKind) {
        let result = kind.op_def().result;
        debug_assert!(
            (result == super::ResultKind::Custom
                || result.produces_value() == self.result_ty.is_some())
                && self.result_ty.is_none_or(|ty| kind.admits_result_type(ty)),
            "replacement must preserve the result representation"
        );
        // %result = old(operands) -> %result = equivalent(new_operands)
        self.kind = kind;
        self.invalidate_operand_metadata();
        self.metadata.set_alloc_ptr(false);
        if !matches!(self.kind, InstKind::Alloc { .. }) {
            self.metadata.clear_deferred_alloc();
            self.metadata.set_preserves_fmp(false);
        }
    }

    /// Visits operands during an equivalence-preserving rewrite and invalidates
    /// operand-dependent facts only if a value actually changes. Without an operand change, the
    /// base of a storage alias is renamed the same way.
    pub fn rewrite_operands(&mut self, mut f: impl FnMut(&mut ValueId)) {
        let mut changed = false;
        // %result = op(operands) -> %result = op(equivalent_operands)
        self.kind.visit_operands_mut(|operand| {
            let old = *operand;
            f(operand);
            changed |= *operand != old;
        });
        if changed {
            self.invalidate_operand_metadata();
        } else if let Some(alias) = self.metadata.storage_alias() {
            // sstore slot, value !metadata(storage=offset(old, n))
            //   -> sstore slot, value !metadata(storage=offset(new, n))
            self.metadata.set_storage_alias(Some(alias.map_base(&mut f)));
        }
    }

    /// Drops facts whose proof can depend on the previous operation or operands.
    fn invalidate_operand_metadata(&mut self) {
        self.metadata.set_memory_region(None);
        self.metadata.set_storage_alias(None);
        self.metadata.set_effect(None);
    }
}

/// Solidity low-level address-call behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressCallKind {
    Call,
    Static,
    Delegate,
}

impl InstKind {
    /// Infers integer results from the operation's declared operand signature.
    pub(crate) fn inferred_result_type(&self, func: &Function) -> Option<MirType> {
        if let Self::CheckedBinary { arithmetic, .. } = self {
            Some(arithmetic.ty().mir_type())
        } else if self.op_def().result == super::ResultKind::Pointer {
            self.op().first_operand().map(|value| super::typing::pointer(func, value))
        } else if self.op_def().result == super::ResultKind::Integer {
            self.op().first_operand().map(|value| super::typing::integer_type(func, value))
        } else {
            self.op_def().result.default_type()
        }
    }

    /// Checks the operation's result representation.
    pub(crate) fn admits_result_type(&self, ty: MirType) -> bool {
        self.op_def().result.admits_type(ty)
    }

    /// Checks scalar operation contracts without applying implicit conversions.
    pub fn scalar_types_match(&self, func: &Function, result: Option<MirType>) -> bool {
        let ty = |value| func.value_ty(value);
        if let Some(expected) = self.operand_types(func) {
            let mut expected = expected.into_iter();
            let mut matches = true;
            self.visit_operands(|value| {
                matches &= expected.next().is_some_and(|expected| ty(value) == Some(expected));
            });
            if !matches || expected.next().is_some() {
                return false;
            }
        }
        match *self {
            Self::Eq(a, b)
            | Self::Ne(a, b)
            | Self::Lt(a, b)
            | Self::Gt(a, b)
            | Self::SLt(a, b)
            | Self::SGt(a, b) => {
                result == Some(MirType::I1)
                    && ty(a) == ty(b)
                    && matches!(ty(a), Some(MirType::Int(_)))
            }
            Self::Trunc(value, bits) => matches!((ty(value), result),
                (Some(MirType::Int(from)), Some(MirType::Int(to))) if from > to && to.get() == bits),
            Self::Zext(value) => matches!((ty(value), result),
                (Some(MirType::Int(from)), Some(MirType::Int(to))) if from < to),
            Self::Sext(value, from_bits, to_bits) => matches!((ty(value), result),
                (Some(MirType::Int(from)), Some(MirType::Int(to)))
                    if from < to && from.get() == from_bits && to.get() == to_bits),
            Self::PtrToInt(value, bits) => {
                ty(value).is_some_and(MirType::is_pointer)
                    && matches!(result, Some(MirType::Int(to)) if to.get() == bits)
            }
            Self::IntToPtr(value) => {
                matches!(ty(value), Some(MirType::Int(_)))
                    && result.is_some_and(MirType::is_pointer)
            }
            Self::CheckedBinary { arithmetic, .. } => result == Some(arithmetic.ty().mir_type()),
            Self::MakeSlice { location, .. } => result == Some(MirType::Slice(location)),
            Self::FrameLoad { kind, .. } => result == Some(kind.result_type()),
            Self::AbiEncode { mode, .. } => result == Some(mode.result_type()),
            Self::Phi(_) => {
                result.is_some_and(|ty| ty != MirType::Void)
                    && self.operands().iter().all(|&value| ty(value) == result)
            }
            Self::Select(condition, a, b) => {
                result.is_some_and(|ty| ty != MirType::Void)
                    && ty(condition) == Some(MirType::I1)
                    && ty(a) == result
                    && ty(b) == result
            }
            Self::AbiDecode { ref layout, .. } => result.is_some() != layout.types.is_empty(),
            Self::InsertValue { .. }
            | Self::ExtractValue { .. }
            | Self::StorageBytesLoad(..)
            | Self::StorageArrayLoad { .. }
            | Self::AbiEncodePacked { .. }
            | Self::LoadImmutable(..) => result.is_some(),
            // Module and builtin signatures are checked by the validator.
            Self::ICall { .. } => true,
            _ if matches!(
                self.op_def().result,
                super::ResultKind::Integer | super::ResultKind::Pointer
            ) =>
            {
                self.inferred_result_type(func) == result
                    && result.is_some_and(|ty| self.admits_result_type(ty))
            }
            _ => {
                let kind = self.op_def().result;
                kind != super::ResultKind::Custom
                    && result.map_or(!kind.produces_value(), |ty| {
                        kind.produces_value() && kind.admits_type(ty)
                    })
            }
        }
    }

    /// Returns the tested operand of a canonical equality with zero.
    pub fn zero_test_operand(&self, func: &super::Function) -> Option<ValueId> {
        match *self {
            Self::Eq(a, b) if func.value_u256(b).is_some_and(|v| v.is_zero()) => Some(a),
            Self::Eq(a, b) if func.value_u256(a).is_some_and(|v| v.is_zero()) => Some(b),
            _ => None,
        }
    }

    /// Returns the tested operand of a canonical inequality with zero.
    pub fn nonzero_test_operand(&self, func: &super::Function) -> Option<ValueId> {
        match *self {
            Self::Ne(a, b) if func.value_u256(b).is_some_and(|v| v.is_zero()) => Some(a),
            Self::Ne(a, b) if func.value_u256(a).is_some_and(|v| v.is_zero()) => Some(b),
            _ => None,
        }
    }

    /// Clones the instruction with zeroed value operands to compare its remaining fields.
    pub fn clone_without_operands(&self) -> Self {
        let mut kind = self.clone();
        kind.visit_operands_mut(|value| *value = ValueId::from_usize(0));
        kind
    }

    /// Returns binary operands whose evaluation order may be exchanged during EVM lowering.
    ///
    /// This includes commutative instructions and comparisons whose opcode can be reversed with
    /// their operands.
    pub const fn reorderable_binary_operands(&self) -> Option<(ValueId, ValueId)> {
        if !self.op_def().traits.contains(super::OpTraits::REORDERABLE) {
            return None;
        }
        match self {
            Self::InsertValue { aggregate: a, value: b, .. }
            | Self::DataCopy(_, a, b)
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

    /// Returns the operands of this instruction.
    #[must_use]
    pub fn operands(&self) -> SmallVec<[ValueId; 8]> {
        let mut out = SmallVec::new();
        self.collect_operands(&mut out);
        out
    }

    /// Returns whether `value` is an operand of this instruction, without collecting them.
    #[must_use]
    pub fn reads(&self, value: ValueId) -> bool {
        self.any_operand(|operand| operand == value)
    }

    /// Returns whether any operand of this instruction satisfies `predicate`, without collecting
    /// them.
    #[must_use]
    pub fn any_operand(&self, mut predicate: impl FnMut(ValueId) -> bool) -> bool {
        let mut found = false;
        self.visit_operands(|operand| found = found || predicate(operand));
        found
    }

    /// Returns the mnemonic for this instruction.
    #[must_use]
    pub const fn mnemonic(&self) -> &'static str {
        match self {
            Self::CheckedBinary { op, .. } => op.name(),
            Self::ValidateAbi(_) => "validate_abi",
            Self::AbiEncodePacked { hash, .. } => {
                if *hash {
                    "keccak256_packed"
                } else {
                    "abi_encode_packed"
                }
            }
            Self::AddressCall { kind: AddressCallKind::Call, .. } => "address_call",
            Self::AddressCall { kind: AddressCallKind::Static, .. } => "address_staticcall",
            Self::AddressCall { kind: AddressCallKind::Delegate, .. } => "address_delegatecall",
            _ => self.op_def().mnemonic,
        }
    }

    /// Returns whether the instruction has required effects, including control effects.
    #[must_use]
    pub const fn has_side_effects(&self) -> bool {
        self.effects().must_execute()
    }

    /// Returns a conservative effect classification for this instruction.
    #[must_use]
    pub const fn effect_kind(&self) -> EffectKind {
        match self {
            Self::ICall { function: Callee::Builtin(builtin), .. } => match builtin {
                Builtin::CheckedAddMod | Builtin::CheckedMulMod | Builtin::Check { .. } => {
                    EffectKind::Pure
                }
                Builtin::Erc7201 | Builtin::ReturndataBytes => EffectKind::MemoryWrite,
                Builtin::Require(_) => EffectKind::MemoryRead,
                Builtin::Sha256
                | Builtin::Ripemd160
                | Builtin::EcRecover
                | Builtin::Send
                | Builtin::Transfer => EffectKind::ExternalCall,
            },
            _ => self.op_def().effect,
        }
    }

    /// Projects field `index` of `slice`, with the field's type in the slice's location.
    #[must_use]
    pub fn slice_field(func: &Function, slice: ValueId, index: u32) -> (Self, MirType) {
        let ty = super::typing::slice_type(func, slice);
        let MirType::Slice(location) = ty else { unreachable!("slice_type returns a slice") };
        let field = MirType::slice_fields(location)[index as usize];
        (Self::ExtractValue { ty, aggregate: slice, index }, field)
    }

    /// Returns the slice and field index this instruction projects, if it projects a slice.
    #[must_use]
    pub const fn slice_projection(&self) -> Option<(ValueId, u32)> {
        match *self {
            Self::ExtractValue { ty: MirType::Slice(_), aggregate, index } => {
                Some((aggregate, index))
            }
            _ => None,
        }
    }

    /// Returns whether this is a `shl`, `shr`, or `sar`.
    #[must_use]
    pub const fn is_shift(&self) -> bool {
        matches!(self, Self::Shl(..) | Self::Shr(..) | Self::Sar(..))
    }

    /// Returns whether the textual form lists the stored operands in reverse: shifts are stored
    /// in EVM stack order but read as `value, shift`.
    #[must_use]
    pub(crate) const fn reverses_text_operands(&self) -> bool {
        self.is_shift()
    }
}

impl fmt::Display for InstKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.mnemonic())
    }
}
