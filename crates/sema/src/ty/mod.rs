use crate::{
    ast_lowering::SymbolResolver,
    builtins::{
        members::{self, MemberMap},
        Builtin,
    },
    hir::{self, Hir},
};
use alloy_primitives::{keccak256, Selector, B256};
use solar_ast::ast::{DataLocation, StateMutability, TypeSize, Visibility};
use solar_data_structures::{
    map::{FxBuildHasher, FxHashMap, FxHashSet},
    BumpExt,
};
use solar_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    Ident, Session, Span,
};
use std::{
    fmt,
    hash::{BuildHasher, Hash},
};
use thread_local::ThreadLocal;

mod abi;

mod common;
pub use common::{CommonTypes, EachDataLoc};

mod interner;
use interner::Interner;

#[allow(clippy::module_inception)]
mod ty;
pub use ty::{Ty, TyData, TyFlags, TyFnPtr, TyKind};

type FxOnceMap<K, V> = once_map::OnceMap<K, V, FxBuildHasher>;

/// A function exported by a contract.
#[derive(Clone, Copy, Debug)]
pub struct InterfaceFunction<'gcx> {
    /// The function 4-byte selector.
    pub selector: Selector,
    /// The function ID.
    pub id: hir::FunctionId,
    /// The function type. This is always a function pointer.
    pub ty: Ty<'gcx>,
}

/// List of all the functions exported by a contract.
///
/// Return type of [`Gcx::interface_functions`].
#[derive(Clone, Copy, Debug)]
pub struct InterfaceFunctions<'gcx> {
    /// The exported functions along with their selector.
    pub functions: &'gcx [InterfaceFunction<'gcx>],
    /// The index in `functions` where the inherited functions start.
    pub inheritance_start: usize,
}

impl<'gcx> InterfaceFunctions<'gcx> {
    pub fn all_functions(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        self.functions
    }

    pub fn own_functions(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        &self.functions[..self.inheritance_start]
    }

    pub fn inherited_functions(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        &self.functions[self.inheritance_start..]
    }
}

/// Reference to the [global context](GlobalCtxt).
#[derive(Clone, Copy)]
#[cfg_attr(feature = "nightly", rustc_pass_by_value)]
pub struct Gcx<'gcx>(&'gcx GlobalCtxt<'gcx>);

impl<'gcx> std::ops::Deref for Gcx<'gcx> {
    type Target = &'gcx GlobalCtxt<'gcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
fn _gcx_traits() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Gcx<'static>>();
}

/// The global compilation context.
pub struct GlobalCtxt<'gcx> {
    pub sess: &'gcx Session,
    pub types: CommonTypes<'gcx>,
    pub hir: Hir<'gcx>,
    pub(crate) symbol_resolver: SymbolResolver<'gcx>,

    interner: Interner<'gcx>,
    cache: Cache<'gcx>,
}

impl<'gcx> GlobalCtxt<'gcx> {
    pub(crate) fn new(
        sess: &'gcx Session,
        arena: &'gcx ThreadLocal<hir::Arena>,
        hir: Hir<'gcx>,
        symbol_resolver: SymbolResolver<'gcx>,
    ) -> Self {
        let interner = Interner::new(arena);
        Self {
            sess,
            types: CommonTypes::new(&interner),
            hir,
            symbol_resolver,
            interner,
            cache: Cache::default(),
        }
    }
}

impl<'gcx> Gcx<'gcx> {
    pub(crate) fn new(gcx: &'gcx GlobalCtxt<'gcx>) -> Self {
        Self(gcx)
    }

    /// Returns the diagnostics context.
    pub fn dcx(self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    pub fn arena(self) -> &'gcx hir::Arena {
        self.interner.arena.get_or_default()
    }

    pub fn bump(self) -> &'gcx bumpalo::Bump {
        &self.arena().bump
    }

    pub fn alloc<T>(self, value: T) -> &'gcx T {
        self.bump().alloc(value)
    }

    pub fn mk_ty(self, kind: TyKind<'gcx>) -> Ty<'gcx> {
        self.interner.intern_ty_with_flags(kind, |kind| TyFlags::calculate(self, kind))
    }

    pub fn mk_tys(self, tys: &[Ty<'gcx>]) -> &'gcx [Ty<'gcx>] {
        self.interner.intern_tys(tys)
    }

    pub fn mk_ty_iter(self, tys: impl Iterator<Item = Ty<'gcx>>) -> &'gcx [Ty<'gcx>] {
        self.interner.intern_ty_iter(tys)
    }

    pub fn mk_ty_string_literal(self, s: &[u8]) -> Ty<'gcx> {
        self.mk_ty(TyKind::StringLiteral(
            std::str::from_utf8(s).is_ok(),
            TypeSize::new(s.len().min(32) as u8).unwrap(),
        ))
    }

    pub fn mk_ty_int_literal(self, size: TypeSize) -> Ty<'gcx> {
        self.mk_ty(TyKind::IntLiteral(size))
    }

    pub fn mk_ty_fn_ptr(self, ptr: TyFnPtr<'gcx>) -> Ty<'gcx> {
        self.mk_ty(TyKind::FnPtr(self.interner.intern_ty_fn_ptr(ptr)))
    }

    pub fn mk_ty_fn(
        self,
        parameters: &[Ty<'gcx>],
        state_mutability: StateMutability,
        visibility: Visibility,
        returns: &[Ty<'gcx>],
    ) -> Ty<'gcx> {
        self.mk_ty_fn_ptr(TyFnPtr {
            parameters: self.mk_tys(parameters),
            returns: self.mk_tys(returns),
            state_mutability,
            visibility,
        })
    }

    pub(crate) fn mk_builtin_fn(
        self,
        parameters: &[Ty<'gcx>],
        state_mutability: StateMutability,
        returns: &[Ty<'gcx>],
    ) -> Ty<'gcx> {
        self.mk_ty_fn(parameters, state_mutability, Visibility::Internal, returns)
    }

    pub(crate) fn mk_builtin_mod(self, builtin: Builtin) -> Ty<'gcx> {
        self.mk_ty(TyKind::BuiltinModule(builtin))
    }

    pub fn mk_ty_err(self, guar: ErrorGuaranteed) -> Ty<'gcx> {
        Ty::new(self, TyKind::Err(guar))
    }

    /// Returns the name of the given item.
    ///
    /// # Panics
    ///
    /// Panics if the item has no name, such as unnamed function parameters.
    pub fn item_name(self, id: impl Into<hir::ItemId>) -> Ident {
        let id = id.into();
        self.item_name_opt(id).unwrap_or_else(|| panic!("item_name: missing name for item {id:?}"))
    }

    /// Returns the canonical name of the given item.
    ///
    /// This is the name of the item prefixed by the name of the contract it belongs to.
    pub fn item_canonical_name(self, id: impl Into<hir::ItemId>) -> impl fmt::Display {
        self.item_canonical_name_(id.into())
    }
    fn item_canonical_name_(self, id: hir::ItemId) -> impl fmt::Display {
        let name = self.item_name(id);
        let contract = self.hir.item(id).contract().map(|id| self.item_name(id));
        solar_data_structures::fmt_from_fn(move |f| {
            if let Some(contract) = contract {
                write!(f, "{contract}.")?;
            }
            write!(f, "{name}")
        })
    }

    /// Returns the fully qualified name of the contract.
    pub fn contract_fully_qualified_name(self, id: hir::ContractId) -> String {
        let c = self.hir.contract(id);
        let source = self.hir.source(c.source);
        format!("{}:{}", source.file.name.display(), c.name)
    }

    /// Returns an iterator over the fields of the given item.
    ///
    /// Accepts structs, functions, errors, and events.
    pub fn item_fields(
        self,
        id: impl Into<hir::ItemId>,
    ) -> impl Iterator<Item = (Ty<'gcx>, hir::VariableId)> {
        self.item_fields_(id.into())
    }

    fn item_fields_(self, id: hir::ItemId) -> impl Iterator<Item = (Ty<'gcx>, hir::VariableId)> {
        let tys = if let hir::ItemId::Struct(id) = id {
            self.struct_field_types(id)
        } else {
            self.item_parameter_types(id)
        };
        let params = self.item_parameters(id);
        debug_assert_eq!(tys.len(), params.len());
        std::iter::zip(tys.iter().copied(), params.iter().copied())
    }

    /// Returns the parameter variable declarations of the given function-like item.
    ///
    /// Also accepts structs.
    pub fn item_parameters(self, id: hir::ItemId) -> &'gcx [hir::VariableId] {
        self.item_parameters_opt(id)
            .unwrap_or_else(|| panic!("item_parameters: invalid item {id:?}"))
    }

    /// Returns the parameter variable declarations of the given function-like item.
    ///
    /// Also accepts structs.
    pub fn item_parameters_opt(self, id: hir::ItemId) -> Option<&'gcx [hir::VariableId]> {
        self.hir.item(id).parameters()
    }

    /// Returns the return variable declarations of the given function-like item.
    pub fn item_parameter_types(self, id: hir::ItemId) -> &'gcx [Ty<'gcx>] {
        self.item_parameter_types_opt(id)
            .unwrap_or_else(|| panic!("item_parameter_types: invalid item {id:?}"))
    }

    /// Returns the return variable declarations of the given function-like item.
    pub fn item_parameter_types_opt(self, id: hir::ItemId) -> Option<&'gcx [Ty<'gcx>]> {
        self.type_of_item(id).parameters()
    }

    /// Returns the name of the given item.
    pub fn item_name_opt(self, id: impl Into<hir::ItemId>) -> Option<Ident> {
        self.hir.item(id).name()
    }

    /// Returns the span of the given item.
    pub fn item_span(self, id: impl Into<hir::ItemId>) -> Span {
        self.hir.item(id).span()
    }

    /// Returns the 4-byte selector of the given item. Only accepts functions and errors.
    pub fn function_selector(self, id: impl Into<hir::ItemId>) -> Selector {
        let id = id.into();
        assert!(
            matches!(id, hir::ItemId::Function(_) | hir::ItemId::Error(_)),
            "function_selector: invalid item {id:?}"
        );
        self.item_selector(id)[..4].try_into().unwrap()
    }

    /// Returns the 32-byte selector of the given event.
    pub fn event_selector(self, id: hir::EventId) -> B256 {
        self.item_selector(id.into())
    }

    /// Computes the [`Ty`] of the given [`hir::Type`]. Not cached.
    pub fn type_of_hir_ty(self, ty: &hir::Type<'_>) -> Ty<'gcx> {
        let kind = match ty.kind {
            hir::TypeKind::Elementary(ty) => TyKind::Elementary(ty),
            hir::TypeKind::Array(array) => {
                let ty = self.type_of_hir_ty(&array.element);
                match array.size {
                    Some(size) => match crate::eval::ConstantEvaluator::new(self).eval(size) {
                        Ok(int) => {
                            if int.data.is_zero() {
                                let msg = "array length must be greater than zero";
                                self.dcx().err(msg).span(size.span).emit();
                            }
                            TyKind::Array(ty, int.data)
                        }
                        Err(guar) => TyKind::Err(guar),
                    },
                    None => TyKind::DynArray(ty),
                }
            }
            hir::TypeKind::Function(f) => {
                let parameters =
                    self.mk_ty_iter(f.parameters.iter().map(|ty| self.type_of_hir_ty(ty)));
                let returns = self.mk_ty_iter(f.returns.iter().map(|ty| self.type_of_hir_ty(ty)));
                TyKind::FnPtr(self.interner.intern_ty_fn_ptr(TyFnPtr {
                    parameters,
                    returns,
                    state_mutability: f.state_mutability,
                    visibility: f.visibility,
                }))
            }
            hir::TypeKind::Mapping(mapping) => {
                let key = self.type_of_hir_ty(&mapping.key);
                let value = self.type_of_hir_ty(&mapping.value);
                TyKind::Mapping(key, value)
            }
            hir::TypeKind::Custom(item) => return self.type_of_item_simple(item, ty.span),
            hir::TypeKind::Err(guar) => TyKind::Err(guar),
        };
        self.mk_ty(kind)
    }

    fn type_of_item_simple(self, id: hir::ItemId, span: Span) -> Ty<'gcx> {
        match id {
            hir::ItemId::Contract(_)
            | hir::ItemId::Struct(_)
            | hir::ItemId::Enum(_)
            | hir::ItemId::Udvt(_) => self.type_of_item(id),
            _ => {
                let msg = "name has to refer to a valid user-defined type";
                self.mk_ty_err(self.dcx().err(msg).span(span).emit())
            }
        }
    }

    #[allow(dead_code)]
    fn type_of_res(self, res: hir::Res) -> Ty<'gcx> {
        match res {
            hir::Res::Item(id) => self.type_of_item(id),
            hir::Res::Namespace(id) => self.mk_ty(TyKind::Module(id)),
            hir::Res::Builtin(builtin) => builtin.ty(self),
            hir::Res::Err(error_guaranteed) => self.mk_ty(TyKind::Err(error_guaranteed)),
        }
    }
}

macro_rules! cached {
    ($($(#[$attr:meta])* $vis:vis fn $name:ident($gcx:ident: _, $key:ident : $key_type:ty) -> $value:ty $imp:block)*) => {
        #[derive(Default)]
        struct Cache<'gcx> {
            $(
                $name: FxOnceMap<$key_type, $value>,
            )*
        }

        impl<'gcx> Gcx<'gcx> {
            $(
                $(#[$attr])*
                $vis fn $name(self, $key: $key_type) -> $value {
                    let _guard = log_cache_query(stringify!($name), &$key);
                    let mut hit = true;
                    let r = cache_insert(&self.cache.$name, $key, |&$key| {
                        hit = false;
                        let $gcx = self;
                        $imp
                    });
                    log_cache_query_result(&r, hit);
                    r
                }
            )*
        }
    };
}

cached! {
/// Returns the [ERC-165] interface ID of the given contract.
///
/// This is the XOR of the selectors of all function selectors in the interface.
///
/// The solc implementation excludes inheritance: <https://github.com/ethereum/solidity/blob/ad2644c52b3afbe80801322c5fe44edb59383500/libsolidity/ast/AST.cpp#L310-L316>
///
/// See [ERC-165] for more details.
///
/// [ERC-165]: https://eips.ethereum.org/EIPS/eip-165
pub fn interface_id(gcx: _, id: hir::ContractId) -> Selector {
    let kind = gcx.hir.contract(id).kind;
    assert!(kind.is_interface(), "{kind} {id:?} is not an interface");
    let selectors = gcx.interface_functions(id).own_functions().iter().map(|f| f.selector);
    selectors.fold(Selector::ZERO, std::ops::BitXor::bitxor)
}

/// Returns all the exported functions of the given contract.
///
/// The contract doesn't have to be an interface.
pub fn interface_functions(gcx: _, id: hir::ContractId) -> InterfaceFunctions<'gcx> {
    let c = gcx.hir.contract(id);
    let mut inheritance_start = None;
    let mut signatures_seen = FxHashSet::default();
    let mut hash_collisions = FxHashMap::default();
    let functions = c.linearized_bases.iter().flat_map(|&base| {
        let b = gcx.hir.contract(base);
        let functions =
            b.functions().filter(|&f| gcx.hir.function(f).is_part_of_external_interface());
        if base == id {
            assert!(inheritance_start.is_none(), "duplicate self ID in linearized_bases");
            inheritance_start = Some(functions.clone().count());
        }
        functions
    }).filter_map(|f_id| {
        let f = gcx.hir.function(f_id);
        let ty = gcx.type_of_item(f_id.into());
        let TyKind::FnPtr(ty_f) = ty.kind else { unreachable!() };
        let mut result = Ok(());
        for (var_id, ty) in f.variables().zip(ty_f.tys()) {
            if !ty.can_be_exported() {
                let msg = if ty.has_mapping() {
                    "types containing mappings cannot be parameter or return types of public functions"
                } else if ty.is_recursive() {
                    "recursive types cannot be parameter or return types of public functions"
                } else {
                    "this type cannot be parameter or return type of a public function"
                };
                let span = gcx.item_span(var_id);
                result = Err(gcx.dcx().err(msg).span(span).emit());
            }
        }
        if result.is_err() {
            return None;
        }

        // Virtual functions or ones with the same function parameter types are checked separately,
        // skip them here to avoid reporting them as selector hash collision errors below.
        let hash = gcx.item_selector(f_id.into());
        let selector: Selector = hash[..4].try_into().unwrap();
        if !signatures_seen.insert(hash) {
            return None;
        }

        // Check for selector hash collisions.
        if let Some(prev) = hash_collisions.insert(selector, f_id) {
            let f2 = gcx.hir.function(prev);
            let msg = "function signature hash collision";
            let full_note = format!(
                "the function signatures `{}` and `{}` produce the same 4-byte selector `{selector}`",
                gcx.item_signature(f_id.into()),
                gcx.item_signature(prev.into()),
            );
            gcx.dcx().err(msg).span(c.name.span).span_note(f.span, "first function").span_note(f2.span, "second function").note(full_note).emit();
        }

        Some(InterfaceFunction { selector, id: f_id, ty })
    });
    let functions = gcx.bump().alloc_from_iter(functions);
    let inheritance_start = inheritance_start.expect("linearized_bases did not contain self ID");
    InterfaceFunctions { functions, inheritance_start }
}

/// Returns the ABI signature of the given item. Only accepts functions, errors, and events.
pub fn item_signature(gcx: _, id: hir::ItemId) -> &'gcx str {
    let name = gcx.item_name(id);
    let tys = gcx.item_parameter_types(id);
    gcx.bump().alloc_str(&gcx.mk_abi_signature(name.as_str(), tys.iter().copied()))
}

fn item_selector(gcx: _, id: hir::ItemId) -> B256 {
    keccak256(gcx.item_signature(id))
}

/// Returns the type of the given item.
pub fn type_of_item(gcx: _, id: hir::ItemId) -> Ty<'gcx> {
    let kind = match id {
        hir::ItemId::Contract(id) => TyKind::Contract(id),
        hir::ItemId::Function(id) => {
            let f = gcx.hir.function(id);
            TyKind::FnPtr(gcx.interner.intern_ty_fn_ptr(TyFnPtr {
                parameters:
                    gcx.mk_ty_iter(f.parameters.iter().map(|&var| gcx.type_of_item(var.into()))),
                returns: gcx.mk_ty_iter(f.returns.iter().map(|&var| gcx.type_of_item(var.into()))),
                state_mutability: f.state_mutability,
                visibility: f.visibility,
            }))
        }
        hir::ItemId::Variable(id) => {
            let var = gcx.hir.variable(id);
            let ty = gcx.type_of_hir_ty(&var.ty);
            match (var.contract, var.data_location) {
                (_, Some(loc)) => TyKind::Ref(ty, loc),
                (Some(_), None) => TyKind::Ref(ty, DataLocation::Storage),
                (None, None) => return ty,
            }
        }
        hir::ItemId::Struct(id) => TyKind::Struct(id),
        hir::ItemId::Enum(id) => TyKind::Enum(id),
        hir::ItemId::Udvt(id) => {
            let udvt = gcx.hir.udvt(id);
            // TODO: let-chains plz
            let ty;
            if udvt.ty.kind.is_elementary() && {
                ty = gcx.type_of_hir_ty(&udvt.ty);
                ty.is_value_type()
            } {
                TyKind::Udvt(ty, id)
            } else {
                let msg = "the underlying type of UDVTs must be an elementary value type";
                TyKind::Err(gcx.dcx().err(msg).span(udvt.ty.span).emit())
            }
        }
        hir::ItemId::Error(id) => {
            let tys = gcx.hir.error(id).parameters.iter().map(|&p| gcx.type_of_item(p.into()));
            TyKind::Error(gcx.mk_ty_iter(tys), id)
        }
        hir::ItemId::Event(id) => {
            let tys = gcx.hir.event(id).parameters.iter().map(|&p| gcx.type_of_item(p.into()));
            TyKind::Event(gcx.mk_ty_iter(tys), id)
        }
    };
    gcx.mk_ty(kind)
}

/// Returns the types of the fields of the given struct.
pub fn struct_field_types(gcx: _, id: hir::StructId) -> &'gcx [Ty<'gcx>] {
    gcx.mk_ty_iter(gcx.hir.strukt(id).fields.iter().map(|&f| gcx.type_of_item(f.into())))
}

/// Returns the members of the given type.
pub fn members_of(gcx: _, ty: Ty<'gcx>) -> MemberMap<'gcx> {
    members::members_of(gcx, ty)
}
}

/// `OnceMap::insert` but with `Copy` keys and values.
fn cache_insert<K, V, S>(
    map: &once_map::OnceMap<K, V, S>,
    key: K,
    make_val: impl FnOnce(&K) -> V,
) -> V
where
    K: Copy + Eq + Hash,
    V: Copy,
    S: BuildHasher,
{
    map.map_insert(key, make_val, |_k, v| *v)
}

fn log_cache_query(name: &str, key: &dyn fmt::Debug) -> tracing::span::EnteredSpan {
    let guard = trace_span!("query", %name, ?key).entered();
    trace!("entered");
    guard
}

fn log_cache_query_result(result: &dyn fmt::Debug, hit: bool) {
    trace!(?result, hit);
}
