use crate::{
    Source, Sources, ast,
    ast_lowering::SymbolResolver,
    builtins::{Builtin, members},
    hir::{self, Hir, SourceId},
};
use alloy_primitives::{B256, Selector, U256, keccak256};
use either::Either;
use solar_ast::{DataLocation, StateMutability, TypeSize, Visibility};
use solar_data_structures::{
    BumpExt,
    fmt::{from_fn, or_list},
    map::{FxBuildHasher, FxHashMap, FxHashSet},
    smallvec::SmallVec,
    trustme,
};
use solar_interface::{
    Ident, Session, Span,
    config::CompilerStage,
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    source_map::{FileName, SourceFile},
};
use std::{
    fmt,
    hash::Hash,
    ops::ControlFlow,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use thread_local::ThreadLocal;

mod abi;
pub use abi::{TyAbiPrinter, TyAbiPrinterMode};

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
    /// The function ID.
    pub id: hir::FunctionId,
    /// The function 4-byte selector.
    pub selector: Selector,
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
    /// Returns all the functions.
    pub fn all(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        self.functions
    }

    /// Returns the defined functions.
    pub fn own(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        &self.functions[..self.inheritance_start]
    }

    /// Returns the inherited functions.
    pub fn inherited(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        &self.functions[self.inheritance_start..]
    }
}

impl<'gcx> std::ops::Deref for InterfaceFunctions<'gcx> {
    type Target = &'gcx [InterfaceFunction<'gcx>];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.functions
    }
}

impl<'gcx> IntoIterator for InterfaceFunctions<'gcx> {
    type Item = &'gcx InterfaceFunction<'gcx>;
    type IntoIter = std::slice::Iter<'gcx, InterfaceFunction<'gcx>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.functions.iter()
    }
}

/// Recursiveness of a type.
#[derive(Clone, Copy, Debug)]
pub enum Recursiveness {
    /// Not recursive.
    None,
    /// Recursive through indirection.
    Recursive,
    /// Recursive through direct reference. An error has already been emitted.
    Infinite(ErrorGuaranteed),
}

impl Recursiveness {
    /// Returns `true` if the type is not recursive.
    #[inline]
    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if the type is recursive.
    #[inline]
    pub fn is_recursive(self) -> bool {
        !self.is_none()
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

impl<'gcx> fmt::Debug for Gcx<'gcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Transparent wrapper around `&'gcx mut GlobalCtxt<'gcx>`.
///
/// This uses a raw pointer because using `&mut` directly would make `'gcx` covariant and this just
/// is too annoying/impossible to deal with.
/// Since it's only used internally (`pub(crate)`), this is fine.
#[repr(transparent)]
pub(crate) struct GcxMut<'gcx>(*mut GlobalCtxt<'gcx>);

impl<'gcx> GcxMut<'gcx> {
    #[inline(always)]
    pub(crate) fn new(gcx: &mut GlobalCtxt<'gcx>) -> Self {
        Self(gcx)
    }

    #[inline(always)]
    pub(crate) fn get(&self) -> Gcx<'gcx> {
        unsafe { Gcx(&*self.0) }
    }

    #[inline(always)]
    pub(crate) fn get_mut(&mut self) -> &'gcx mut GlobalCtxt<'gcx> {
        unsafe { &mut *self.0 }
    }
}

impl<'gcx> std::ops::Deref for GcxMut<'gcx> {
    type Target = &'gcx mut GlobalCtxt<'gcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

impl<'gcx> std::ops::DerefMut for GcxMut<'gcx> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

#[cfg(test)]
fn _gcx_traits() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Gcx<'static>>();
}

struct AtomicCompilerStage(AtomicUsize);

impl AtomicCompilerStage {
    fn new() -> Self {
        Self(AtomicUsize::new(usize::MAX))
    }

    fn set(&self, stage: CompilerStage) {
        self.0.store(stage as usize, Ordering::Relaxed);
    }

    fn get(&self) -> Option<CompilerStage> {
        let stage = self.0.load(Ordering::Relaxed);
        if stage == usize::MAX { None } else { Some(CompilerStage::from_repr(stage).unwrap()) }
    }
}

/// The global compilation context.
pub struct GlobalCtxt<'gcx> {
    pub sess: &'gcx Session,
    pub sources: Sources<'gcx>,
    pub(crate) symbol_resolver: SymbolResolver<'gcx>,
    pub hir: Hir<'gcx>,
    stage: AtomicCompilerStage,

    pub types: CommonTypes<'gcx>,

    pub(crate) ast_arenas: ThreadLocal<ast::Arena>,
    pub(crate) hir_arenas: ThreadLocal<hir::Arena>,
    interner: Interner<'gcx>,
    cache: Cache<'gcx>,
}

impl fmt::Debug for GlobalCtxt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalCtxt")
            .field("stage", &self.stage.get())
            .field("sess", self.sess)
            .field("sources", &self.sources.len())
            .finish_non_exhaustive()
    }
}

impl<'gcx> GlobalCtxt<'gcx> {
    pub(crate) fn new(sess: &'gcx Session) -> Self {
        let interner = Interner::new();
        let hir_arenas = ThreadLocal::<hir::Arena>::new();
        Self {
            sess,
            sources: Sources::new(),
            symbol_resolver: SymbolResolver::new(&sess.dcx),
            hir: Hir::new(),
            stage: AtomicCompilerStage::new(),

            // SAFETY: stable address because ThreadLocal holds the arenas through indirection.
            types: CommonTypes::new(
                &interner,
                unsafe { trustme::decouple_lt(&hir_arenas) }.get_or_default().bump(),
            ),

            ast_arenas: ThreadLocal::new(),
            hir_arenas,
            interner,
            cache: Cache::default(),
        }
    }
}

impl<'gcx> Gcx<'gcx> {
    pub(crate) fn new(gcx: &'gcx GlobalCtxt<'gcx>) -> Self {
        Self(gcx)
    }

    /// Returns the current compiler stage.
    pub fn stage(&self) -> Option<CompilerStage> {
        self.stage.get()
    }

    pub(crate) fn advance_stage(&self, to: CompilerStage) -> ControlFlow<()> {
        let from = self.stage();
        let result = self.advance_stage_(to);
        trace!(?from, ?to, ?result, "advance stage");
        result
    }

    fn advance_stage_(&self, to: CompilerStage) -> ControlFlow<()> {
        let current = self.stage();

        // Special case: allow calling `parse` multiple times while currently parsing.
        if to == CompilerStage::Parsing && current == Some(to) {
            return ControlFlow::Continue(());
        }

        let next = CompilerStage::next_opt(current);
        if next.is_none_or(|next| to != next) {
            let current_s = match current {
                Some(s) => s.to_str(),
                None => "none",
            };
            let next_s = match next {
                Some(s) => &format!("`{s}`"),
                None => "none (current stage is the last)",
            };
            self.dcx()
                .bug(format!(
                    "invalid compiler stage transition: cannot advance from `{current_s}` to `{to}`"
                ))
                .note(format!("expected next stage: {next_s}"))
                .note("stages must be advanced sequentially")
                .emit();
        }

        if let Some(current) = current
            && self.sess.stop_after(current)
        {
            return ControlFlow::Break(());
        }

        self.stage.set(to);
        ControlFlow::Continue(())
    }

    /// Returns the diagnostics context.
    pub fn dcx(self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    pub fn arena(self) -> &'gcx hir::Arena {
        self.hir_arenas.get_or_default()
    }

    pub fn bump(self) -> &'gcx bumpalo::Bump {
        self.arena().bump()
    }

    pub fn alloc<T>(self, value: T) -> &'gcx T {
        self.bump().alloc(value)
    }

    pub fn mk_ty(self, kind: TyKind<'gcx>) -> Ty<'gcx> {
        self.interner.intern_ty_with_flags(self.bump(), kind, |kind| TyFlags::calculate(self, kind))
    }

    pub fn mk_tys(self, tys: &[Ty<'gcx>]) -> &'gcx [Ty<'gcx>] {
        self.interner.intern_tys(self.bump(), tys)
    }

    pub fn mk_ty_iter(self, tys: impl Iterator<Item = Ty<'gcx>>) -> &'gcx [Ty<'gcx>] {
        self.interner.intern_ty_iter(self.bump(), tys)
    }

    fn mk_item_tys<T: Into<hir::ItemId> + Copy>(self, ids: &[T]) -> &'gcx [Ty<'gcx>] {
        self.mk_ty_iter(ids.iter().map(|&id| self.type_of_item(id.into())))
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
        self.mk_ty(TyKind::FnPtr(self.interner.intern_ty_fn_ptr(self.bump(), ptr)))
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

    /// Returns the source file with the given path, if it exists.
    pub fn get_file(self, name: impl Into<FileName>) -> Option<Arc<SourceFile>> {
        self.sess.source_map().get_file(name)
    }

    /// Returns the AST source at the given path, if it exists.
    pub fn get_ast_source(
        self,
        name: impl Into<FileName>,
    ) -> Option<(SourceId, &'gcx Source<'gcx>)> {
        let file = self.get_file(name)?;
        self.sources.get_file(&file)
    }

    /// Returns the HIR source at the given path, if it exists.
    pub fn get_hir_source(
        self,
        name: impl Into<FileName>,
    ) -> Option<(SourceId, &'gcx hir::Source<'gcx>)> {
        let file = self.get_file(name)?;
        self.hir.sources.iter_enumerated().find(|(_, source)| Arc::ptr_eq(&source.file, &file))
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
        from_fn(move |f| {
            if let Some(contract) = contract {
                write!(f, "{contract}.")?;
            }
            write!(f, "{name}")
        })
    }

    /// Returns the fully qualified name of the contract.
    pub fn contract_fully_qualified_name(
        self,
        id: hir::ContractId,
    ) -> impl fmt::Display + use<'gcx> {
        from_fn(move |f| {
            let c = self.hir.contract(id);
            let source = self.hir.source(c.source);
            write!(f, "{}:{}", source.file.name.display(), c.name)
        })
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
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function-like item or a struct.
    pub fn item_parameters(self, id: impl Into<hir::ItemId>) -> &'gcx [hir::VariableId] {
        let id = id.into();
        self.item_parameters_opt(id)
            .unwrap_or_else(|| panic!("item_parameters: invalid item {id:?}"))
    }

    /// Returns the parameter variable declarations of the given function-like item.
    ///
    /// Also accepts structs.
    pub fn item_parameters_opt(
        self,
        id: impl Into<hir::ItemId>,
    ) -> Option<&'gcx [hir::VariableId]> {
        self.hir.item(id).parameters()
    }

    /// Returns the return variable declarations of the given function-like item.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function-like item.
    pub fn item_parameter_types(self, id: impl Into<hir::ItemId>) -> &'gcx [Ty<'gcx>] {
        let id = id.into();
        self.item_parameter_types_opt(id)
            .unwrap_or_else(|| panic!("item_parameter_types: invalid item {id:?}"))
    }

    /// Returns the return variable declarations of the given function-like item.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function-like item.
    pub fn item_parameter_types_opt(self, id: impl Into<hir::ItemId>) -> Option<&'gcx [Ty<'gcx>]> {
        self.type_of_item(id.into()).parameters()
    }

    /// Returns the name of the given item.
    #[inline]
    pub fn item_name_opt(self, id: impl Into<hir::ItemId>) -> Option<Ident> {
        self.hir.item(id).name()
    }

    /// Returns the span of the given item.
    #[inline]
    pub fn item_span(self, id: impl Into<hir::ItemId>) -> Span {
        self.hir.item(id).span()
    }

    /// Returns the 4-byte selector of the given item. Only accepts functions and errors.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function or error.
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
                                let guar = self.dcx().err(msg).span(size.span).emit();
                                TyKind::Array(self.mk_ty_err(guar), int.data)
                            } else {
                                TyKind::Array(ty, int.data)
                            }
                        }
                        Err(guar) => TyKind::Array(self.mk_ty_err(guar), U256::from(1)),
                    },
                    None => TyKind::DynArray(ty),
                }
            }
            hir::TypeKind::Function(f) => {
                return self.mk_ty_fn_ptr(TyFnPtr {
                    parameters: self.mk_item_tys(f.parameters),
                    returns: self.mk_item_tys(f.returns),
                    state_mutability: f.state_mutability,
                    visibility: f.visibility,
                });
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

    /// Returns the type of the given [`hir::Res`].
    pub fn type_of_res(self, res: hir::Res) -> Ty<'gcx> {
        match res {
            hir::Res::Item(id) => self.type_of_item(id),
            hir::Res::Namespace(id) => self.mk_ty(TyKind::Module(id)),
            hir::Res::Builtin(builtin) => builtin.ty(self),
            hir::Res::Err(guar) => self.mk_ty_err(guar),
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
                    #[cfg(false)]
                    let _guard = log_cache_query(stringify!($name), &$key);
                    #[cfg(false)]
                    let mut hit = true;
                    let r = cache_insert(&self.cache.$name, $key, |&$key| {
                        #[cfg(false)]
                        {
                            hit = false;
                        }
                        let $gcx = self;
                        $imp
                    });
                    #[cfg(false)]
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
/// The solc implementation excludes inheritance: <https://github.com/argotorg/solidity/blob/ad2644c52b3afbe80801322c5fe44edb59383500/libsolidity/ast/AST.cpp#L310-L316>
///
/// See [ERC-165] for more details.
///
/// [ERC-165]: https://eips.ethereum.org/EIPS/eip-165
pub fn interface_id(gcx: _, id: hir::ContractId) -> Selector {
    let kind = gcx.hir.contract(id).kind;
    assert!(kind.is_interface(), "{kind} {id:?} is not an interface");
    let selectors = gcx.interface_functions(id).own().iter().map(|f| f.selector);
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
            if let Err(guar) = ty.has_error() {
                result = Err(guar);
                continue;
            }
            if !ty.can_be_exported() {
                // TODO: implement `interfaceType`
                if c.kind.is_library() {
                    result = Err(ErrorGuaranteed::new_unchecked());
                    continue;
                }

                let kind = f.description();
                let msg = if ty.has_mapping() {
                    format!("types containing mappings cannot be parameter or return types of public {kind}s")
                } else if ty.is_recursive() {
                    format!("recursive types cannot be parameter or return types of public {kind}s")
                } else {
                    format!("this type cannot be parameter or return type of a public {kind}")
                };
                let span = gcx.hir.variable(var_id).ty.span;
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
    trace!("{}.interfaceFunctions.len() = {}", gcx.contract_fully_qualified_name(id), functions.len());
    let inheritance_start = inheritance_start.expect("linearized_bases did not contain self ID");
    InterfaceFunctions { functions, inheritance_start }
}

/// Returns the ABI signature of the given item. Only accepts functions, errors, and events.
pub fn item_signature(gcx: _, id: hir::ItemId) -> &'gcx str {
    let name = gcx.item_name(id);
    let tys = gcx.item_parameter_types(id);
    gcx.bump().alloc_str(&gcx.mk_abi_signature(name.as_str(), tys.iter().copied()))
}

pub(crate) fn item_selector(gcx: _, id: hir::ItemId) -> B256 {
    keccak256(gcx.item_signature(id))
}

/// Returns the type of the given item.
pub fn type_of_item(gcx: _, id: hir::ItemId) -> Ty<'gcx> {
    let kind = match id {
        hir::ItemId::Contract(id) => TyKind::Contract(id),
        hir::ItemId::Function(id) => {
            let f = gcx.hir.function(id);
            TyKind::FnPtr(gcx.interner.intern_ty_fn_ptr(gcx.bump(), TyFnPtr {
                parameters: gcx.mk_item_tys(f.parameters),
                returns: gcx.mk_item_tys(f.returns),
                state_mutability: f.state_mutability,
                visibility: f.visibility,
            }))
        }
        hir::ItemId::Variable(id) => {
            let var = gcx.hir.variable(id);
            let ty = gcx.type_of_hir_ty(&var.ty);
            return var_type(gcx, var, ty);
        }
        hir::ItemId::Struct(id) => TyKind::Struct(id),
        hir::ItemId::Enum(id) => TyKind::Enum(id),
        hir::ItemId::Udvt(id) => {
            let udvt = gcx.hir.udvt(id);
            if udvt.ty.kind.is_elementary()
                && let ty = gcx.type_of_hir_ty(&udvt.ty)
                && ty.is_value_type()
            {
                TyKind::Udvt(ty, id)
            } else {
                let msg = "the underlying type of UDVTs must be an elementary value type";
                TyKind::Err(gcx.dcx().err(msg).span(udvt.ty.span).emit())
            }
        }
        hir::ItemId::Error(id) => {
            TyKind::Error(gcx.mk_item_tys(gcx.hir.error(id).parameters), id)
        }
        hir::ItemId::Event(id) => {
            TyKind::Event(gcx.mk_item_tys(gcx.hir.event(id).parameters), id)
        }
    };
    gcx.mk_ty(kind)
}

/// Returns the types of the fields of the given struct.
pub fn struct_field_types(gcx: _, id: hir::StructId) -> &'gcx [Ty<'gcx>] {
    gcx.mk_ty_iter(gcx.hir.strukt(id).fields.iter().map(|&f| gcx.type_of_item(f.into())))
}

/// Returns the recursiveness of the given struct.
pub fn struct_recursiveness(gcx: _, id: hir::StructId) -> Recursiveness {
    use solar_data_structures::cycle::*;

    let r = CycleDetector::detect(gcx, id, |gcx, cd, id| {
        let s = gcx.hir.strukt(id);

        if cd.depth() >= 256 {
            let guar = gcx.dcx().err("struct is too deeply nested").span(s.span).emit();
            return CycleDetectorResult::Break(Either::Left(guar));
        }

        for &field_id in s.fields {
            let field = gcx.hir.variable(field_id);
            let mut check = |ty: &hir::Type<'_>, dynamic: bool| {
                if let hir::TypeKind::Custom(hir::ItemId::Struct(other)) = ty.kind {
                    match cd.run(other) {
                        CycleDetectorResult::Continue => {}
                        CycleDetectorResult::Cycle(_) if dynamic => {
                            return CycleDetectorResult::Break(Either::Right(()));
                        }
                        r => return r,
                    }
                }
                CycleDetectorResult::Continue
            };
            let mut dynamic = false;
            let mut ty = &field.ty;
            while let hir::TypeKind::Array(array) = ty.kind {
                if array.size.is_none() {
                    dynamic = true;
                }
                ty = &array.element;
            }
            cdr_try!(check(ty, dynamic));
            if let ControlFlow::Break(r) = field.ty.visit(&gcx.hir, &mut |ty| check(ty, true).to_controlflow()) {
                return r;
            }
        }

        CycleDetectorResult::Continue
    });
    match r {
        CycleDetectorResult::Continue => Recursiveness::None,
        CycleDetectorResult::Break(Either::Left(guar)) => Recursiveness::Infinite(guar),
        CycleDetectorResult::Break(Either::Right(())) => Recursiveness::Recursive,
        CycleDetectorResult::Cycle(id) => Recursiveness::Infinite(
            gcx.dcx().err("recursive struct definition").span(gcx.item_span(id)).emit()
        ),
    }
}

/// Returns the members of the given type.
pub fn members_of(gcx: _, ty: Ty<'gcx>) -> members::MemberList<'gcx> {
    members::members_of(gcx, ty)
}
}

fn var_type<'gcx>(gcx: Gcx<'gcx>, var: &'gcx hir::Variable<'gcx>, ty: Ty<'gcx>) -> Ty<'gcx> {
    use hir::DataLocation::*;

    // https://github.com/argotorg/solidity/blob/48d40d5eaf97c835cf55896a7a161eedc57c57f9/libsolidity/ast/AST.cpp#L820
    let has_reference_or_mapping_type = ty.is_reference_type() || ty.has_mapping();
    let mut func_vis = None;
    let mut locs;
    let allowed: &[_] = if var.is_state_variable() {
        &[None, Some(Transient)]
    } else if !has_reference_or_mapping_type || var.is_event_or_error_parameter() {
        &[None]
    } else if var.is_callable_or_catch_parameter() {
        locs = SmallVec::<[_; 3]>::new();
        locs.push(Some(Memory));
        let mut is_constructor_parameter = false;
        if let Some(f) = var.function {
            let f = gcx.hir.function(f);
            is_constructor_parameter = f.kind.is_constructor();
            if !var.is_try_catch_parameter() && !is_constructor_parameter {
                func_vis = Some(f.visibility);
            }
            if is_constructor_parameter
                || f.visibility <= hir::Visibility::Internal
                || f.contract.is_some_and(|c| gcx.hir.contract(c).kind.is_library())
            {
                locs.push(Some(Storage));
            }
        }
        if !var.is_try_catch_parameter() && !is_constructor_parameter {
            locs.push(Some(Calldata));
        }
        &locs
    } else if var.is_local_variable() {
        &[Some(Memory), Some(Storage), Some(Calldata)]
    } else {
        &[None]
    };

    let mut var_loc = var.data_location;
    if !allowed.contains(&var_loc) {
        if ty.has_error().is_ok() {
            let msg = if !has_reference_or_mapping_type {
                "data location can only be specified for array, struct or mapping types".to_string()
            } else if let Some(var_loc) = var_loc {
                format!("invalid data location `{var_loc}`")
            } else {
                "expected data location".to_string()
            };
            let mut err = gcx.dcx().err(msg).span(var.span);
            if has_reference_or_mapping_type {
                let note = format!(
                    "data location must be {expected} for {vis}{descr}{got}",
                    expected = or_list(
                        allowed.iter().map(|d| format!("`{}`", DataLocation::opt_to_str(*d)))
                    ),
                    vis = if let Some(vis) = func_vis { format!("{vis} ") } else { String::new() },
                    descr = var.description(),
                    got = if let Some(var_loc) = var_loc {
                        format!(", but got `{var_loc}`")
                    } else {
                        String::new()
                    },
                );
                err = err.note(note);
            }
            err.emit();
        }
        var_loc = allowed[0];
    }

    let ty_loc = if var.is_event_or_error_parameter() || var.is_file_level_variable() {
        Memory
    } else if var.is_state_variable() {
        let mut_specified = var.mutability.is_some();
        match var_loc {
            None => {
                if mut_specified {
                    Memory
                } else {
                    Storage
                }
            }
            Some(Transient) => {
                if mut_specified {
                    let msg = "transient cannot be used as data location for constant or immutable variables";
                    gcx.dcx().err(msg).span(var.span).emit();
                }
                if var.initializer.is_some() {
                    let msg =
                        "initialization of transient storage state variables is not supported";
                    gcx.dcx().err(msg).span(var.span).emit();
                }
                Transient
            }
            Some(_) => unreachable!(),
        }
    } else if var.is_struct_member() {
        Storage
    } else {
        match var_loc {
            Some(loc @ (Memory | Storage | Calldata)) => loc,
            Some(Transient) => unimplemented!(),
            None => {
                assert!(!has_reference_or_mapping_type, "data location not properly set");
                Memory
            }
        }
    };

    if ty.is_reference_type() { ty.with_loc(gcx, ty_loc) } else { ty }
}

/// `OnceMap::insert` but with `Copy` keys and values.
#[inline]
fn cache_insert<K, V>(map: &FxOnceMap<K, V>, key: K, make_val: impl FnOnce(&K) -> V) -> V
where
    K: Copy + Eq + Hash,
    V: Copy,
{
    map.map_insert(key, make_val, cache_insert_with_result)
}

#[inline]
fn cache_insert_with_result<K, V: Copy>(_: &K, v: &V) -> V {
    *v
}

#[cfg(false)]
fn log_cache_query(name: &str, key: &dyn fmt::Debug) -> tracing::span::EnteredSpan {
    let guard = trace_span!("query", %name, ?key).entered();
    trace!("entered");
    guard
}

#[cfg(false)]
fn log_cache_query_result(result: &dyn fmt::Debug, hit: bool) {
    trace!(?result, hit);
}
