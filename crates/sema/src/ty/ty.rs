use super::{abi::TySolcPrinter, Gcx, Recursiveness};
use crate::{builtins::Builtin, hir};
use alloy_primitives::U256;
use solar_ast::{DataLocation, ElementaryType, StateMutability, TypeSize, Visibility};
use solar_data_structures::{fmt, Interned};
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
        &self.0 .0
    }
}

impl<'gcx> Ty<'gcx> {
    pub fn new(gcx: Gcx<'gcx>, kind: TyKind<'gcx>) -> Self {
        gcx.mk_ty(kind)
    }

    // TODO: with_loc_if_ref ?
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

    /// Peels `Ref` layers from the type, returning the inner type.
    pub fn peel_refs(self) -> Self {
        let mut ty = self;
        while let TyKind::Ref(inner, _) = ty.kind {
            ty = inner;
        }
        ty
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
                    if is_calldata(ret) {
                        ret.with_loc(gcx, DataLocation::Memory)
                    } else {
                        *ret
                    }
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
    pub fn is_ref_at(self, loc: DataLocation) -> bool {
        matches!(self.kind, TyKind::Ref(_, l) if l == loc)
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
            TyKind::Struct(_) | TyKind::Array(..) | TyKind::DynArray(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if the type is recursive.
    pub fn is_recursive(self) -> bool {
        self.flags.contains(TyFlags::IS_RECURSIVE)
    }

    /// Returns `true` if this type contains a mapping.
    pub fn has_mapping(self) -> bool {
        self.flags.contains(TyFlags::HAS_MAPPING)
    }

    /// Returns `true` if this type contains an error.
    pub fn has_error(self) -> Result<(), ErrorGuaranteed> {
        if self.flags.contains(TyFlags::HAS_ERROR) {
            Err(ErrorGuaranteed::new_unchecked())
        } else {
            Ok(())
        }
    }

    /// Returns `true` if this type can be part of an externally callable function.
    #[inline]
    pub fn can_be_exported(self) -> bool {
        !(self.is_recursive() || self.has_mapping() || self.has_error().is_err())
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
            | TyKind::IntLiteral(_)
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

    /// Displays the type with the default format.
    pub fn display(self, gcx: Gcx<'gcx>) -> impl fmt::Display + use<'gcx> {
        fmt::from_fn(move |f| TySolcPrinter::new(gcx, f).data_locations(true).print(self))
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
#[derive(Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TyKind<'gcx> {
    /// An elementary/primitive type.
    Elementary(ElementaryType),

    /// Any string literal. Contains `(is_valid_utf8(s), min(s.len(), 32))`.
    /// - all string literals can coerce to `bytes`
    /// - only valid UTF-8 string literals can coerce to `string`
    /// - only string literals with `len <= N` can coerce to `bytesN`
    StringLiteral(bool, TypeSize),

    /// Any integer or fixed-point number literal. Contains `min(s.len(), 32)`.
    IntLiteral(TypeSize),

    /// A reference to another type which lives in the data location.
    Ref(Ty<'gcx>, DataLocation),

    /// Dynamic array: `T[]`.
    DynArray(Ty<'gcx>),

    /// Fixed-size array: `T[N]`.
    Array(Ty<'gcx>, U256),

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
            | TyKind::IntLiteral(_)
            | TyKind::Contract(_)
            | TyKind::FnPtr(_)
            | TyKind::Enum(_)
            | TyKind::Module(_)
            | TyKind::BuiltinModule(_) => {}

            TyKind::Ref(ty, _)
            | TyKind::DynArray(ty)
            | TyKind::Array(ty, _)
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
