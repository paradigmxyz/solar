//! ABI type shapes and function-boundary metadata for HIR lowering.

use super::Lowerer;
use crate::mir::{AbiLayout, AbiLayoutRef, AbiParamLayout, AbiParamType, AbiType, SliceLocation};
use solar_ast::ElementaryType;
use solar_data_structures::map::FxHashSet;
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_sema::{
    hir,
    ty::{Ty, TyKind},
};

/// ABI metadata prepared while lowering one HIR function. The body lowerer
/// consumes the return types; the later `lower-abi` phase consumes the input
/// and output layouts.
pub(super) struct FunctionAbi<'gcx> {
    pub(super) return_types: Vec<Ty<'gcx>>,
    pub(super) external_arg_head_size: u64,
    pub(super) external_static_return_size: u64,
    pub(super) return_layout: Option<AbiLayoutRef>,
    pub(super) param_layout: Option<AbiParamLayout>,
}

impl<'gcx> Lowerer<'gcx> {
    /// Prepares the ABI metadata needed by the body and by the later ABI phase.
    pub(super) fn lower_function_abi(
        &mut self,
        func_id: hir::FunctionId,
        uses_external_abi: bool,
        has_abi_inputs: bool,
    ) -> Result<FunctionAbi<'gcx>, ErrorGuaranteed> {
        let function = self.gcx.hir.function(func_id);
        let parameters = function.parameters;
        let returns = function.returns;
        let return_types =
            returns.iter().map(|&id| self.gcx.type_of_item(id.into())).collect::<Vec<_>>();

        let param_layout = if has_abi_inputs {
            let types = parameters
                .iter()
                .map(|&id| self.abi_param_type(self.gcx.type_of_item(id.into())))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| self.abi_type_error())?;
            Some(AbiParamLayout::new(types))
        } else {
            None
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

        let external_arg_head_size = if has_abi_inputs {
            param_layout
                .as_ref()
                .and_then(AbiParamLayout::checked_head_size)
                .ok_or_else(|| self.abi_head_size_overflow())?
        } else {
            0
        };
        let external_static_return_size = if uses_external_abi && !return_types.is_empty() {
            let layout = return_layout.as_ref().expect("return layout exists for ABI returns");
            if layout.types.iter().any(AbiType::is_dynamic) {
                0
            } else {
                layout.checked_head_size().ok_or_else(|| self.abi_head_size_overflow())?
            }
        } else {
            0
        };

        Ok(FunctionAbi {
            return_types,
            external_arg_head_size,
            external_static_return_size,
            return_layout,
            param_layout,
        })
    }

    fn abi_param_type(&self, ty: Ty<'gcx>) -> Option<AbiParamType> {
        let mut visiting = FxHashSet::default();
        self.abi_param_type_inner(ty, &mut visiting)
    }

    fn abi_param_type_inner(
        &self,
        ty: Ty<'gcx>,
        visiting: &mut FxHashSet<hir::StructId>,
    ) -> Option<AbiParamType> {
        let ty = ty.peel_refs();
        Some(match ty.kind {
            TyKind::Mapping(..) | TyKind::Ref(_, solar_ast::DataLocation::Storage) => {
                AbiParamType::Scalar(self.lower_type_from_ty(ty))
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                AbiParamType::Bytes
            }
            TyKind::Enum(id) => AbiParamType::Enum {
                ty: self.lower_type_from_ty(ty),
                variants: self.gcx.hir.enumm(id).variants.len() as u64,
            },
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                AbiParamType::DynamicArray(Box::new(self.abi_param_type_inner(element, visiting)?))
            }
            TyKind::Array(element, len) => AbiParamType::FixedArray {
                element: Box::new(self.abi_param_type_inner(element, visiting)?),
                len: u64::try_from(len).ok()?,
            },
            TyKind::Struct(id) => {
                if !visiting.insert(id) {
                    return None;
                }
                let fields = self
                    .gcx
                    .struct_field_types(id)
                    .iter()
                    .map(|&field| self.abi_param_type_inner(field, visiting))
                    .collect::<Option<Vec<_>>>()?;
                visiting.remove(&id);
                AbiParamType::Tuple(fields.into())
            }
            TyKind::Tuple(fields) => AbiParamType::Tuple(
                fields
                    .iter()
                    .map(|&field| self.abi_param_type_inner(field, visiting))
                    .collect::<Option<Vec<_>>>()?
                    .into(),
            ),
            TyKind::Udvt(inner, _) => return self.abi_param_type_inner(inner, visiting),
            _ => AbiParamType::Scalar(self.lower_type_from_ty(ty)),
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
                if len.checked_mul(element.checked_head_size()?).is_none() {
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
        self.abi_type(ty, false).is_none_or(|abi| abi.is_dynamic())
    }

    /// Static ABI head size, in bytes, of one top-level item.
    pub(super) fn abi_head_size(&self, ty: Ty<'gcx>) -> Result<u64, ErrorGuaranteed> {
        let abi = self.abi_type(ty, false).ok_or_else(|| self.abi_type_error())?;
        abi.checked_head_size().ok_or_else(|| self.abi_head_size_overflow())
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
