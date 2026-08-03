//! Type-directed HIR to MIR and ABI shape conversion.

use crate::mir::{
    AbiParamType, AbiType, MemoryObjectKind, MemoryObjectLayout, MirType, SliceLocation,
};
use solar_ast::{DataLocation, TypeSize};
use solar_sema::{
    Gcx,
    hir::{ElementaryType, StructId},
    ty::{Ty, TyKind},
};
use std::collections::HashSet;

/// Converts checked HIR types while carrying recursion state for aggregates.
pub(super) struct TypeLowerer<'gcx> {
    gcx: Gcx<'gcx>,
    seen_structs: HashSet<StructId>,
}

impl<'gcx> TypeLowerer<'gcx> {
    pub(super) fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, seen_structs: HashSet::new() }
    }

    /// Converts a checked Solidity type to its coarse MIR representation.
    pub(super) fn mir_type(ty: Ty<'_>) -> MirType {
        if let TyKind::Ref(inner, DataLocation::Calldata) = ty.kind
            && matches!(
                inner.peel_refs().kind,
                TyKind::DynArray(_)
                    | TyKind::Slice(_)
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

    /// Builds the ABI input shape for a function parameter.
    pub(super) fn abi_param_type(&mut self, ty: Ty<'gcx>) -> Option<AbiParamType> {
        self.seen_structs.clear();
        self.abi_param_type_inner(ty)
    }

    /// Builds the ABI output shape for a function return value.
    pub(super) fn abi_type(&mut self, ty: Ty<'gcx>) -> Option<AbiType> {
        self.seen_structs.clear();
        self.abi_type_inner(ty)
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

    fn abi_param_type_inner(&mut self, ty: Ty<'gcx>) -> Option<AbiParamType> {
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
                AbiParamType::DynamicArray(Box::new(self.abi_param_type_inner(element)?))
            }
            TyKind::Array(element, len) => AbiParamType::FixedArray {
                element: Box::new(self.abi_param_type_inner(element)?),
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
                    .map(|&field| self.abi_param_type_inner(self.gcx.type_of_item(field.into())))
                    .collect::<Option<Vec<_>>>()?;
                self.seen_structs.remove(&id);
                AbiParamType::Tuple(fields.into_boxed_slice())
            }
            TyKind::Udvt(underlying, _) => return self.abi_param_type_inner(underlying),
            TyKind::Contract(_) | TyKind::Super(_) => AbiParamType::Scalar(MirType::Address),
            _ => AbiParamType::Scalar(Self::mir_type(ty)),
        })
    }

    fn abi_type_inner(&mut self, ty: Ty<'gcx>) -> Option<AbiType> {
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes) => {
                AbiType::Bytes(if ty.is_ref_at(DataLocation::Calldata) {
                    SliceLocation::Calldata
                } else {
                    SliceLocation::Memory
                })
            }
            TyKind::Elementary(_) => AbiType::Word,
            TyKind::Enum(_) | TyKind::Contract(_) | TyKind::Super(_) => AbiType::Word,
            TyKind::DynArray(element) => AbiType::DynamicArray {
                element: Box::new(self.abi_type_inner(element)?),
                location: if ty.is_ref_at(DataLocation::Calldata) {
                    SliceLocation::Calldata
                } else {
                    SliceLocation::Memory
                },
            },
            TyKind::Slice(element) => return self.abi_type_inner(element),
            TyKind::Array(element, len) => AbiType::FixedArray {
                element: Box::new(self.abi_type_inner(element)?),
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
                    .map(|&field| self.abi_type_inner(self.gcx.type_of_item(field.into())))
                    .collect::<Option<Vec<_>>>()?;
                self.seen_structs.remove(&id);
                AbiType::Tuple(fields.into_boxed_slice())
            }
            TyKind::Udvt(underlying, _) => return self.abi_type_inner(underlying),
            _ => AbiType::Word,
        })
    }
}
