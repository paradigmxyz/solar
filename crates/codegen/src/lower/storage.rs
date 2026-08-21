//! Type-directed storage locations used by HIR lowering.

use std::{cell::RefCell, sync::OnceLock};

use crate::mir::{FunctionBuilder, TypeSize, ValueId};
use alloy_primitives::U256;
use solar_ast::DataLocation;
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
    transient: bool,
}

impl StorageLocation {
    const WORD: TypeSize = TypeSize::new_int_bits(256);

    pub(super) const fn word(slot: U256) -> Self {
        Self {
            slot,
            offset: 0,
            size: Self::WORD,
            encoding: StorageEncoding::Unsigned,
            transient: false,
        }
    }

    pub(super) const fn packed_word(size: TypeSize, encoding: StorageEncoding) -> Self {
        Self { slot: U256::ZERO, offset: 0, size, encoding, transient: false }
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

    fn load(self, builder: &mut FunctionBuilder<'_>, slot: ValueId) -> ValueId {
        if self.transient { builder.tload(slot) } else { builder.sload(slot) }
    }

    fn store(self, builder: &mut FunctionBuilder<'_>, slot: ValueId, value: ValueId) {
        if self.transient {
            builder.tstore(slot, value);
        } else {
            builder.sstore(slot, value);
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

    pub(super) fn element_slots(&self, ty: Ty<'gcx>, span: Span) -> u64 {
        self.builder.storage_slots(ty, span)
    }

    fn load_at(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
    ) -> ValueId {
        let word = location.load(builder, slot);
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
        let word = location.load(builder, slot);
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
            location.store(builder, slot, value);
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
            location.store(builder, slot, value);
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
        let old = location.load(builder, slot);
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
        location.store(builder, slot, updated);
    }
}

/// Stateful storage-layout computation for one contract.
struct StorageBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    locations: FxHashMap<VariableId, StorageLocation>,
    field_types: IndexVec<StructId, OnceLock<&'gcx [Ty<'gcx>]>>,
    field_locations: RefCell<FxHashMap<StructId, Option<Box<[StorageLocation]>>>>,
    storage_cursor: StorageCursor,
    transient_cursor: StorageCursor,
}

/// Walks Solidity's sequential storage layout, including fields packed into a word.
#[derive(Clone, Copy, Debug)]
struct StorageCursor {
    slot: U256,
    offset: u8,
    transient: bool,
}

impl StorageCursor {
    fn new(transient: bool) -> Self {
        Self { slot: U256::ZERO, offset: 0, transient }
    }

    fn take(
        &mut self,
        encoding: Option<(TypeSize, StorageEncoding)>,
        slots: u64,
    ) -> Option<StorageLocation> {
        if let Some((size, encoding)) = encoding
            && size < StorageLocation::WORD
        {
            let bytes = size.bytes();
            if self.offset.checked_add(bytes)? > StorageLocation::word_bytes() {
                self.finish_word()?;
            }
            let location = StorageLocation {
                slot: self.slot,
                offset: self.offset,
                size,
                encoding,
                transient: self.transient,
            };
            self.offset += bytes;
            if self.offset == StorageLocation::word_bytes() {
                self.finish_word()?;
            }
            return Some(location);
        }

        self.finish_word()?;
        let location = StorageLocation {
            slot: self.slot,
            offset: 0,
            size: StorageLocation::WORD,
            encoding: StorageEncoding::Unsigned,
            transient: self.transient,
        };
        self.slot = self.slot.checked_add(U256::from(slots))?;
        Some(location)
    }

    fn finish_word(&mut self) -> Option<()> {
        if self.offset != 0 {
            self.slot = self.slot.checked_add(U256::from(1))?;
            self.offset = 0;
        }
        Some(())
    }

    fn slots(&mut self) -> Option<u64> {
        self.finish_word()?;
        self.slot.try_into().ok()
    }
}

impl<'gcx> StorageBuilder<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            locations: FxHashMap::default(),
            field_types: gcx.hir.strukt_ids().map(|_| OnceLock::new()).collect(),
            field_locations: RefCell::new(FxHashMap::default()),
            storage_cursor: StorageCursor::new(false),
            transient_cursor: StorageCursor::new(true),
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
                let transient = var.data_location == Some(DataLocation::Transient);
                if let Some(location) = self.allocate(ty, var.span, transient) {
                    self.locations.insert(id, location);
                }
            }
        }
        StorageLayout { builder: self }
    }

    fn allocate(&mut self, ty: Ty<'gcx>, span: Span, transient: bool) -> Option<StorageLocation> {
        let encoding = self.packed_encoding(ty);
        let slots = self.storage_slots(ty, span);
        let cursor = if transient { &mut self.transient_cursor } else { &mut self.storage_cursor };
        cursor.take(encoding, slots).or_else(|| {
            self.gcx
                .dcx()
                .err("contract storage layout exceeds the addressable storage space")
                .span(span)
                .emit();
            None
        })
    }

    fn locate_field(&self, struct_id: StructId, field: usize) -> Option<StorageLocation> {
        if let Some(locations) = self.field_locations.borrow().get(&struct_id) {
            return locations.as_ref()?.get(field).copied();
        }

        let mut cursor = StorageCursor::new(false);
        let locations = self
            .struct_field_types(struct_id)
            .iter()
            .map(|&ty| {
                let encoding = self.packed_encoding(ty);
                let slots = self.storage_slots(ty, Span::DUMMY);
                cursor.take(encoding, slots)
            })
            .collect::<Option<Box<_>>>();
        if locations.is_none() {
            self.unsupported_size(Span::DUMMY, "storage struct");
        }
        let mut cached = self.field_locations.borrow_mut();
        cached.entry(struct_id).or_insert(locations).as_ref()?.get(field).copied()
    }

    fn struct_field_types(&self, struct_id: StructId) -> &'gcx [Ty<'gcx>] {
        self.field_types[struct_id].get_or_init(|| self.gcx.struct_field_types(struct_id))
    }

    fn packed_encoding(&self, ty: Ty<'gcx>) -> Option<(TypeSize, StorageEncoding)> {
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bool) => {
                (TypeSize::new_int_bits(8), StorageEncoding::Unsigned)
            }
            TyKind::Elementary(ElementaryType::Address(_)) | TyKind::Contract(_) => {
                (TypeSize::new_int_bits(160), StorageEncoding::Unsigned)
            }
            TyKind::Fn(function) if function.is_external() => {
                (TypeSize::new_int_bits(192), StorageEncoding::Unsigned)
            }
            TyKind::Fn(_) => (TypeSize::new_int_bits(64), StorageEncoding::Unsigned),
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
        self.storage_slots_inner(ty, span).unwrap_or(1)
    }

    fn storage_slots_inner(&self, ty: Ty<'gcx>, span: Span) -> Option<u64> {
        match ty.peel_refs().kind {
            TyKind::Struct(id) => {
                self.sequence_slots(self.struct_field_types(id).iter().copied(), span)
            }
            TyKind::Array(element, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.unsupported_size(span, "fixed-size storage array");
                    return None;
                };
                if let Some((size, _)) = self.packed_encoding(element)
                    && size < StorageLocation::WORD
                {
                    let bytes = u64::from(size.bytes());
                    let elements_per_slot = u64::from(StorageLocation::word_bytes()) / bytes;
                    return Some(len.div_ceil(elements_per_slot).max(1));
                }
                let slots = self.storage_slots_inner(element, span)?;
                let Some(slots) = len.checked_mul(slots) else {
                    self.unsupported_size(span, "fixed-size storage array");
                    return None;
                };
                Some(slots.max(1))
            }
            _ => Some(1),
        }
    }

    fn sequence_slots(&self, tys: impl Iterator<Item = Ty<'gcx>>, span: Span) -> Option<u64> {
        let mut cursor = StorageCursor::new(false);
        for ty in tys {
            let encoding = self.packed_encoding(ty);
            let slots = self.storage_slots_inner(ty, span)?;
            if cursor.take(encoding, slots).is_none() {
                self.unsupported_size(span, "storage struct");
                return None;
            }
        }
        let Some(slots) = cursor.slots() else {
            self.unsupported_size(span, "storage struct");
            return None;
        };
        Some(slots.max(1))
    }

    fn unsupported_size(&self, span: Span, kind: &str) {
        self.gcx.dcx().err(format!("{kind} is too large for codegen")).span(span).emit();
    }
}
