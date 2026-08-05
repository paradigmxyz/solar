//! Type-directed storage locations used by HIR lowering.

use std::sync::OnceLock;

use crate::mir::{FunctionBuilder, TypeSize, ValueId};
use alloy_primitives::U256;
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_interface::Span;
use solar_sema::{
    Gcx,
    hir::{ContractId, ElementaryType, StructId, VariableId},
    ty::{Ty, TyKind},
};

/// The representation of a packed value in an EVM storage word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StorageEncoding {
    Unsigned,
    Signed,
    FixedBytes,
}

/// A logical storage location relative to the contract's storage root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StorageLocation {
    pub(super) slot: U256,
    pub(super) offset: u8,
    pub(super) size: TypeSize,
    pub(super) encoding: StorageEncoding,
}

impl StorageLocation {
    const WORD: TypeSize = TypeSize::new_int_bits(256);

    pub(super) const fn word(slot: U256) -> Self {
        Self { slot, offset: 0, size: Self::WORD, encoding: StorageEncoding::Unsigned }
    }

    const fn packed(self) -> bool {
        self.offset != 0 || self.size.bits() != Self::WORD.bits()
    }

    const fn word_bytes() -> u8 {
        Self::WORD.bytes()
    }

    fn mask(self) -> U256 {
        if self.size >= Self::WORD {
            U256::MAX
        } else {
            (U256::from(1) << self.size.bits()) - U256::from(1)
        }
    }
}

/// Storage locations for all state variables visible to the selected contract.
pub(super) struct StorageLayout<'gcx> {
    builder: StorageBuilder<'gcx>,
}

impl<'gcx> StorageLayout<'gcx> {
    /// Computes Solidity's base-to-derived storage order once for a module.
    pub(super) fn for_contract(gcx: Gcx<'gcx>, contract_id: ContractId) -> Self {
        StorageBuilder::new(gcx).lower(contract_id)
    }

    pub(super) fn get(&self, id: VariableId) -> Option<StorageLocation> {
        self.builder.locations.get(&id).copied()
    }

    pub(super) fn load(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
    ) -> ValueId {
        let slot = builder.imm_u256(location.slot);
        self.load_at(builder, location, slot)
    }

    pub(super) fn store(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        value: ValueId,
    ) {
        let slot = builder.imm_u256(location.slot);
        self.store_at(builder, location, slot, value);
    }

    pub(super) fn load_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
    ) -> ValueId {
        self.load_at(builder, location, slot)
    }

    pub(super) fn store_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        value: ValueId,
    ) {
        self.store_at(builder, location, slot, value);
    }

    pub(super) fn load_packed_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        offset: ValueId,
    ) -> ValueId {
        self.load_at_offset(builder, location, slot, offset)
    }

    pub(super) fn store_packed_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        offset: ValueId,
        value: ValueId,
    ) {
        self.store_at_offset(builder, location, slot, offset, value);
    }

    pub(super) fn packed_encoding(&self, ty: Ty<'gcx>) -> Option<(TypeSize, StorageEncoding)> {
        self.builder.packed_encoding(ty)
    }

    pub(super) fn field_location(
        &self,
        struct_id: solar_sema::hir::StructId,
        field: usize,
    ) -> Option<StorageLocation> {
        self.builder.locate_field(struct_id, field)
    }

    pub(super) fn element_slots(&self, ty: Ty<'gcx>) -> u64 {
        self.builder.storage_slots(ty, Span::DUMMY)
    }

    fn load_at(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
    ) -> ValueId {
        let word = builder.sload(slot);
        let shift = (location.offset != 0).then(|| builder.imm_u64(u64::from(location.offset) * 8));
        self.load_word(builder, location, word, shift)
    }

    fn load_at_offset(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        offset: ValueId,
    ) -> ValueId {
        let word = builder.sload(slot);
        if !location.packed() {
            return word;
        }
        let eight = builder.imm_u64(8);
        let shift = builder.mul(offset, eight);
        self.load_word(builder, location, word, Some(shift))
    }

    fn load_word(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        word: ValueId,
        shift: Option<ValueId>,
    ) -> ValueId {
        if !location.packed() {
            return word;
        }
        let shifted = shift.map_or(word, |shift| builder.shr(shift, word));
        let field_mask = builder.imm_u256(location.mask());
        let masked = builder.and(shifted, field_mask);
        match location.encoding {
            StorageEncoding::Unsigned => masked,
            StorageEncoding::Signed => {
                let index = builder.imm_u64(u64::from(location.size.bytes() - 1));
                builder.signextend(index, masked)
            }
            StorageEncoding::FixedBytes => {
                let shift = u64::from(StorageLocation::word_bytes() - location.size.bytes()) * 8;
                let shift = builder.imm_u64(shift);
                builder.shl(shift, masked)
            }
        }
    }

    fn store_at(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        value: ValueId,
    ) {
        if !location.packed() {
            builder.sstore(slot, value);
            return;
        }
        let shift = (location.offset != 0).then(|| builder.imm_u64(u64::from(location.offset) * 8));
        self.store_word(builder, location, slot, shift, value);
    }

    fn store_at_offset(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        offset: ValueId,
        value: ValueId,
    ) {
        if !location.packed() {
            builder.sstore(slot, value);
            return;
        }

        let eight = builder.imm_u64(8);
        let shift = builder.mul(offset, eight);
        self.store_word(builder, location, slot, Some(shift), value);
    }

    fn store_word(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        shift: Option<ValueId>,
        value: ValueId,
    ) {
        let field_mask = location.mask();
        let field_mask_value = builder.imm_u256(field_mask);
        let shifted_mask =
            shift.map_or(field_mask_value, |shift| builder.shl(shift, field_mask_value));
        let old = builder.sload(slot);
        let keep_mask = builder.not(shifted_mask);
        let cleared = builder.and(old, keep_mask);
        let value = match location.encoding {
            StorageEncoding::FixedBytes => {
                let shift = u64::from(StorageLocation::word_bytes() - location.size.bytes()) * 8;
                let shift = builder.imm_u64(shift);
                builder.shr(shift, value)
            }
            StorageEncoding::Unsigned | StorageEncoding::Signed => value,
        };
        let value = builder.and(value, field_mask_value);
        let value = shift.map_or(value, |shift| builder.shl(shift, value));
        let updated = builder.or(cleared, value);
        builder.sstore(slot, updated);
    }
}

/// Stateful storage-layout computation for one contract.
struct StorageBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    locations: FxHashMap<VariableId, StorageLocation>,
    field_locations: IndexVec<StructId, OnceLock<Box<[StorageLocation]>>>,
    cursor: StorageCursor,
}

/// Walks Solidity's sequential storage layout, including fields packed into a word.
#[derive(Clone, Copy, Debug)]
struct StorageCursor {
    slot: U256,
    offset: u8,
}

impl StorageCursor {
    fn new() -> Self {
        Self { slot: U256::ZERO, offset: 0 }
    }

    fn take(
        &mut self,
        encoding: Option<(TypeSize, StorageEncoding)>,
        slots: u64,
    ) -> StorageLocation {
        if let Some((size, encoding)) = encoding
            && size < StorageLocation::WORD
        {
            let bytes = size.bytes();
            if self.offset.saturating_add(bytes) > StorageLocation::word_bytes() {
                self.finish_word();
            }
            let location = StorageLocation { slot: self.slot, offset: self.offset, size, encoding };
            self.offset = self.offset.saturating_add(bytes);
            if self.offset == StorageLocation::word_bytes() {
                self.finish_word();
            }
            return location;
        }

        self.finish_word();
        let location = StorageLocation::word(self.slot);
        self.slot = self.slot.saturating_add(U256::from(slots));
        location
    }

    fn finish_word(&mut self) {
        if self.offset != 0 {
            self.slot = self.slot.saturating_add(U256::from(1));
            self.offset = 0;
        }
    }

    fn slots(&mut self) -> u64 {
        self.finish_word();
        self.slot.try_into().unwrap_or(u64::MAX)
    }
}

impl<'gcx> StorageBuilder<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            locations: FxHashMap::default(),
            field_locations: gcx.hir.strukt_ids().map(|_| OnceLock::new()).collect(),
            cursor: StorageCursor::new(),
        }
    }

    fn lower(mut self, contract_id: ContractId) -> StorageLayout<'gcx> {
        let contract = self.gcx.hir.contract(contract_id);
        for &base in contract.linearized_bases.iter().rev() {
            for id in self.gcx.hir.contract(base).variables() {
                let var = self.gcx.hir.variable(id);
                if !var.is_state_variable() || var.is_constant() || var.is_immutable() {
                    continue;
                }
                let ty = self.gcx.type_of_item(id.into());
                if let Some(location) = self.allocate(ty, var.span) {
                    self.locations.insert(id, location);
                }
            }
        }
        StorageLayout { builder: self }
    }

    fn allocate(&mut self, ty: Ty<'gcx>, span: Span) -> Option<StorageLocation> {
        let encoding = self.packed_encoding(ty);
        let slots = self.storage_slots(ty, span);
        Some(self.cursor.take(encoding, slots))
    }

    fn locate_field(&self, struct_id: StructId, field: usize) -> Option<StorageLocation> {
        let locations = self.field_locations[struct_id].get_or_init(|| {
            let mut cursor = StorageCursor::new();
            self.gcx
                .hir
                .strukt(struct_id)
                .fields
                .iter()
                .map(|&field_id| {
                    let ty = self.gcx.type_of_item(field_id.into());
                    let encoding = self.packed_encoding(ty);
                    let slots = self.storage_slots(ty, Span::DUMMY);
                    cursor.take(encoding, slots)
                })
                .collect::<Box<_>>()
        });
        locations.get(field).copied()
    }

    fn packed_encoding(&self, ty: Ty<'gcx>) -> Option<(TypeSize, StorageEncoding)> {
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bool) => {
                (TypeSize::new_int_bits(8), StorageEncoding::Unsigned)
            }
            TyKind::Elementary(ElementaryType::Address(_)) | TyKind::Contract(_) => {
                (TypeSize::new_int_bits(160), StorageEncoding::Unsigned)
            }
            TyKind::Elementary(ElementaryType::FixedBytes(size)) => {
                (size, StorageEncoding::FixedBytes)
            }
            TyKind::Elementary(ElementaryType::Int(size)) => (size, StorageEncoding::Signed),
            TyKind::Elementary(ElementaryType::UInt(size)) => (size, StorageEncoding::Unsigned),
            TyKind::Enum(id) => {
                let variants = self.gcx.hir.enumm(id).variants.len().max(1);
                let bits = (usize::BITS - (variants - 1).leading_zeros()).max(1);
                (TypeSize::new_int_bits(bits.div_ceil(8) as u16 * 8), StorageEncoding::Unsigned)
            }
            TyKind::Udvt(inner, _) => return self.packed_encoding(inner),
            _ => return None,
        })
    }

    fn storage_slots(&self, ty: Ty<'gcx>, span: Span) -> u64 {
        match ty.peel_refs().kind {
            TyKind::Struct(id) => {
                let fields = self.gcx.hir.strukt(id).fields;
                self.sequence_slots(
                    fields.iter().map(|&field| self.gcx.type_of_item(field.into())),
                    span,
                )
            }
            TyKind::Array(element, len) => {
                let Ok(len) = u64::try_from(len) else {
                    return self.unsupported_size(span, "fixed-size storage array");
                };
                if let Some((size, _)) = self.packed_encoding(element)
                    && size < StorageLocation::WORD
                {
                    return u64::from(size.bytes()).saturating_mul(len).div_ceil(32).max(1);
                }
                len.saturating_mul(self.storage_slots(element, span)).max(1)
            }
            _ => 1,
        }
    }

    fn sequence_slots(&self, tys: impl Iterator<Item = Ty<'gcx>>, span: Span) -> u64 {
        let mut cursor = StorageCursor::new();
        for ty in tys {
            let encoding = self.packed_encoding(ty);
            let slots = self.storage_slots(ty, span);
            cursor.take(encoding, slots);
        }
        cursor.slots().max(1)
    }

    fn unsupported_size(&self, span: Span, kind: &str) -> u64 {
        self.gcx.dcx().err(format!("{kind} is too large for codegen")).span(span).emit();
        1
    }
}
