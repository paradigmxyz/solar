//! Type-directed HIR to MIR and ABI shape conversion.

use crate::mir::{
    AbiParamType, AbiType, MemoryObjectKind, MemoryObjectLayout, MirType, SliceLocation,
};
use solar_ast::{DataLocation, TypeSize};
use solar_data_structures::map::FxHashSet;
use solar_sema::{
    Gcx,
    hir::{ElementaryType, StructId},
    ty::{Ty, TyKind},
};

/// Converts checked HIR types while carrying recursion state for aggregates.
pub(super) struct TypeLowerer<'gcx> {
    gcx: Gcx<'gcx>,
    seen_structs: FxHashSet<StructId>,
}

impl<'gcx> TypeLowerer<'gcx> {
    pub(super) fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, seen_structs: FxHashSet::default() }
    }

    /// Converts a checked Solidity type to its coarse MIR representation.
    pub(super) fn mir_type(ty: Ty<'_>) -> MirType {
        if let TyKind::Ref(inner, DataLocation::Calldata) = ty.kind
            && matches!(
                inner.peel_refs().kind,
                TyKind::DynArray(_)
                    | TyKind::Slice(_)
                    | TyKind::Array(_, _)
                    | TyKind::Struct(_)
                    | TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes)
            )
        {
            return MirType::Slice(SliceLocation::Calldata);
        }
        if matches!(ty.kind, TyKind::Ref(_, DataLocation::Storage)) {
            return MirType::StoragePtr;
        }
        match ty.peel_refs().kind {
            TyKind::Elementary(elementary) => match elementary {
                ElementaryType::Bool => MirType::Bool,
                ElementaryType::Address(_) => MirType::Address,
                ElementaryType::Int(size) => MirType::Int(TypeSize::new_int_bits(size.bits())),
                ElementaryType::UInt(size) => MirType::UInt(TypeSize::new_int_bits(size.bits())),
                ElementaryType::Fixed(size, _) => MirType::Int(TypeSize::new_int_bits(size.bits())),
                ElementaryType::UFixed(size, _) => {
                    MirType::UInt(TypeSize::new_int_bits(size.bits()))
                }
                ElementaryType::FixedBytes(size) => MirType::FixedBytes(size),
                ElementaryType::String | ElementaryType::Bytes => {
                    MirType::MemoryObject(MemoryObjectKind::Bytes)
                }
            },
            TyKind::Mapping(_, _) => MirType::StoragePtr,
            TyKind::DynArray(_) | TyKind::Slice(_) => {
                MirType::MemoryObject(MemoryObjectKind::DynamicArray)
            }
            TyKind::Array(_, _) => MirType::MemoryObject(MemoryObjectKind::FixedArray),
            TyKind::Struct(_) => MirType::MemoryObject(MemoryObjectKind::Struct),
            TyKind::Fn(_) => MirType::Function,
            TyKind::Enum(_) => MirType::UInt(TypeSize::new_int_bits(8)),
            TyKind::Udvt(underlying, _) => Self::mir_type(underlying),
            TyKind::Contract(_) | TyKind::Super(_) => MirType::Address,
            _ => MirType::uint256(),
        }
    }

    /// Returns the MIR representation used for a function return value.
    pub(super) fn mir_return_type(ty: Ty<'_>) -> MirType {
        Self::mir_type(ty)
    }

    /// Builds the ABI input shape for a function parameter.
    pub(super) fn abi_param_type(&mut self, ty: Ty<'gcx>) -> Option<AbiParamType> {
        self.seen_structs.clear();
        self.abi_param_type_inner(ty, ty.loc().unwrap_or(DataLocation::Memory))
    }

    /// Builds the ABI output shape for a function return value.
    pub(super) fn abi_type(&mut self, ty: Ty<'gcx>) -> Option<AbiType> {
        self.seen_structs.clear();
        self.abi_type_inner(ty, ty.loc().unwrap_or(DataLocation::Memory))
    }

    /// Builds the ABI shape of a return value, which is always encoded from
    /// memory even when its source HIR type names calldata.
    pub(super) fn abi_return_type(&mut self, ty: Ty<'gcx>) -> Option<AbiType> {
        self.abi_type(ty.with_loc_if_ref(self.gcx, DataLocation::Memory))
    }

    /// Builds the ABI return shape while retaining scalar MIR types for the ABI phase.
    pub(super) fn abi_return_param_type(&mut self, ty: Ty<'gcx>) -> Option<AbiParamType> {
        self.abi_param_type(ty.with_loc_if_ref(self.gcx, DataLocation::Memory))
    }

    /// Builds both ABI shapes for a return value in one recursive walk.
    pub(super) fn abi_return_shapes(&mut self, ty: Ty<'gcx>) -> Option<(AbiType, AbiParamType)> {
        self.seen_structs.clear();
        self.abi_return_shapes_inner(
            ty.with_loc_if_ref(self.gcx, DataLocation::Memory),
            DataLocation::Memory,
        )
    }

    /// Returns the semantic object layout for a memory-backed aggregate.
    pub(super) fn memory_layout(&self, ty: Ty<'gcx>) -> Option<MemoryObjectLayout> {
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                MemoryObjectLayout::Bytes
            }
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                MemoryObjectLayout::DynamicArray { element_words: self.element_words(element) }
            }
            TyKind::Array(element, len) => MemoryObjectLayout::FixedArray {
                len: u64::try_from(len).ok()?,
                element_words: self.element_words(element),
            },
            TyKind::Struct(id) => {
                MemoryObjectLayout::Struct { fields: self.gcx.hir.strukt(id).fields.len() as u64 }
            }
            _ => return None,
        })
    }

    /// Returns the number of words used to reference one aggregate element.
    pub(super) const fn element_words(&self, _ty: Ty<'gcx>) -> u32 {
        1
    }

    fn abi_slice_location(ty: Ty<'_>) -> SliceLocation {
        if ty.is_ref_at(DataLocation::Calldata) {
            SliceLocation::Calldata
        } else {
            SliceLocation::Memory
        }
    }

    fn abi_param_type_inner(
        &mut self,
        ty: Ty<'gcx>,
        location: DataLocation,
    ) -> Option<AbiParamType> {
        if ty.is_ref_at(DataLocation::Storage) || matches!(ty.peel_refs().kind, TyKind::Mapping(..))
        {
            return Some(AbiParamType::Scalar(MirType::StoragePtr));
        }
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                AbiParamType::Bytes
            }
            TyKind::Elementary(_) => AbiParamType::Scalar(Self::mir_type(ty)),
            TyKind::Enum(id) => AbiParamType::Enum {
                ty: Self::mir_type(ty),
                variants: self.gcx.hir.enumm(id).variants.len() as u64,
            },
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                AbiParamType::DynamicArray(Box::new(
                    self.abi_param_type_inner(
                        element.with_loc_if_ref(self.gcx, location),
                        location,
                    )?,
                ))
            }
            TyKind::Array(element, len) => {
                AbiParamType::FixedArray {
                    element: Box::new(self.abi_param_type_inner(
                        element.with_loc_if_ref(self.gcx, location),
                        location,
                    )?),
                    len: u64::try_from(len).ok()?,
                }
            }
            TyKind::Struct(id) => {
                if !self.seen_structs.insert(id) {
                    return None;
                }
                let fields = self
                    .gcx
                    .hir
                    .strukt(id)
                    .fields
                    .iter()
                    .map(|&field| {
                        let field_ty = self.gcx.type_of_item(field.into());
                        self.abi_param_type_inner(
                            field_ty.with_loc_if_ref(self.gcx, location),
                            location,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.seen_structs.remove(&id);
                AbiParamType::Tuple(fields.into_boxed_slice())
            }
            TyKind::Udvt(underlying, _) => {
                return self.abi_param_type_inner(
                    underlying.with_loc_if_ref(self.gcx, location),
                    location,
                );
            }
            TyKind::Contract(_) | TyKind::Super(_) => AbiParamType::Scalar(MirType::Address),
            _ => AbiParamType::Scalar(Self::mir_type(ty)),
        })
    }

    fn abi_return_shapes_inner(
        &mut self,
        ty: Ty<'gcx>,
        location: DataLocation,
    ) -> Option<(AbiType, AbiParamType)> {
        let param_ty = ty.with_loc_if_ref(self.gcx, location);
        if param_ty.is_ref_at(DataLocation::Storage)
            || matches!(param_ty.peel_refs().kind, TyKind::Mapping(..))
        {
            return Some((AbiType::Word, AbiParamType::Scalar(MirType::StoragePtr)));
        }

        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                (AbiType::Bytes(Self::abi_slice_location(ty)), AbiParamType::Bytes)
            }
            TyKind::Elementary(_) => {
                (AbiType::Word, AbiParamType::Scalar(Self::mir_type(param_ty)))
            }
            TyKind::Fn(function) => (
                if function.is_external() { AbiType::Function } else { AbiType::Word },
                AbiParamType::Scalar(Self::mir_type(param_ty)),
            ),
            TyKind::Enum(id) => (
                AbiType::Word,
                AbiParamType::Enum {
                    ty: Self::mir_type(param_ty),
                    variants: self.gcx.hir.enumm(id).variants.len() as u64,
                },
            ),
            TyKind::Contract(_) | TyKind::Super(_) => {
                (AbiType::Word, AbiParamType::Scalar(MirType::Address))
            }
            TyKind::DynArray(element) => {
                let (abi_element, param_element) = self.abi_return_shapes_inner(
                    element.with_loc_if_ref(self.gcx, location),
                    location,
                )?;
                (
                    AbiType::DynamicArray {
                        element: Box::new(abi_element),
                        location: Self::abi_slice_location(ty),
                    },
                    AbiParamType::DynamicArray(Box::new(param_element)),
                )
            }
            TyKind::Slice(element) => {
                let (abi_element, param_element) = self.abi_return_shapes_inner(
                    element.with_loc_if_ref(self.gcx, location),
                    location,
                )?;
                (abi_element, AbiParamType::DynamicArray(Box::new(param_element)))
            }
            TyKind::Array(element, len) => {
                let (abi_element, param_element) = self.abi_return_shapes_inner(
                    element.with_loc_if_ref(self.gcx, location),
                    location,
                )?;
                (
                    AbiType::FixedArray {
                        element: Box::new(abi_element),
                        len: u64::try_from(len).ok()?,
                    },
                    AbiParamType::FixedArray {
                        element: Box::new(param_element),
                        len: u64::try_from(len).ok()?,
                    },
                )
            }
            TyKind::Struct(id) => {
                if !self.seen_structs.insert(id) {
                    return None;
                }
                let fields = self
                    .gcx
                    .hir
                    .strukt(id)
                    .fields
                    .iter()
                    .map(|&field| {
                        self.abi_return_shapes_inner(
                            self.gcx.type_of_item(field.into()).with_loc_if_ref(self.gcx, location),
                            location,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.seen_structs.remove(&id);
                let (abi_fields, param_fields): (Vec<_>, Vec<_>) = fields.into_iter().unzip();
                (
                    AbiType::Tuple(abi_fields.into_boxed_slice()),
                    AbiParamType::Tuple(param_fields.into_boxed_slice()),
                )
            }
            TyKind::Udvt(underlying, _) => {
                return self.abi_return_shapes_inner(underlying, location);
            }
            _ => (AbiType::Word, AbiParamType::Scalar(Self::mir_type(param_ty))),
        })
    }

    fn abi_type_inner(&mut self, ty: Ty<'gcx>, location: DataLocation) -> Option<AbiType> {
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                AbiType::Bytes(Self::abi_slice_location(ty))
            }
            TyKind::Elementary(_) => AbiType::Word,
            TyKind::Fn(function) if function.is_external() => AbiType::Function,
            TyKind::Enum(_) | TyKind::Contract(_) | TyKind::Super(_) => AbiType::Word,
            TyKind::DynArray(element) => AbiType::DynamicArray {
                element: Box::new(
                    self.abi_type_inner(element.with_loc_if_ref(self.gcx, location), location)?,
                ),
                location: Self::abi_slice_location(ty),
            },
            TyKind::Slice(element) => {
                return self.abi_type_inner(element.with_loc_if_ref(self.gcx, location), location);
            }
            TyKind::Array(element, len) => AbiType::FixedArray {
                element: Box::new(
                    self.abi_type_inner(element.with_loc_if_ref(self.gcx, location), location)?,
                ),
                len: u64::try_from(len).ok()?,
            },
            TyKind::Struct(id) => {
                if !self.seen_structs.insert(id) {
                    return None;
                }
                let fields = self
                    .gcx
                    .hir
                    .strukt(id)
                    .fields
                    .iter()
                    .map(|&field| {
                        self.abi_type_inner(
                            self.gcx.type_of_item(field.into()).with_loc_if_ref(self.gcx, location),
                            location,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.seen_structs.remove(&id);
                AbiType::Tuple(fields.into_boxed_slice())
            }
            TyKind::Udvt(underlying, _) => {
                return self
                    .abi_type_inner(underlying.with_loc_if_ref(self.gcx, location), location);
            }
            _ => AbiType::Word,
        })
    }
}
