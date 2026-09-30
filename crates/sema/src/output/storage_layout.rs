use crate::{
    hir,
    ty::{Gcx, Ty, TyKind},
};
use alloy_primitives::U256;
use serde::Serialize;
use solar_ast::{DataLocation, ElementaryType};
use solar_data_structures::map::{FxHashMap, FxIndexMap, IndexEntry};

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
    /// The ERC-7201 namespaces the contract declares or inherits, from each struct documented
    /// `@custom:storage-location erc7201:<id>`, keyed `erc7201:<id>` as OpenZeppelin's upgrade
    /// tooling keys them. Each member's slot is relative to the namespace's location, as a struct
    /// member's is to the struct. `solc` has no such field, so it is left out when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<FxIndexMap<String, Vec<StorageLayoutEntry>>>,
    /// The records of the mapping groups documented `@custom:solar-fuse <group>`, keyed by the
    /// slot of each group's first mapping, whose hash with a key locates the group's record for
    /// that key as a mapping's slot locates its value. Each member is the value one mapping
    /// keeps in the record, with its slot relative to the record's start. `solc` has no such
    /// field, so it is left out when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fused: Option<FxIndexMap<String, Vec<StorageLayoutEntry>>>,
    /// The arrays documented `@custom:solar-inline`, as `storage` lists them. While such an array
    /// has at most `31 / size` elements of `size` bytes, its slot holds them, element `i` at byte
    /// `i * size`, and its length in the top byte; a longer array has the standard layout, whose
    /// length leaves the top byte zero. `solc` has no such field, so it is left out when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline: Option<Vec<StorageLayoutEntry>>,
    /// The mappings documented `@custom:solar-bitmap`, as `storage` lists them. The value for key
    /// `k` is bit `k % 256` of the word at `keccak256((k / 256) . slot)`. `solc` has no such
    /// field, so it is left out when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmaps: Option<Vec<StorageLayoutEntry>>,
    /// The handle fields of the structs documented `@custom:solar-handle <field> <dictionary>`
    /// that the contract stores, keyed by the struct's type. `types` lists a handle field as
    /// `uint72`: it keeps one plus the index of the field's value in its dictionary, the storage
    /// array this lists as `storage` does, or zero for the value zero. `solc` has no such field,
    /// so it is left out when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles: Option<FxIndexMap<String, Vec<StorageLayoutHandle>>>,
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

/// A handle field of a struct documented `@custom:solar-handle`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageLayoutHandle {
    /// The field's name, its label among the struct's members.
    pub member: String,
    /// The dictionary the field's handle indexes.
    pub dictionary: StorageLayoutEntry,
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
        StorageLayoutBuilder::new(
            self,
            self.contract_fully_qualified_name(contract_id).to_string(),
            DataLocation::Storage,
        )
        .build_contract(contract_id)
    }

    /// Returns the transient storage layout for the given contract.
    pub fn transient_storage_layout(self, contract_id: hir::ContractId) -> StorageLayoutOutput {
        StorageLayoutBuilder::new(
            self,
            self.contract_fully_qualified_name(contract_id).to_string(),
            DataLocation::Transient,
        )
        .build_contract(contract_id)
    }

    /// Returns the storage layout for a struct rooted at `base_slot`.
    ///
    /// Top-level fields use absolute slots; nested struct members retain relative slots.
    /// Slot arithmetic wraps modulo 2^256, just like EVM storage addressing.
    pub fn storage_layout_for_struct(
        self,
        struct_id: hir::StructId,
        base_slot: U256,
    ) -> StorageLayoutOutput {
        let strukt = self.hir.strukt(struct_id);
        let contract_name = strukt.contract.map_or_else(
            || format!("{}:{}", self.hir.source(strukt.source).file.name.display(), strukt.name),
            |id| self.contract_fully_qualified_name(id).to_string(),
        );
        let mut builder = StorageLayoutBuilder::new(self, contract_name, DataLocation::Storage);
        let storage = builder.layout_fields(strukt.fields, &mut StorageCursor::new(base_slot));
        let types = (!builder.types.is_empty()).then_some(builder.types);
        StorageLayoutOutput {
            storage,
            types,
            namespaces: None,
            fused: None,
            inline: None,
            bitmaps: None,
            handles: None,
        }
    }
}

struct StorageLayoutBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    contract_name: String,
    location: DataLocation,
    types: FxIndexMap<String, StorageLayoutType>,
}

impl<'gcx> StorageLayoutBuilder<'gcx> {
    fn new(gcx: Gcx<'gcx>, contract_name: String, location: DataLocation) -> Self {
        Self { gcx, contract_name, location, types: FxIndexMap::default() }
    }

    fn build_contract(mut self, contract_id: hir::ContractId) -> StorageLayoutOutput {
        let contract = self.gcx.hir.contract(contract_id);
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
            std::slice::from_ref(&contract_id)
        } else {
            contract.linearized_bases
        }
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
        let mut cursor = StorageCursor::new(base_slot);
        let mut storage = Vec::new();
        let mut slots = FxHashMap::default();
        let mut inline = Vec::new();
        let mut bitmaps = Vec::new();

        for &base in &bases {
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
                if self.gcx.hir.solar_inline(variable_id).is_some()
                    && matches!(ty.peel_refs().kind, TyKind::DynArray(_))
                {
                    inline.push(self.storage_entry(variable_id, slot, offset, ty_name.clone()));
                }
                if self.gcx.hir.solar_bitmap(variable_id).is_some()
                    && matches!(ty.peel_refs().kind, TyKind::Mapping(..))
                {
                    bitmaps.push(self.storage_entry(variable_id, slot, offset, ty_name.clone()));
                }
                storage.push(self.storage_entry(variable_id, slot, offset, ty_name));
                slots.insert(variable_id, slot);
            }
        }

        let mut namespaces = FxIndexMap::default();
        if matches!(self.location, DataLocation::Storage) {
            for &base in &bases {
                let structs =
                    self.gcx.hir.contract(base).items.iter().filter_map(hir::ItemId::as_struct);
                for id in structs {
                    if let Some((namespace, _)) = self.gcx.hir.erc7201_namespace(id)
                        && let IndexEntry::Vacant(entry) =
                            namespaces.entry(format!("erc7201:{namespace}"))
                    {
                        let fields = self.gcx.hir.strukt(id).fields;
                        entry.insert(
                            self.layout_fields(fields, &mut StorageCursor::new(U256::ZERO)),
                        );
                    }
                }
            }
        }

        let mut fused = FxIndexMap::default();
        if matches!(self.location, DataLocation::Storage) {
            let mut groups = FxIndexMap::<_, Vec<_>>::default();
            for &base in &bases {
                for variable_id in self.gcx.hir.contract(base).variables() {
                    if let Some((group, _)) = self.gcx.hir.solar_fuse(variable_id)
                        && let TyKind::Mapping(_, value) =
                            self.gcx.type_of_item(variable_id.into()).peel_refs().kind
                    {
                        groups.entry((base, group)).or_default().push((variable_id, value));
                    }
                }
            }
            for members in groups.values() {
                let [(first, _), _, ..] = members.as_slice() else { continue };
                let Some(record) = slots.get(first) else { continue };
                let mut cursor = StorageCursor::new(U256::ZERO);
                let mut entries = Vec::with_capacity(members.len());
                for &(id, value) in members {
                    let value = value.with_loc_if_ref(self.gcx, DataLocation::Storage);
                    let ty_name = self.generate_type(value);
                    let (slot, offset) = self.place_type(value, &mut cursor);
                    entries.push(self.storage_entry(id, slot, offset, ty_name));
                }
                fused.insert(record.to_string(), entries);
            }
        }

        let mut handles = FxIndexMap::default();
        if matches!(self.location, DataLocation::Storage) {
            for &base in &bases {
                let structs =
                    self.gcx.hir.contract(base).items.iter().filter_map(hir::ItemId::as_struct);
                for id in structs {
                    let ty = self.gcx.mk_ty(TyKind::Struct(id));
                    let key =
                        self.storage_type_key(ty.with_loc_if_ref(self.gcx, DataLocation::Storage));
                    if !self.types.contains_key(&key) {
                        continue;
                    }
                    let mut entries = Vec::new();
                    for (field, dictionary) in self.gcx.hir.solar_handles(id) {
                        let Some(&slot) = slots.get(&dictionary) else { continue };
                        let ty_name =
                            self.storage_type_key(self.gcx.type_of_item(dictionary.into()));
                        entries.push(StorageLayoutHandle {
                            member: self.gcx.hir.variable(field).name.unwrap().to_string(),
                            dictionary: self.storage_entry(dictionary, slot, 0, ty_name),
                        });
                    }
                    if !entries.is_empty() {
                        handles.insert(key, entries);
                    }
                }
            }
        }

        let types = (!self.types.is_empty()).then_some(self.types);
        let namespaces = (!namespaces.is_empty()).then_some(namespaces);
        let fused = (!fused.is_empty()).then_some(fused);
        let inline = (!inline.is_empty()).then_some(inline);
        let bitmaps = (!bitmaps.is_empty()).then_some(bitmaps);
        let handles = (!handles.is_empty()).then_some(handles);
        StorageLayoutOutput { storage, types, namespaces, fused, inline, bitmaps, handles }
    }

    fn layout_members(&mut self, fields: &[hir::VariableId]) -> (Vec<StorageLayoutEntry>, U256) {
        let mut cursor = StorageCursor::new(U256::ZERO);
        let members = self.layout_fields(fields, &mut cursor);
        (members, cursor.size())
    }

    fn layout_fields(
        &mut self,
        fields: &[hir::VariableId],
        cursor: &mut StorageCursor,
    ) -> Vec<StorageLayoutEntry> {
        let mut members = Vec::with_capacity(fields.len());
        for &field in fields {
            let ty = self.field_type(field);
            let ty_name = self.generate_type(ty);
            let (slot, offset) = self.place_type(ty, cursor);
            members.push(self.storage_entry(field, slot, offset, ty_name));
        }
        members
    }

    /// The type of the struct field `field` in storage: `uint72` for a handle field, which keeps
    /// one plus the index of its value in its dictionary.
    fn field_type(&self, field: hir::VariableId) -> Ty<'gcx> {
        if let Some(hir::ItemId::Struct(id)) = self.gcx.hir.variable(field).parent
            && self.gcx.hir.solar_handles(id).any(|(handle, _)| handle == field)
        {
            return self.gcx.types.uint(72);
        }
        self.gcx.type_of_item(field.into())
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
            self.slot = self.slot.wrapping_add(U256::from(1));
            self.offset = 0;
        }
    }

    fn advance(&mut self, slots: U256) {
        self.slot = self.slot.wrapping_add(slots);
        self.offset = 0;
    }

    fn size(self) -> U256 {
        (self.slot + U256::from(u8::from(self.offset != 0))) * U256::from(32)
    }
}

fn slots_for(bytes: U256) -> U256 {
    bytes / U256::from(32) + U256::from(u8::from(bytes % U256::from(32) != U256::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;
    use solar_interface::Session;
    use std::{ops::ControlFlow, path::PathBuf};

    #[test]
    fn struct_storage_roots() {
        let mut compiler = Compiler::new(Session::builder().with_test_emitter().build());
        compiler.enter_mut(|compiler| {
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(
                    PathBuf::from("test.sol"),
                    r#"
                struct Nested { uint128 a; uint128 b; }
                struct Data {
                    address owner;
                    bool paused;
                    Nested nested;
                    uint256[2] values;
                    mapping(uint256 => address) accounts;
                    uint256[] dynamicValues;
                }
                "#,
                )
                .unwrap();
            let mut parser = compiler.parse();
            parser.add_file(file);
            parser.parse();
            assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
            let gcx = compiler.gcx();
            let id = gcx
                .hir
                .strukt_ids()
                .find(|&id| gcx.hir.strukt(id).name.as_str() == "Data")
                .unwrap();
            let zero = gcx.storage_layout_for_struct(id, U256::ZERO);
            for root in [U256::from(0x1000), U256::MAX] {
                let layout = gcx.storage_layout_for_struct(id, root);
                assert_eq!(layout.storage.len(), 6);
                for (entry, relative) in layout.storage.iter().zip(&zero.storage) {
                    assert_eq!(
                        entry.slot,
                        root.wrapping_add(relative.slot.parse().unwrap()).to_string()
                    );
                    assert_eq!(entry.offset, relative.offset);
                }
                assert_eq!(
                    serde_json::to_value(&layout.types).unwrap(),
                    serde_json::to_value(&zero.types).unwrap()
                );
            }
            assert_eq!(
                zero.storage.iter().map(|s| (s.slot.as_str(), s.offset)).collect::<Vec<_>>(),
                [("0", 0), ("0", 20), ("1", 0), ("2", 0), ("4", 0), ("5", 0)]
            );
            assert_eq!(zero.storage[0].contract, "test.sol:Data");
        });
    }
}
