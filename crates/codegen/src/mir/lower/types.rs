//! Type-directed HIR to MIR and ABI shape conversion.

use crate::mir::{
    AbiParamType, AbiType, AbiWordValidator, MemoryObjectKind, MemoryObjectLayout, MirType,
    SliceLocation, ValueLayout,
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

    pub(super) fn mir_type(ty: Ty<'_>) -> MirType {
        Self::value_layout(ty).mir_type()
    }

    /// Converts a checked Solidity type to its coarse MIR representation.
    pub(super) fn value_layout(ty: Ty<'_>) -> ValueLayout {
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
            return ValueLayout::Slice(SliceLocation::Calldata);
        }
        if matches!(ty.kind, TyKind::Ref(_, DataLocation::Storage)) {
            return ValueLayout::StoragePtr;
        }
        match ty.peel_refs().kind {
            TyKind::Elementary(elementary) => match elementary {
                ElementaryType::Bool => ValueLayout::Bool,
                ElementaryType::Address(_) => ValueLayout::Address,
                ElementaryType::Int(size) => ValueLayout::Int(TypeSize::new_int_bits(size.bits())),
                ElementaryType::UInt(size) => {
                    ValueLayout::UInt(TypeSize::new_int_bits(size.bits()))
                }
                ElementaryType::Fixed(size, _) => {
                    ValueLayout::Int(TypeSize::new_int_bits(size.bits()))
                }
                ElementaryType::UFixed(size, _) => {
                    ValueLayout::UInt(TypeSize::new_int_bits(size.bits()))
                }
                ElementaryType::FixedBytes(size) => ValueLayout::FixedBytes(size),
                ElementaryType::String | ElementaryType::Bytes => {
                    ValueLayout::MemoryObject(MemoryObjectKind::Bytes)
                }
            },
            TyKind::Mapping(_, _) => ValueLayout::StoragePtr,
            TyKind::DynArray(_) | TyKind::Slice(_) => {
                ValueLayout::MemoryObject(MemoryObjectKind::DynamicArray)
            }
            TyKind::Array(_, _) => ValueLayout::MemoryObject(MemoryObjectKind::FixedArray),
            TyKind::Struct(_) => ValueLayout::MemoryObject(MemoryObjectKind::Struct),
            TyKind::Fn(_) => ValueLayout::Function,
            TyKind::Enum(_) => ValueLayout::UInt(TypeSize::new_int_bits(8)),
            TyKind::Udvt(underlying, _) => Self::value_layout(underlying),
            TyKind::Contract(_) | TyKind::Super(_) => ValueLayout::Address,
            _ => ValueLayout::uint256(),
        }
    }

    /// Carries raw Solidity scalar bits across calls, where assembly may observe them.
    pub(super) fn mir_signature_type(ty: Ty<'_>) -> MirType {
        match Self::mir_type(ty) {
            MirType::I1 | MirType::I160 => MirType::I256,
            ty => ty,
        }
    }

    /// Returns the MIR representation used for a function return value.
    pub(super) fn mir_return_type(ty: Ty<'_>) -> MirType {
        Self::mir_signature_type(ty)
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

    /// Returns the type a return value is encoded from and decoded into.
    ///
    /// A reference return is encoded from memory even when its source HIR type names calldata,
    /// except for a storage reference, which keeps its location and travels as its slot word.
    pub(super) fn return_encoding_ty(gcx: Gcx<'gcx>, ty: Ty<'gcx>) -> Ty<'gcx> {
        if ty.encodes_as_slot() {
            return ty;
        }
        ty.with_loc_if_ref(gcx, DataLocation::Memory)
    }

    /// Builds the ABI shape of a return value.
    ///
    /// A storage reference is one word holding its slot number.
    pub(super) fn abi_return_type(&mut self, ty: Ty<'gcx>) -> Option<AbiType> {
        if ty.encodes_as_slot() {
            return Some(AbiType::Word(None));
        }
        self.abi_type(ty.with_loc_if_ref(self.gcx, DataLocation::Memory))
    }

    /// Builds the ABI return shape while retaining scalar MIR types for ABI lowering.
    pub(super) fn abi_return_param_type(&mut self, ty: Ty<'gcx>) -> Option<AbiParamType> {
        self.abi_param_type(Self::return_encoding_ty(self.gcx, ty))
    }

    /// Builds both ABI shapes for a return value in one recursive walk.
    pub(super) fn abi_return_shapes(&mut self, ty: Ty<'gcx>) -> Option<(AbiType, AbiParamType)> {
        self.seen_structs.clear();
        self.abi_return_shapes_inner(Self::return_encoding_ty(self.gcx, ty), DataLocation::Memory)
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
        if ty.encodes_as_slot() {
            return Some(AbiParamType::Scalar(ValueLayout::StoragePtr));
        }
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                AbiParamType::Bytes
            }
            TyKind::Elementary(_) => AbiParamType::Scalar(Self::value_layout(ty)),
            TyKind::Enum(id) => AbiParamType::Enum {
                ty: Self::value_layout(ty),
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
            TyKind::Contract(_) | TyKind::Super(_) => AbiParamType::Scalar(ValueLayout::Address),
            _ => AbiParamType::Scalar(Self::value_layout(ty)),
        })
    }

    fn abi_return_shapes_inner(
        &mut self,
        ty: Ty<'gcx>,
        location: DataLocation,
    ) -> Option<(AbiType, AbiParamType)> {
        if ty.encodes_as_slot() {
            return Some((AbiType::Word(None), AbiParamType::Scalar(ValueLayout::StoragePtr)));
        }
        let param_ty = ty.with_loc_if_ref(self.gcx, location);

        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                (AbiType::Bytes(Self::abi_slice_location(ty)), AbiParamType::Bytes)
            }
            TyKind::Elementary(_) => {
                let mir_ty = Self::value_layout(param_ty);
                (AbiType::Word(AbiWordValidator::from_layout(mir_ty)), AbiParamType::Scalar(mir_ty))
            }
            TyKind::Fn(function) => (
                if function.is_external() { AbiType::Function } else { AbiType::Word(None) },
                AbiParamType::Scalar(Self::value_layout(param_ty)),
            ),
            TyKind::Enum(id) => {
                let variants = self.gcx.hir.enumm(id).variants.len() as u64;
                (
                    AbiType::Word(Some(AbiWordValidator::EnumRange(variants))),
                    AbiParamType::Enum { ty: Self::value_layout(param_ty), variants },
                )
            }
            TyKind::Contract(_) | TyKind::Super(_) => (
                AbiType::Word(AbiWordValidator::from_layout(ValueLayout::Address)),
                AbiParamType::Scalar(ValueLayout::Address),
            ),
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
            _ => (AbiType::Word(None), AbiParamType::Scalar(Self::value_layout(param_ty))),
        })
    }

    fn abi_type_inner(&mut self, ty: Ty<'gcx>, location: DataLocation) -> Option<AbiType> {
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                AbiType::Bytes(Self::abi_slice_location(ty))
            }
            TyKind::Elementary(_) => {
                AbiType::Word(AbiWordValidator::from_layout(Self::value_layout(ty)))
            }
            TyKind::Fn(function) if function.is_external() => AbiType::Function,
            TyKind::Enum(id) => AbiType::Word(Some(AbiWordValidator::EnumRange(
                self.gcx.hir.enumm(id).variants.len() as u64,
            ))),
            TyKind::Contract(_) | TyKind::Super(_) => {
                AbiType::Word(AbiWordValidator::from_layout(ValueLayout::Address))
            }
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
            _ => AbiType::Word(None),
        })
    }
}
