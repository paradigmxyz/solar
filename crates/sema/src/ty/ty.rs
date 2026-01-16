use super::{Gcx, Recursiveness, print::TySolcPrinter};
use crate::{builtins::Builtin, hir};
use alloy_primitives::U256;
use solar_ast::{DataLocation, ElementaryType, StateMutability, TypeSize, Visibility};
use solar_data_structures::{Interned, fmt};
use solar_interface::diagnostics::ErrorGuaranteed;
use std::{borrow::Borrow, hash::Hash, ops::ControlFlow};

/// An interned type.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ty<'gcx>(pub(super) Interned<'gcx, TyData<'gcx>>);

impl fmt::Debug for Ty<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'gcx> std::ops::Deref for Ty<'gcx> {
    type Target = &'gcx TyData<'gcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TyConvertError {
    /// Generic incompatibility (fallback).
    Incompatible,

    /// Contract doesn't inherit from target contract.
    NonDerivedContract,

    /// Invalid conversion between types.
    InvalidConversion,

    /// Literal is larger than the target type.
    LiteralTooLarge,

    /// Contract cannot be converted to address payable because it cannot receive ether.
    ContractNotPayable,

    /// Non-payable address cannot be converted to a contract that can receive ether.
    AddressNotPayable,
}

impl TyConvertError {
    /// Returns the error message for this conversion error.
    pub fn message<'gcx>(self, from: Ty<'gcx>, to: Ty<'gcx>, gcx: Gcx<'gcx>) -> String {
        match self {
            Self::NonDerivedContract => {
                format!(
                    "contract `{}` does not inherit from `{}`",
                    from.display(gcx),
                    to.display(gcx)
                )
            }
            Self::InvalidConversion => {
                format!("cannot convert `{}` to `{}`", from.display(gcx), to.display(gcx))
            }
            Self::Incompatible => {
                format!("expected `{}`, found `{}`", to.display(gcx), from.display(gcx))
            }
            Self::LiteralTooLarge => {
                format!(
                    "literal `{}` is larger than the type `{}`",
                    from.display(gcx),
                    to.display(gcx)
                )
            }
            Self::ContractNotPayable => {
                format!(
                    "cannot convert `{}` to `address payable` because it has no receive function or payable fallback",
                    from.display(gcx)
                )
            }
            Self::AddressNotPayable => {
                format!(
                    "cannot convert non-payable `address` to `{}` because it has a receive function or payable fallback",
                    to.display(gcx)
                )
            }
        }
    }
}

impl<'gcx> Ty<'gcx> {
    pub fn new(gcx: Gcx<'gcx>, kind: TyKind<'gcx>) -> Self {
        gcx.mk_ty(kind)
    }

    /// Displays the type for human-readable diagnostics.
    pub fn display(self, gcx: Gcx<'gcx>) -> impl fmt::Display + use<'gcx> {
        fmt::from_fn(move |f| TySolcPrinter::new(gcx, f).data_locations(true).print(self))
    }

    #[doc(alias = "with_location")]
    pub fn with_loc(self, gcx: Gcx<'gcx>, loc: DataLocation) -> Self {
        let mut ty = self;
        if let TyKind::Ref(inner, l2) = self.kind {
            if l2 == loc {
                return self;
            }
            ty = inner;
        }
        Self::new(gcx, TyKind::Ref(ty, loc))
    }

    #[doc(alias = "with_location_if_reference")]
    pub fn with_loc_if_ref(self, gcx: Gcx<'gcx>, loc: DataLocation) -> Self {
        if self.is_reference_type() {
            return self.with_loc(gcx, loc);
        }
        self
    }

    pub fn with_loc_if_ref_opt(self, gcx: Gcx<'gcx>, loc: Option<DataLocation>) -> Self {
        if let Some(loc) = loc {
            return self.with_loc_if_ref(gcx, loc);
        }
        self
    }

    /// Returns the location of the type if it is a reference.
    #[doc(alias = "location")]
    pub fn loc(self) -> Option<DataLocation> {
        match self.kind {
            TyKind::Ref(_, loc) => Some(loc),
            _ => None,
        }
    }

    /// Peels `Ref` layers from the type, returning the inner type.
    pub fn peel_refs(mut self) -> Self {
        // There shouldn't be any double references so we can avoid using a loop here.
        if let TyKind::Ref(inner, _) = self.kind {
            self = inner;
        }
        debug_assert!(!self.is_ref(), "double reference type found");
        self
    }

    pub fn as_externally_callable_function(self, gcx: Gcx<'gcx>) -> Self {
        let is_calldata = |param: &Ty<'_>| param.is_ref_at(DataLocation::Calldata);
        let parameters = self.parameters().unwrap_or_default();
        let returns = self.returns().unwrap_or_default();
        let any_parameter = parameters.iter().any(is_calldata);
        let any_return = returns.iter().any(is_calldata);
        if !any_parameter && !any_return {
            return self;
        }
        gcx.mk_ty_fn_ptr(TyFnPtr {
            parameters: if any_parameter {
                gcx.mk_ty_iter(parameters.iter().map(|param| {
                    if is_calldata(param) {
                        param.with_loc(gcx, DataLocation::Memory)
                    } else {
                        *param
                    }
                }))
            } else {
                parameters
            },
            returns: if any_return {
                gcx.mk_ty_iter(returns.iter().map(|ret| {
                    if is_calldata(ret) { ret.with_loc(gcx, DataLocation::Memory) } else { *ret }
                }))
            } else {
                returns
            },
            state_mutability: self.state_mutability().unwrap_or(StateMutability::NonPayable),
            visibility: self.visibility().unwrap_or(Visibility::Public),
        })
    }

    pub fn make_ref(self, gcx: Gcx<'gcx>, loc: DataLocation) -> Self {
        if self.is_ref_at(loc) {
            return self;
        }
        Self::new(gcx, TyKind::Ref(self, loc))
    }

    pub fn make_type_type(self, gcx: Gcx<'gcx>) -> Self {
        if let TyKind::Type(_) = self.kind {
            return self;
        }
        Self::new(gcx, TyKind::Type(self))
    }

    pub fn make_meta(self, gcx: Gcx<'gcx>) -> Self {
        if let TyKind::Meta(_) = self.kind {
            return self;
        }
        Self::new(gcx, TyKind::Meta(self))
    }

    /// Returns `true` if the type is a reference.
    #[inline]
    pub fn is_ref(self) -> bool {
        matches!(self.kind, TyKind::Ref(..))
    }

    /// Returns `true` if the type is a reference to the given location.
    #[inline]
    #[doc(alias = "is_reference_with_location")]
    pub fn is_ref_at(self, loc: DataLocation) -> bool {
        matches!(self.kind, TyKind::Ref(_, l) if l == loc)
    }

    /// Returns `true` if the type is a reference to the given location.
    pub fn data_stored_in(self, loc: DataLocation) -> bool {
        match self.kind {
            TyKind::Ref(_, l) => l == loc,
            TyKind::Mapping(..) => loc == DataLocation::Storage,
            _ => false,
        }
    }

    /// Returns `true` if the type is a value type.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/types.html#value-types>
    #[inline]
    pub fn is_value_type(self) -> bool {
        match self.kind {
            TyKind::Elementary(t) => t.is_value_type(),
            TyKind::Contract(_) | TyKind::FnPtr(_) | TyKind::Enum(_) | TyKind::Udvt(..) => true,
            _ => false,
        }
    }

    /// Returns `true` if the type is a reference type.
    #[inline]
    pub fn is_reference_type(self) -> bool {
        match self.kind {
            TyKind::Elementary(t) => t.is_reference_type(),
            TyKind::Struct(_) | TyKind::Array(..) | TyKind::DynArray(_) | TyKind::Slice(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if the type is recursive.
    #[inline]
    pub fn is_recursive(self) -> bool {
        self.flags.contains(TyFlags::IS_RECURSIVE)
    }

    /// Returns `true` if this type contains a mapping.
    #[inline]
    pub fn has_mapping(self) -> bool {
        self.flags.contains(TyFlags::HAS_MAPPING)
    }

    /// Returns `true` if this type contains a non-public (internal/private) function pointer.
    #[inline]
    pub fn has_internal_function(self) -> bool {
        self.flags.contains(TyFlags::HAS_INTERNAL_FN)
    }

    /// Returns `Err(guar)` if this type contains an error.
    #[inline]
    pub fn error_reported(self) -> Result<(), ErrorGuaranteed> {
        if self.references_error() { Err(ErrorGuaranteed::new_unchecked()) } else { Ok(()) }
    }

    /// Returns `true` if this type contains an error.
    #[inline]
    pub fn references_error(self) -> bool {
        self.flags.contains(TyFlags::HAS_ERROR)
    }

    /// Returns `true` if this type can be part of an externally callable function.
    #[inline]
    pub fn can_be_exported(self) -> bool {
        !(self.is_recursive()
            || self.has_mapping()
            || self.has_internal_function()
            || self.references_error())
    }

    /// Returns the parameter types of the type.
    #[inline]
    pub fn parameters(self) -> Option<&'gcx [Self]> {
        Some(match self.kind {
            TyKind::FnPtr(f) => f.parameters,
            TyKind::Event(tys, _) | TyKind::Error(tys, _) => tys,
            _ => return None,
        })
    }

    /// Returns the return types of the type.
    #[inline]
    pub fn returns(self) -> Option<&'gcx [Self]> {
        Some(match self.kind {
            TyKind::FnPtr(f) => f.returns,
            _ => return None,
        })
    }

    /// Returns the state mutability of the type.
    #[inline]
    pub fn state_mutability(self) -> Option<StateMutability> {
        match self.kind {
            TyKind::FnPtr(f) => Some(f.state_mutability),
            _ => None,
        }
    }

    /// Returns the visibility of the type.
    #[inline]
    pub fn visibility(self) -> Option<Visibility> {
        match self.kind {
            TyKind::FnPtr(f) => Some(f.visibility),
            _ => None,
        }
    }

    /// Visits the type and its subtypes.
    pub fn visit<T>(self, f: &mut impl FnMut(Self) -> ControlFlow<T>) -> ControlFlow<T> {
        f(self)?;
        match self.kind {
            TyKind::Elementary(_)
            | TyKind::StringLiteral(..)
            | TyKind::IntLiteral(..)
            | TyKind::Contract(_)
            | TyKind::FnPtr(_)
            | TyKind::Enum(_)
            | TyKind::Module(_)
            | TyKind::BuiltinModule(_)
            | TyKind::Struct(_)
            | TyKind::Err(_) => ControlFlow::Continue(()),

            TyKind::Ref(ty, _)
            | TyKind::DynArray(ty)
            | TyKind::Array(ty, _)
            | TyKind::Slice(ty)
            | TyKind::Udvt(ty, _)
            | TyKind::Type(ty)
            | TyKind::Meta(ty) => ty.visit(f),

            TyKind::Error(list, _) | TyKind::Event(list, _) | TyKind::Tuple(list) => {
                for ty in list {
                    ty.visit(f)?;
                }
                ControlFlow::Continue(())
            }

            TyKind::Mapping(k, v) => {
                k.visit(f)?;
                v.visit(f)
            }
        }
    }

    /// Returns `true` if the type is an array.
    #[inline]
    pub fn is_array(self) -> bool {
        matches!(self.kind, TyKind::Array(..) | TyKind::DynArray(..))
    }

    /// Returns `true` if the type is an array-like type.
    ///
    /// This is either an array or bytes/string.
    #[inline]
    pub fn is_array_like(&self) -> bool {
        self.is_array()
            || matches!(
                self.kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            )
    }

    /// Returns `true` if the type is sliceable.
    ///
    /// This is either an array, bytes, string, or slice.
    #[inline]
    pub fn is_sliceable(self) -> bool {
        let inner = self.peel_refs();
        inner.is_array_like() || matches!(inner.kind, TyKind::Slice(..))
    }

    /// Returns `true` if the type is dynamically sized.
    pub fn is_dynamically_sized(self) -> bool {
        matches!(
            self.kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                | TyKind::DynArray(..)
                | TyKind::Slice(..)
        )
    }

    pub fn is_dynamically_encoded(self, gcx: Gcx<'gcx>) -> bool {
        match self.kind {
            TyKind::Struct(id) => {
                self.is_recursive()
                    || gcx.struct_field_types(id).iter().any(|ty| ty.is_dynamically_encoded(gcx))
            }
            TyKind::Array(element, _) => element.is_dynamically_encoded(gcx),
            _ => self.is_dynamically_sized(),
        }
    }

    /// Returns `true` if the type is a fixed-size byte array.
    pub fn is_fixed_bytes(self) -> bool {
        matches!(self.kind, TyKind::Elementary(ElementaryType::FixedBytes(_)))
    }

    /// Returns `true` if the type is an integer, including literals.
    pub fn is_integer(self) -> bool {
        matches!(
            self.kind,
            TyKind::Elementary(hir::ElementaryType::Int(_) | hir::ElementaryType::UInt(_))
                | TyKind::IntLiteral(..)
        )
    }

    /// Returns `true` if the type is a signed integer, including negative literals.
    pub fn is_signed(self) -> bool {
        matches!(
            self.kind,
            TyKind::Elementary(ElementaryType::Int(_)) | TyKind::IntLiteral(true, _)
        )
    }

    /// Returns `true` if the type is a tuple.
    pub fn is_tuple(self) -> bool {
        matches!(self.kind, TyKind::Tuple(..))
    }

    /// Returns `true` if the type is the unit type `()`.
    pub fn is_unit(self) -> bool {
        matches!(self.kind, TyKind::Tuple([]))
    }

    /// Returns `true` if the type can be used for variables.
    pub fn nameable(self) -> bool {
        matches!(
            self.kind,
            TyKind::Elementary(_)
                | TyKind::Array(..)
                | TyKind::DynArray(_)
                | TyKind::Contract(_)
                | TyKind::Struct(_)
                | TyKind::Enum(_)
                | TyKind::Udvt(..)
                | TyKind::Mapping(..)
        )
    }

    /// Returns the common type between the two types.
    pub fn common_type(self, b: Self, gcx: Gcx<'gcx>) -> Option<Self> {
        let a = self;
        if let Some(a) = a.mobile(gcx)
            && b.convert_implicit_to(a, gcx)
        {
            return Some(a);
        }
        if let Some(b) = b.mobile(gcx)
            && a.convert_implicit_to(b, gcx)
        {
            return Some(b);
        }
        None
    }

    /// Returns the base type, if any.
    pub fn base_type(self, gcx: Gcx<'gcx>) -> Option<Self> {
        let loc = self.loc();
        match self.peel_refs().kind {
            TyKind::Array(base, _) | TyKind::DynArray(base) => {
                Some(base.with_loc_if_ref_opt(gcx, loc))
            }
            TyKind::Slice(arr) => arr.with_loc_if_ref_opt(gcx, loc).base_type(gcx),
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(gcx.types.fixed_bytes(1))
            }
            _ => None,
        }
    }

    /// Returns `true` if the type is implicitly convertible to the given type.
    ///
    /// Prefer using [`Ty::try_convert_implicit_to`] if you need to handle the error case.
    #[inline]
    #[doc(alias = "is_implicitly_convertible_to")]
    #[must_use]
    pub fn convert_implicit_to(self, other: Self, gcx: Gcx<'gcx>) -> bool {
        self.try_convert_implicit_to(other, gcx).is_ok()
    }

    /// Checks if the type is implicitly convertible to the given type.
    ///
    /// See: <https://docs.soliditylang.org/en/latest/types.html#implicit-conversions>
    pub fn try_convert_implicit_to(
        self,
        other: Self,
        gcx: Gcx<'gcx>,
    ) -> Result<(), TyConvertError> {
        use ElementaryType::*;
        use TyKind::*;

        if self == other || self.references_error() || other.references_error() {
            return Ok(());
        }

        match (self.kind, other.kind) {
            // address payable -> address.
            (Elementary(Address(true)), Elementary(Address(false))) => Ok(()),

            // Reference type location coercion rules.
            // See: <https://docs.soliditylang.org/en/latest/types.html#data-location-and-assignment-behaviour>
            (Ref(from_inner, from_loc), Ref(to_inner, to_loc)) => {
                match (from_loc, to_loc) {
                    // Same location: allowed if base types match.
                    (DataLocation::Memory, DataLocation::Memory)
                    | (DataLocation::Calldata, DataLocation::Calldata) => {
                        from_inner.try_convert_implicit_to(to_inner, gcx)
                    }

                    // storage -> storage: allowed (reference assignment).
                    (DataLocation::Storage, DataLocation::Storage) => {
                        from_inner.try_convert_implicit_to(to_inner, gcx)
                    }

                    // calldata/storage -> memory: allowed (copy semantics).
                    (DataLocation::Calldata, DataLocation::Memory)
                    | (DataLocation::Storage, DataLocation::Memory) => {
                        from_inner.try_convert_implicit_to(to_inner, gcx)
                    }

                    // memory/calldata -> storage: allowed (copy semantics).
                    (DataLocation::Memory, DataLocation::Storage)
                    | (DataLocation::Calldata, DataLocation::Storage) => {
                        from_inner.try_convert_implicit_to(to_inner, gcx)
                    }

                    // storage -> calldata: never allowed.
                    // memory -> calldata: never allowed.
                    _ => Result::Err(TyConvertError::Incompatible),
                }
            }

            // Array slices are implicitly convertible to arrays of their underlying element type.
            // Note: conversion to storage pointers is NOT allowed.
            // See: <https://docs.soliditylang.org/en/latest/types.html#array-slices>
            // `T[] loc slice` -> `T[] loc`
            (Slice(underlying), _) if underlying == other => Ok(()),
            // `T[] loc slice` -> `T[] memory`
            (Slice(underlying), Ref(other_inner, DataLocation::Memory)) => {
                if let Ref(self_inner, _) = underlying.kind
                    && self_inner == other_inner
                {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::Incompatible)
                }
            }

            // Contract -> Base contract (inheritance check)
            (Contract(self_contract_id), Contract(other_contract_id)) => {
                let self_contract = gcx.hir.contract(self_contract_id);
                if self_contract.linearized_bases.contains(&other_contract_id) {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::NonDerivedContract)
                }
            }
            // byte literal -> bytesN/bytes
            // See: <https://docs.soliditylang.org/en/latest/types.html#index-34>
            (StringLiteral(_, _), Elementary(Bytes)) => Ok(()),
            (StringLiteral(_, size_from), Elementary(FixedBytes(size_to))) => {
                if size_from.bytes() <= size_to.bytes() {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::LiteralTooLarge)
                }
            }

            // Integer literals can coerce to typed integers if they fit.
            // Non-negative literals can coerce to both uint and int types.
            (IntLiteral(neg, size), Elementary(UInt(target_size))) => {
                // Unsigned: reject negative, check size fits
                if neg {
                    Result::Err(TyConvertError::Incompatible)
                } else if size.bits() <= target_size.bits() {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::Incompatible)
                }
            }
            (IntLiteral(neg, size), Elementary(Int(target_size))) => {
                // Signed: non-negative values need strict inequality since they use the
                // positive range [0, 2^(N-1)-1]. Negative values use <= since negative
                // int_literal[N] can fit in int(N) (e.g., -128 needs 8 bits, fits in int8).
                if neg {
                    if size.bits() <= target_size.bits() {
                        Ok(())
                    } else {
                        Result::Err(TyConvertError::Incompatible)
                    }
                } else if size.bits() < target_size.bits() {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::Incompatible)
                }
            }

            // Integer width conversions: smaller int -> larger int (same signedness).
            // See: <https://docs.soliditylang.org/en/latest/types.html#implicit-conversions>
            (Elementary(UInt(from_size)), Elementary(UInt(to_size))) => {
                if from_size.bits() <= to_size.bits() {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::Incompatible)
                }
            }
            (Elementary(Int(from_size)), Elementary(Int(to_size))) => {
                if from_size.bits() <= to_size.bits() {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::Incompatible)
                }
            }

            // FixedBytes size conversions: smaller bytesN -> larger bytesN (right-padded with
            // zeros). See: <https://docs.soliditylang.org/en/latest/types.html#implicit-conversions>
            (Elementary(FixedBytes(from_size)), Elementary(FixedBytes(to_size))) => {
                if from_size.bytes() <= to_size.bytes() {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::Incompatible)
                }
            }

            // Function pointer conversions.
            // See: <https://docs.soliditylang.org/en/latest/types.html#function-types>
            (FnPtr(from_fn), FnPtr(to_fn)) => {
                // Parameter and return types must match exactly (no implicit conversion).
                if from_fn.parameters != to_fn.parameters || from_fn.returns != to_fn.returns {
                    return Result::Err(TyConvertError::Incompatible);
                }

                // Visibility must match.
                if from_fn.visibility != to_fn.visibility {
                    return Result::Err(TyConvertError::Incompatible);
                }

                // State mutability compatibility:
                // - pure can convert to view or non-payable (more restrictive -> less restrictive)
                // - view can convert to non-payable
                // - payable can convert to non-payable (you can pay 0)
                // - non-payable cannot convert to payable
                use StateMutability::*;
                let mutability_ok = match (from_fn.state_mutability, to_fn.state_mutability) {
                    (a, b) if a == b => true,
                    // pure is the most restrictive, can convert to anything except payable.
                    (Pure, View) | (Pure, NonPayable) => true,
                    // view can convert to non-payable.
                    (View, NonPayable) => true,
                    // payable can convert to non-payable.
                    (Payable, NonPayable) => true,
                    _ => false,
                };
                if mutability_ok { Ok(()) } else { Result::Err(TyConvertError::Incompatible) }
            }

            _ => Result::Err(TyConvertError::Incompatible),
        }
    }

    /// Returns `true` if the type is explicitly convertible to the given type.
    ///
    /// Prefer using [`Ty::try_convert_explicit_to`] if you need to handle the error case.
    #[inline]
    #[doc(alias = "is_explicitly_convertible_to")]
    #[must_use]
    pub fn convert_explicit_to(self, other: Self, gcx: Gcx<'gcx>) -> bool {
        self.try_convert_explicit_to(other, gcx).is_ok()
    }

    /// Checks if the type is explicitly convertible to the given type.
    ///
    /// See: <https://docs.soliditylang.org/en/latest/types.html#explicit-conversions>
    fn can_convert_explicit_to(self, other: Self, gcx: Gcx<'gcx>) -> Result<(), TyConvertError> {
        use ElementaryType::*;
        use TyKind::*;

        macro_rules! unreachable {
            () => {
                gcx.dcx()
                    .bug(format!("unreachable explicit conversion from `{self:?}` to `{other:?}`"))
                    .emit()
            };
        }

        if self.try_convert_implicit_to(other, gcx).is_ok() {
            return Ok(());
        }
        match (self.kind, other.kind) {
            // Enum <-> all integer types.
            (Enum(_), _) if other.is_integer() => Ok(()),
            (_, Enum(_)) if self.is_integer() => Ok(()),

            // bytes/FixedBytes to FixedBytes: always allowed (any size).
            // Smaller to larger right-pads with zeros, larger to smaller truncates on the right.
            (Elementary(FixedBytes(_)), Elementary(FixedBytes(_))) => Ok(()),
            (Ref(ty, _), Elementary(FixedBytes(_))) => {
                if matches!(ty.kind, Elementary(Bytes)) {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::InvalidConversion)
                }
            }

            // FixedBytes <-> UInt: same size only (signed integers not allowed).
            (Elementary(FixedBytes(size_from)), Elementary(UInt(size_to)))
            | (Elementary(UInt(size_from)), Elementary(FixedBytes(size_to)))
                if size_from == size_to =>
            {
                Ok(())
            }

            // address <-> bytes20.
            (Elementary(Address(false)), Elementary(FixedBytes(s))) if s.bytes() == 20 => Ok(()),
            (Elementary(FixedBytes(s)), Elementary(Address(false))) if s.bytes() == 20 => Ok(()),

            // address <-> uint160.
            (Elementary(Address(false)), Elementary(UInt(s))) if s.bits() == 160 => Ok(()),
            (Elementary(UInt(s)), Elementary(Address(false))) if s.bits() == 160 => Ok(()),

            // address -> address payable.
            (Elementary(Address(false)), Elementary(Address(true))) => Ok(()),
            // IntLiteral -> IntLiteral: explicit conversion to a literal type shouldn't be
            // possible.
            (IntLiteral(_, _), IntLiteral(_, _)) => unreachable!(),

            // Int <-> Int: any size allowed (only width changes, sign stays same).
            (Elementary(Int(_)), Elementary(Int(_))) => Ok(()),

            // UInt <-> UInt: any size allowed (only width changes, sign stays same).
            (Elementary(UInt(_)), Elementary(UInt(_))) => Ok(()),

            // Int <-> UInt: same size only (prevents multi-aspect conversion).
            // This enforces the Solidity 0.8.0+ restriction: cannot change both sign and width.
            (Elementary(Int(size_from)), Elementary(UInt(size_to)))
            | (Elementary(UInt(size_from)), Elementary(Int(size_to)))
                if size_from == size_to =>
            {
                Ok(())
            }

            // Contract -> Base contract (inheritance check)
            (Contract(self_contract_id), Contract(other_contract_id)) => {
                let self_contract = gcx.hir.contract(self_contract_id);
                if self_contract.linearized_bases.contains(&other_contract_id) {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::NonDerivedContract)
                }
            }

            // Contract -> address (always allowed)
            (Contract(_), Elementary(Address(false))) => Ok(()),

            // Contract -> address payable (only if contract can receive ether)
            (Contract(contract_id), Elementary(Address(true))) => {
                let contract = gcx.hir.contract(contract_id);

                if hir::can_receive_ether(contract, gcx) {
                    Ok(())
                } else {
                    Result::Err(TyConvertError::ContractNotPayable)
                }
            }

            // Address payable -> Contract (always allowed)
            (Elementary(Address(true)), Contract(_)) => Ok(()),

            // Address (non-payable) -> Contract (only if contract cannot receive ether)
            (Elementary(Address(false)), Contract(contract_id)) => {
                let contract = gcx.hir.contract(contract_id);

                if hir::can_receive_ether(contract, gcx) {
                    Result::Err(TyConvertError::AddressNotPayable)
                } else {
                    Ok(())
                }
            }

            // bytes <-> string (explicit conversion).
            // See: https://docs.soliditylang.org/en/latest/types.html#explicit-conversions
            // When target is Ref with location, locations must match.
            (Ref(from_inner, from_loc), Ref(to_inner, to_loc)) if from_loc == to_loc => {
                match (from_inner.kind, to_inner.kind) {
                    (Elementary(Bytes), Elementary(String)) => Ok(()),
                    (Elementary(String), Elementary(Bytes)) => Ok(()),
                    _ => Result::Err(TyConvertError::InvalidConversion),
                }
            }
            // When target is unlocated (e.g., `bytes(s)` where s is `string memory`),
            // the location is inherited from the source.
            (Ref(from_inner, _), Elementary(Bytes))
                if matches!(from_inner.kind, Elementary(String)) =>
            {
                Ok(())
            }
            (Ref(from_inner, _), Elementary(String))
                if matches!(from_inner.kind, Elementary(Bytes)) =>
            {
                Ok(())
            }

            _ => Result::Err(TyConvertError::InvalidConversion),
        }
    }

    /// Performs an explicit type conversion, returning the result type.
    ///
    /// For most conversions this is `other`, but for bytes <-> string with unlocated target,
    /// the result type inherits the source's data location.
    ///
    /// See: <https://docs.soliditylang.org/en/latest/types.html#explicit-conversions>
    pub fn try_convert_explicit_to(
        self,
        other: Self,
        gcx: Gcx<'gcx>,
    ) -> Result<Self, TyConvertError> {
        self.can_convert_explicit_to(other, gcx)?;

        // Handle special case: bytes <-> string with unlocated target inherits source location.
        use ElementaryType::*;
        use TyKind::*;
        Ok(match (self.kind, other.kind) {
            (Ref(from_inner, loc), Elementary(Bytes))
                if matches!(from_inner.kind, Elementary(String)) =>
            {
                gcx.types.bytes.with_loc(gcx, loc)
            }
            (Ref(from_inner, loc), Elementary(String))
                if matches!(from_inner.kind, Elementary(Bytes)) =>
            {
                gcx.types.string.with_loc(gcx, loc)
            }
            _ => other,
        })
    }

    /// Returns the mobile (in contrast to static) type corresponding to the given type.
    #[doc(alias = "mobile_type")]
    pub fn mobile(self, gcx: Gcx<'gcx>) -> Option<Self> {
        Some(match self.kind {
            TyKind::IntLiteral(false, size) => gcx.types.uint_(size),
            TyKind::IntLiteral(true, size) => gcx.types.int_(size),
            TyKind::StringLiteral(..) => gcx.types.string_ref.memory,
            // TODO: basetype.is_dynamically_encoded
            TyKind::Slice(ty)
                if ty.data_stored_in(DataLocation::Calldata) && ty.is_dynamically_sized() =>
            {
                ty
            }
            TyKind::Tuple(tys) => {
                let tys = tys.iter().map(|ty| ty.mobile(gcx)).collect::<Option<Vec<_>>>()?;
                gcx.mk_ty_tuple(gcx.mk_tys(&tys))
            }
            // TODO: functions
            _ => self,
        })
    }
}

/// The interned data of a type.
pub struct TyData<'gcx> {
    pub kind: TyKind<'gcx>,
    pub flags: TyFlags,
}

impl<'gcx> Borrow<TyKind<'gcx>> for &TyData<'gcx> {
    #[inline]
    fn borrow(&self) -> &TyKind<'gcx> {
        &self.kind
    }
}

impl PartialEq for TyData<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for TyData<'_> {}

impl std::hash::Hash for TyData<'_> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

impl fmt::Debug for TyData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

/// The kind of a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TyKind<'gcx> {
    /// An elementary/primitive type.
    Elementary(ElementaryType),

    /// Any string literal. Contains `(is_valid_utf8(s), min(s.len(), 32))`.
    /// - all string literals can coerce to `bytes`
    /// - only valid UTF-8 string literals can coerce to `string`
    /// - only string literals with `len <= N` can coerce to `bytesN`
    StringLiteral(bool, TypeSize),

    /// Any integer or fixed-point number literal. Contains `(negative, min(s.len(), 32))`.
    IntLiteral(bool, TypeSize),

    /// A reference to another type which lives in the data location.
    Ref(Ty<'gcx>, DataLocation),

    /// Dynamic array: `T[]`.
    DynArray(Ty<'gcx>),

    /// Fixed-size array: `T[N]`.
    Array(Ty<'gcx>, U256),

    /// Array slice: result of `expr[1:2]`.
    ///
    /// Holds the underlying array type it is slicing (which can also be string/bytes).
    Slice(Ty<'gcx>),

    /// Tuple: `(T1, T2, ...)`.
    Tuple(&'gcx [Ty<'gcx>]),

    /// Mapping: `mapping(K => V)`.
    Mapping(Ty<'gcx>, Ty<'gcx>),

    /// Function pointer: `function(...) returns (...)`.
    FnPtr(&'gcx TyFnPtr<'gcx>),

    /// Contract.
    Contract(hir::ContractId),

    /// A struct.
    ///
    /// Cannot contain the types of its fields because it can be recursive.
    Struct(hir::StructId),

    /// An enum.
    Enum(hir::EnumId),

    /// A custom error.
    Error(&'gcx [Ty<'gcx>], hir::ErrorId),

    /// An event.
    Event(&'gcx [Ty<'gcx>], hir::EventId),

    /// A user-defined value type. `Ty` can only be `Elementary`.
    Udvt(Ty<'gcx>, hir::UdvtId),

    /// A source imported as a module: `import "path" as Module;`.
    Module(hir::SourceId),

    /// Builtin module.
    BuiltinModule(Builtin),

    /// The self-referential type, e.g. `Enum` in `Enum.Variant`.
    /// Corresponds to `TypeType` in solc.
    Type(Ty<'gcx>),

    /// The meta type: `type(<inner_type>)`.
    Meta(Ty<'gcx>),

    /// An invalid type. Silences further errors.
    Err(ErrorGuaranteed),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TyFnPtr<'gcx> {
    pub parameters: &'gcx [Ty<'gcx>],
    pub returns: &'gcx [Ty<'gcx>],
    pub state_mutability: StateMutability,
    pub visibility: Visibility,
}

impl<'gcx> TyFnPtr<'gcx> {
    /// Returns an iterator over all the types in the function pointer.
    pub fn tys(&self) -> impl DoubleEndedIterator<Item = Ty<'gcx>> + Clone {
        self.parameters.iter().copied().chain(self.returns.iter().copied())
    }
}

bitflags::bitflags! {
    /// [`Ty`] flags.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct TyFlags: u8 {
        /// Whether this type is recursive.
        const IS_RECURSIVE = 1 << 0;
        /// Whether this type contains a mapping.
        const HAS_MAPPING  = 1 << 1;
        /// Whether an error is reachable.
        const HAS_ERROR    = 1 << 2;
        /// Whether this type contains a non-public (internal/private) function pointer.
        const HAS_INTERNAL_FN = 1 << 3;
    }
}

impl TyFlags {
    pub(super) fn calculate<'gcx>(gcx: Gcx<'gcx>, ty: &TyKind<'gcx>) -> Self {
        let mut flags = Self::empty();
        flags.add_ty_kind(gcx, ty);
        flags
    }

    fn add_ty_kind<'gcx>(&mut self, gcx: Gcx<'gcx>, ty: &TyKind<'gcx>) {
        match *ty {
            TyKind::Elementary(_)
            | TyKind::StringLiteral(..)
            | TyKind::IntLiteral(..)
            | TyKind::Contract(_)
            | TyKind::Enum(_)
            | TyKind::Module(_)
            | TyKind::BuiltinModule(_) => {}

            TyKind::FnPtr(f) => {
                if f.visibility < hir::Visibility::Public {
                    self.add(Self::HAS_INTERNAL_FN);
                }
            }

            TyKind::Ref(ty, _)
            | TyKind::DynArray(ty)
            | TyKind::Array(ty, _)
            | TyKind::Slice(ty)
            | TyKind::Udvt(ty, _)
            | TyKind::Type(ty)
            | TyKind::Meta(ty) => self.add_ty(ty),

            TyKind::Error(list, _) | TyKind::Event(list, _) | TyKind::Tuple(list) => {
                self.add_tys(list)
            }

            TyKind::Mapping(k, v) => {
                self.add_ty(k);
                self.add_ty(v);
                self.add(Self::HAS_MAPPING);
            }

            TyKind::Struct(id) => match gcx.struct_recursiveness(id) {
                Recursiveness::None => self.add_tys(gcx.struct_field_types(id)),
                Recursiveness::Recursive => {
                    self.add(Self::IS_RECURSIVE);
                }
                Recursiveness::Infinite(_guar) => {
                    self.add(Self::HAS_ERROR);
                }
            },

            TyKind::Err(_) => self.add(Self::HAS_ERROR),
        }
    }

    #[inline]
    fn add(&mut self, other: Self) {
        *self |= other;
    }

    #[inline]
    fn add_ty(&mut self, ty: Ty<'_>) {
        self.add(ty.flags);
    }

    #[inline]
    fn add_tys(&mut self, tys: &[Ty<'_>]) {
        for &ty in tys {
            self.add_ty(ty);
        }
    }
}
