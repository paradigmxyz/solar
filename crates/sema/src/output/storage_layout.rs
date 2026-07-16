use crate::{
    hir,
    ty::{Gcx, Ty, TyKind},
};
use alloy_primitives::U256;
use serde::Serialize;
use solar_ast::{DataLocation, ElementaryType};
use solar_data_structures::map::FxIndexMap;

/// Storage layout in solc's Standard JSON `storageLayout` and `transientStorageLayout` output
/// fields.
///
/// Created by [`Gcx::storage_layout`] and [`Gcx::transient_storage_layout`].
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageLayoutOutput {
    pub storage: Vec<StorageLayoutEntry>,
    /// `solc` emits `null` rather than an empty object when no storage types are present.
    pub types: Option<FxIndexMap<String, StorageLayoutType>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageLayoutEntry {
    pub ast_id: u64,
    pub contract: String,
    pub label: String,
    pub offset: u64,
    pub slot: String,
    pub r#type: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageLayoutType {
    pub encoding: StorageEncoding,
    pub label: String,
    pub number_of_bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<StorageLayoutMember>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageEncoding {
    #[default]
    Inplace,
    Mapping,
    DynamicArray,
    Bytes,
}

pub type StorageLayoutMember = StorageLayoutEntry;

impl<'gcx> Gcx<'gcx> {
    /// Returns the storage layout for the given contract.
    pub fn storage_layout(self, contract_id: hir::ContractId) -> StorageLayoutOutput {
        StorageLayoutBuilder::new(self, contract_id, DataLocation::Storage).build()
    }

    /// Returns the transient storage layout for the given contract.
    pub fn transient_storage_layout(self, contract_id: hir::ContractId) -> StorageLayoutOutput {
        StorageLayoutBuilder::new(self, contract_id, DataLocation::Transient).build()
    }
}

struct StorageLayoutBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    contract_id: hir::ContractId,
    contract_name: String,
    location: DataLocation,
    types: FxIndexMap<String, StorageLayoutType>,
}

impl<'gcx> StorageLayoutBuilder<'gcx> {
    fn new(gcx: Gcx<'gcx>, contract_id: hir::ContractId, location: DataLocation) -> Self {
        Self {
            gcx,
            contract_id,
            contract_name: gcx.contract_fully_qualified_name(contract_id).to_string(),
            location,
            types: FxIndexMap::default(),
        }
    }

    fn build(mut self) -> StorageLayoutOutput {
        let contract = self.gcx.hir.contract(self.contract_id);
        let base_slot = match self.location {
            DataLocation::Storage => contract.layout.map_or(U256::ZERO, |layout| {
                self.gcx
                    .eval_const(layout)
                    .ok()
                    .and_then(|value| value.as_u256())
                    .unwrap_or_default()
            }),
            DataLocation::Transient => U256::ZERO,
            DataLocation::Memory | DataLocation::Calldata => unreachable!(),
        };
        let bases = if contract.linearized_bases.is_empty() {
            std::slice::from_ref(&self.contract_id)
        } else {
            contract.linearized_bases
        }
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
        let mut cursor = StorageCursor::new(base_slot);
        let mut storage = Vec::new();

        for base in bases {
            for variable_id in self.gcx.hir.contract(base).variables() {
                let variable = self.gcx.hir.variable(variable_id);
                let is_transient = variable.data_location == Some(DataLocation::Transient);
                if variable.is_constant()
                    || variable.is_immutable()
                    || is_transient != matches!(self.location, DataLocation::Transient)
                {
                    continue;
                }

                let ty = self.gcx.type_of_item(variable_id.into());
                let ty_name = self.generate_type(ty);
                let (slot, offset) = self.place_type(ty, &mut cursor);
                storage.push(self.storage_entry(variable_id, slot, offset, ty_name));
            }
        }

        let types = (!self.types.is_empty()).then_some(self.types);
        StorageLayoutOutput { storage, types }
    }

    fn layout_members(&mut self, fields: &[hir::VariableId]) -> (Vec<StorageLayoutEntry>, U256) {
        let mut cursor = StorageCursor::new(U256::ZERO);
        let mut members = Vec::with_capacity(fields.len());
        for &field in fields {
            let ty = self.gcx.type_of_item(field.into());
            let ty_name = self.generate_type(ty);
            let (slot, offset) = self.place_type(ty, &mut cursor);
            members.push(self.storage_entry(field, slot, offset, ty_name));
        }
        (members, cursor.size())
    }

    fn storage_entry(
        &self,
        variable_id: hir::VariableId,
        slot: U256,
        offset: u64,
        ty: String,
    ) -> StorageLayoutEntry {
        StorageLayoutEntry {
            ast_id: self.gcx.hir.global_item_id(variable_id) as u64,
            contract: self.contract_name.clone(),
            label: self.gcx.hir.variable(variable_id).name.unwrap().to_string(),
            offset,
            slot: slot.to_string(),
            r#type: ty,
        }
    }

    fn place_type(&mut self, ty: Ty<'gcx>, cursor: &mut StorageCursor) -> (U256, u64) {
        let bytes = self.storage_bytes(ty);
        if !self.is_packable(ty) {
            cursor.align();
            let slot = cursor.slot;
            cursor.advance(slots_for(bytes));
            return (slot, 0);
        }

        let bytes = bytes.to::<u64>();
        if cursor.offset + bytes > 32 {
            cursor.align();
        }
        let (slot, offset) = (cursor.slot, cursor.offset);
        cursor.offset += bytes;
        if cursor.offset == 32 {
            cursor.advance(U256::from(1));
        }
        (slot, offset)
    }

    fn generate_type(&mut self, ty: Ty<'gcx>) -> String {
        let key = self.storage_type_key(ty);
        if self.types.contains_key(&key) {
            return key;
        }
        self.types.insert(key.clone(), StorageLayoutType::default());

        let location = ty.loc();
        let ty = ty.peel_refs();
        let mut info = StorageLayoutType {
            encoding: StorageEncoding::Inplace,
            label: self.storage_type_label(ty),
            number_of_bytes: self.storage_bytes(ty).to_string(),
            ..Default::default()
        };
        match ty.kind {
            TyKind::Struct(struct_id) => {
                let (members, _) = self.layout_members(self.gcx.hir.strukt(struct_id).fields);
                info.members = members;
            }
            TyKind::Mapping(key_ty, value_ty) => {
                info.encoding = StorageEncoding::Mapping;
                info.key = Some(self.generate_type(key_ty));
                info.value = Some(
                    self.generate_type(value_ty.with_loc_if_ref(self.gcx, DataLocation::Storage)),
                );
            }
            TyKind::Array(base, _) => {
                info.base = Some(self.generate_type(base.with_loc_if_ref_opt(self.gcx, location)));
            }
            TyKind::DynArray(base) => {
                info.encoding = StorageEncoding::DynamicArray;
                info.base = Some(self.generate_type(base.with_loc_if_ref_opt(self.gcx, location)));
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                info.encoding = StorageEncoding::Bytes;
            }
            TyKind::Elementary(_)
            | TyKind::Contract(_)
            | TyKind::Enum(_)
            | TyKind::Fn(_)
            | TyKind::Udvt(..) => {}
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
        self.types.insert(key.clone(), info);
        key
    }

    fn storage_type_key(&self, ty: Ty<'gcx>) -> String {
        self.storage_type_key_with(ty, None, false)
    }

    fn storage_type_key_with(
        &self,
        ty: Ty<'gcx>,
        location: Option<DataLocation>,
        pointer: bool,
    ) -> String {
        match ty.kind {
            TyKind::Ref(inner, location) => {
                let key = self.storage_type_key_with(inner, Some(location), pointer);
                if matches!(inner.peel_refs().kind, TyKind::Mapping(..)) {
                    key
                } else {
                    let pointer = if pointer { "_ptr" } else { "" };
                    format!("{key}_{location}{pointer}")
                }
            }
            TyKind::Elementary(ty) => format!("t_{}", ty.to_string().replace(' ', "_")),
            TyKind::Array(base, length) => {
                let base = base.with_loc_if_ref_opt(self.gcx, location);
                format!("t_array({}){length}", self.storage_type_key_with(base, None, pointer))
            }
            TyKind::DynArray(base) => {
                let base = base.with_loc_if_ref_opt(self.gcx, location);
                format!("t_array({})dyn", self.storage_type_key_with(base, None, pointer))
            }
            TyKind::Mapping(key, value) => format!(
                "t_mapping({},{})",
                self.storage_type_key_with(key, None, false),
                self.storage_type_key_with(
                    value.with_loc_if_ref(self.gcx, DataLocation::Storage),
                    None,
                    false,
                )
            ),
            TyKind::Contract(id) => {
                format!("t_contract({}){}", self.gcx.item_name(id), self.gcx.hir.global_item_id(id))
            }
            TyKind::Struct(id) => {
                format!("t_struct({}){}", self.gcx.item_name(id), self.gcx.hir.global_item_id(id))
            }
            TyKind::Enum(id) => {
                format!("t_enum({}){}", self.gcx.item_name(id), self.gcx.hir.global_item_id(id))
            }
            TyKind::Udvt(_, id) => {
                format!(
                    "t_userDefinedValueType({}){}",
                    self.gcx.item_name(id),
                    self.gcx.hir.global_item_id(id)
                )
            }
            TyKind::Fn(function) => {
                let kind = if function.is_external() { "external" } else { "internal" };
                let params = function
                    .parameters
                    .iter()
                    .map(|ty| self.function_type_key(*ty))
                    .collect::<Vec<_>>()
                    .join(",");
                let returns = function
                    .returns
                    .iter()
                    .map(|ty| self.function_type_key(*ty))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "t_function_{kind}_{}({params})returns({returns})",
                    function.state_mutability
                )
            }
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
    }

    fn function_type_key(&self, ty: Ty<'gcx>) -> String {
        self.storage_type_key_with(ty, None, true)
    }

    fn storage_type_label(&self, ty: Ty<'gcx>) -> String {
        match ty.kind {
            TyKind::Ref(inner, _) => self.storage_type_label(inner),
            TyKind::Elementary(ty) => ty.to_string(),
            TyKind::Array(base, length) => format!("{}[{length}]", self.storage_type_label(base)),
            TyKind::DynArray(base) => format!("{}[]", self.storage_type_label(base)),
            TyKind::Mapping(key, value) => format!(
                "mapping({} => {})",
                self.storage_type_label(key),
                self.storage_type_label(value)
            ),
            TyKind::Contract(id) => format!("contract {}", self.gcx.item_name(id)),
            TyKind::Struct(id) => format!("struct {}", self.gcx.item_canonical_name(id)),
            TyKind::Enum(id) => format!("enum {}", self.gcx.item_canonical_name(id)),
            TyKind::Udvt(_, id) => self.gcx.item_canonical_name(id).to_string(),
            TyKind::Fn(function) => {
                let params = function
                    .parameters
                    .iter()
                    .map(|ty| self.storage_type_label(*ty))
                    .collect::<Vec<_>>()
                    .join(",");
                let mut label = format!("function ({params})");
                if function.state_mutability != hir::StateMutability::NonPayable {
                    label.push(' ');
                    label.push_str(&function.state_mutability.to_string());
                }
                if function.is_external() {
                    label.push_str(" external");
                }
                if !function.returns.is_empty() {
                    let returns = function
                        .returns
                        .iter()
                        .map(|ty| self.storage_type_label(*ty))
                        .collect::<Vec<_>>()
                        .join(",");
                    label.push_str(&format!(" returns ({returns})"));
                }
                label
            }
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
    }

    fn storage_bytes(&mut self, ty: Ty<'gcx>) -> U256 {
        match ty.kind {
            TyKind::Ref(inner, _) => self.storage_bytes(inner),
            TyKind::Elementary(ty) => match ty {
                ElementaryType::Address(_) => U256::from(20),
                ElementaryType::Bool => U256::from(1),
                ElementaryType::String | ElementaryType::Bytes => U256::from(32),
                ElementaryType::Fixed(size, _)
                | ElementaryType::UFixed(size, _)
                | ElementaryType::Int(size)
                | ElementaryType::UInt(size)
                | ElementaryType::FixedBytes(size) => U256::from(size.bytes()),
            },
            TyKind::Array(base, length) => {
                let base_bytes = self.storage_bytes(base);
                let slots = if self.is_packable(base) {
                    let items_per_slot = U256::from(32) / base_bytes;
                    length / items_per_slot
                        + U256::from(u8::from(length % items_per_slot != U256::ZERO))
                } else {
                    slots_for(base_bytes) * length
                };
                slots.max(U256::from(1)) * U256::from(32)
            }
            TyKind::DynArray(_) | TyKind::Mapping(..) => U256::from(32),
            TyKind::Struct(struct_id) => {
                self.layout_members(self.gcx.hir.strukt(struct_id).fields).1
            }
            TyKind::Contract(_) => U256::from(20),
            TyKind::Enum(_) => U256::from(1),
            TyKind::Udvt(inner, _) => self.storage_bytes(inner),
            TyKind::Fn(function) if function.is_external() => U256::from(24),
            TyKind::Fn(_) => U256::from(8),
            _ => unreachable!("invalid storage type: {ty:?}"),
        }
    }

    fn is_packable(&self, ty: Ty<'gcx>) -> bool {
        matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(
                ElementaryType::Address(_)
                    | ElementaryType::Bool
                    | ElementaryType::Fixed(..)
                    | ElementaryType::UFixed(..)
                    | ElementaryType::Int(_)
                    | ElementaryType::UInt(_)
                    | ElementaryType::FixedBytes(_)
            ) | TyKind::Contract(_)
                | TyKind::Enum(_)
                | TyKind::Udvt(..)
                | TyKind::Fn(_)
        )
    }
}

#[derive(Clone, Copy)]
struct StorageCursor {
    slot: U256,
    offset: u64,
}

impl StorageCursor {
    fn new(slot: U256) -> Self {
        Self { slot, offset: 0 }
    }

    fn align(&mut self) {
        if self.offset != 0 {
            self.slot += U256::from(1);
            self.offset = 0;
        }
    }

    fn advance(&mut self, slots: U256) {
        self.slot += slots;
        self.offset = 0;
    }

    fn size(self) -> U256 {
        (self.slot + U256::from(u8::from(self.offset != 0))) * U256::from(32)
    }
}

fn slots_for(bytes: U256) -> U256 {
    bytes / U256::from(32) + U256::from(u8::from(bytes % U256::from(32) != U256::ZERO))
}
