//! ABI type shapes and function-boundary metadata for HIR lowering.

use super::Lowerer;
use crate::mir::{AbiLayout, AbiLayoutRef, AbiType, SliceLocation};
use solar_ast::ElementaryType;
use solar_data_structures::map::FxHashSet;
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_sema::{
    hir,
    ty::{Ty, TyKind},
};

/// ABI metadata prepared while lowering one HIR function. The body lowerer
/// consumes the return types; the later `lower-abi` phase consumes the layout.
pub(super) struct FunctionAbi<'gcx> {
    pub(super) return_types: Vec<Ty<'gcx>>,
    pub(super) external_arg_head_size: u64,
    pub(super) external_static_return_size: u64,
    pub(super) return_layout: Option<AbiLayoutRef>,
}

impl<'gcx> Lowerer<'gcx> {
    /// Prepares the ABI metadata needed by the body and by the later ABI phase.
    pub(super) fn lower_function_abi(
        &mut self,
        func_id: hir::FunctionId,
        uses_external_abi: bool,
    ) -> Result<FunctionAbi<'gcx>, ErrorGuaranteed> {
        let function = self.gcx.hir.function(func_id);
        let parameters = function.parameters;
        let returns = function.returns;
        let return_types =
            returns.iter().map(|&id| self.gcx.type_of_item(id.into())).collect::<Vec<_>>();

        let external_arg_head_size = if uses_external_abi {
            self.abi_head_size_sum(parameters.iter().map(|&id| self.gcx.type_of_item(id.into())))?
        } else {
            0
        };
        let external_static_return_size =
            if uses_external_abi && !return_types.iter().any(|&ty| self.abi_is_dynamic(ty)) {
                self.abi_head_size_sum(return_types.iter().copied())?
            } else {
                0
            };
        let return_layout = if uses_external_abi && !return_types.is_empty() {
            let types = return_types
                .iter()
                .map(|&ty| self.abi_type(ty, false).ok_or_else(|| self.abi_type_error()))
                .collect::<Result<Vec<_>, _>>()?;
            Some(self.module.intern_abi_layout(AbiLayout::new(types)))
        } else {
            None
        };

        Ok(FunctionAbi {
            return_types,
            external_arg_head_size,
            external_static_return_size,
            return_layout,
        })
    }

    pub(super) fn abi_type(&self, ty: Ty<'gcx>, calldata: bool) -> Option<AbiType> {
        let mut visiting = FxHashSet::default();
        self.abi_type_inner(ty, calldata, &mut visiting)
    }

    pub(super) fn abi_type_error(&self) -> ErrorGuaranteed {
        self.recovery_error(None, "codegen cannot materialize this ABI type")
    }

    fn abi_type_inner(
        &self,
        ty: Ty<'gcx>,
        calldata: bool,
        visiting: &mut FxHashSet<hir::StructId>,
    ) -> Option<AbiType> {
        if matches!(ty.kind, TyKind::Mapping(..) | TyKind::Ref(_, solar_ast::DataLocation::Storage))
        {
            return Some(AbiType::Word);
        }
        let location = if calldata { SliceLocation::Calldata } else { SliceLocation::Memory };
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                AbiType::Bytes(location)
            }
            TyKind::DynArray(element) => AbiType::DynamicArray {
                element: Box::new(self.abi_type_inner(element, false, visiting)?),
                location,
            },
            TyKind::Slice(inner) => self.abi_type_inner(inner, calldata, visiting)?,
            TyKind::Array(element, len) => {
                let len = match u64::try_from(len) {
                    Ok(len) => len,
                    Err(_) => {
                        self.abi_head_size_overflow();
                        return None;
                    }
                };
                let element = Box::new(self.abi_type_inner(element, false, visiting)?);
                if len.checked_mul(element.head_size()).is_none() {
                    self.abi_head_size_overflow();
                    return None;
                }
                AbiType::FixedArray { element, len }
            }
            TyKind::Struct(id) => {
                if !visiting.insert(id) {
                    return None;
                }
                // A field's sema type may carry a storage location ref, but a
                // field of a memory/calldata aggregate is a value, never a
                // storage pointer: peel it so the top-level library
                // storage-parameter guard cannot collapse it to one word.
                let fields = self
                    .gcx
                    .struct_field_types(id)
                    .iter()
                    .map(|&field| self.abi_type_inner(field.peel_refs(), false, visiting))
                    .collect::<Option<Vec<_>>>()?;
                visiting.remove(&id);
                AbiType::Tuple(fields.into())
            }
            TyKind::Tuple(fields) => AbiType::Tuple(
                fields
                    .iter()
                    .map(|&field| self.abi_type_inner(field.peel_refs(), false, visiting))
                    .collect::<Option<Vec<_>>>()?
                    .into(),
            ),
            TyKind::Udvt(inner, _) => self.abi_type_inner(inner, calldata, visiting)?,
            _ => AbiType::Word,
        })
    }

    /// Whether `ty` is encoded dynamically (offset slot in the head + data in the
    /// tail): `bytes`/`string`, dynamic arrays, and any aggregate containing one.
    pub(super) fn abi_is_dynamic(&self, ty: Ty<'gcx>) -> bool {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_)
            | TyKind::Slice(_) => true,
            TyKind::Struct(id) => {
                ty.is_recursive(self.gcx)
                    || self.gcx.struct_field_types(id).iter().any(|&f| self.abi_is_dynamic(f))
            }
            TyKind::Array(elem, _) => self.abi_is_dynamic(elem),
            TyKind::Tuple(fields) => fields.iter().any(|&f| self.abi_is_dynamic(f)),
            TyKind::Udvt(inner, _) => self.abi_is_dynamic(inner),
            _ => false,
        }
    }

    /// Static ABI head size, in bytes, of one top-level item.
    pub(super) fn abi_head_size(&self, ty: Ty<'gcx>) -> Result<u64, ErrorGuaranteed> {
        // A storage reference (a mapping, or a struct/array in storage — legal
        // for library function parameters) travels as its slot: one word.
        if matches!(ty.kind, TyKind::Mapping(..) | TyKind::Ref(_, solar_ast::DataLocation::Storage))
        {
            return Ok(32);
        }
        let ty = ty.peel_refs();
        if self.abi_is_dynamic(ty) {
            return Ok(32);
        }
        match ty.kind {
            TyKind::Array(elem, n) => {
                let len = u64::try_from(n).map_err(|_| self.abi_head_size_overflow())?;
                len.checked_mul(self.abi_head_size(elem)?)
                    .ok_or_else(|| self.abi_head_size_overflow())
            }
            TyKind::Struct(id) => {
                self.abi_head_size_sum(self.gcx.struct_field_types(id).iter().copied())
            }
            _ => Ok(32),
        }
    }

    pub(super) fn abi_head_size_sum(
        &self,
        tys: impl IntoIterator<Item = Ty<'gcx>>,
    ) -> Result<u64, ErrorGuaranteed> {
        tys.into_iter().try_fold(0u64, |size, ty| {
            size.checked_add(self.abi_head_size(ty)?).ok_or_else(|| self.abi_head_size_overflow())
        })
    }

    pub(super) fn abi_head_size_overflow(&self) -> ErrorGuaranteed {
        self.recovery_error(None, "ABI head size exceeds codegen limits")
    }
}
