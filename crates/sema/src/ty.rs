use crate::{
    builtins::{
        members::{self, MemberMap},
        Builtin,
    },
    hir::{self, Hir},
};
use alloy_primitives::{keccak256, Selector, B256};
use dashmap::SharedValue;
use solar_ast::ast::{DataLocation, ElementaryType, StateMutability, TypeSize, Visibility};
use solar_data_structures::{
    map::{FxBuildHasher, FxHashMap, FxHashSet},
    smallvec::SmallVec,
    BumpExt, Interned,
};
use solar_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    Ident, Session, Span,
};
use std::{
    borrow::Borrow,
    fmt,
    hash::{BuildHasher, Hash},
    ops::ControlFlow,
};
use thread_local::ThreadLocal;

type FxDashSet<T> = dashmap::DashMap<T, (), FxBuildHasher>;

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

    interner: Interner<'gcx>,
    cache: Cache<'gcx>,
}

impl<'gcx> GlobalCtxt<'gcx> {
    pub(crate) fn new(
        sess: &'gcx Session,
        arena: &'gcx ThreadLocal<hir::Arena>,
        hir: Hir<'gcx>,
    ) -> GlobalCtxt<'gcx> {
        let interner = Interner::new(arena);
        Self { sess, types: CommonTypes::new(&interner), hir, interner, cache: Cache::default() }
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

    pub fn intern_ty(self, kind: TyKind<'gcx>) -> Ty<'gcx> {
        self.interner.intern_ty_with_flags(kind, |kind| TyFlags::calculate(self, kind))
    }

    pub fn intern_tys(self, tys: &[Ty<'gcx>]) -> &'gcx [Ty<'gcx>] {
        if tys.is_empty() {
            return &[];
        }
        self.interner.ty_lists.intern_ref(tys, || self.bump().alloc_slice_copy(tys))
    }

    pub fn intern_ty_iter(self, tys: impl Iterator<Item = Ty<'gcx>>) -> &'gcx [Ty<'gcx>] {
        solar_data_structures::CollectAndApply::collect_and_apply(tys, |tys| self.intern_tys(tys))
    }

    pub fn intern_fn_ptr(self, ptr: TyFnPtr<'gcx>) -> &'gcx TyFnPtr<'gcx> {
        self.interner.fn_ptrs.intern(ptr, |ptr| self.bump().alloc(ptr))
    }

    pub fn mk_ty(self, kind: TyKind<'gcx>) -> Ty<'gcx> {
        self.intern_ty(kind)
    }

    pub fn mk_ty_string_literal(self, s: &[u8]) -> Ty<'gcx> {
        self.mk_ty(TyKind::StringLiteral(
            core::str::from_utf8(s).is_ok(),
            TypeSize::new(s.len().min(32) as u8).unwrap(),
        ))
    }

    pub fn mk_ty_int_literal(self, size: TypeSize) -> Ty<'gcx> {
        self.mk_ty(TyKind::IntLiteral(size))
    }

    pub fn mk_ty_fn_ptr(self, ptr: TyFnPtr<'gcx>) -> Ty<'gcx> {
        self.mk_ty(TyKind::FnPtr(self.intern_fn_ptr(ptr)))
    }

    pub fn mk_ty_fn(
        self,
        parameters: &[Ty<'gcx>],
        state_mutability: StateMutability,
        visibility: Visibility,
        returns: &[Ty<'gcx>],
    ) -> Ty<'gcx> {
        self.mk_ty_fn_ptr(TyFnPtr {
            parameters: self.intern_tys(parameters),
            returns: self.intern_tys(returns),
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

    fn mk_signature(self, name: &str, tys: impl IntoIterator<Item = Ty<'gcx>>) -> String {
        let mut s = String::with_capacity(64);
        s.push_str(name);
        TyPrinter::new(self, &mut s).print_tuple(tys).unwrap();
        s
    }

    /// Returns the name of the given item.
    ///
    /// # Panics
    ///
    /// Panics if the item has no name, such as unnamed function parameters.
    pub fn item_name(self, id: impl Into<hir::ItemId>) -> Ident {
        let id = id.into();
        self.opt_item_name(id).unwrap_or_else(|| panic!("item_name: missing name for item {id:?}"))
    }

    /// Returns the name of the given item.
    pub fn opt_item_name(self, id: impl Into<hir::ItemId>) -> Option<Ident> {
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

    pub fn type_of_hir_ty(self, ty: &hir::Type<'_>) -> Ty<'gcx> {
        let kind = match ty.kind {
            hir::TypeKind::Elementary(ty) => TyKind::Elementary(ty),
            hir::TypeKind::Array(array) => {
                let ty = self.type_of_hir_ty(&array.element);
                match array.size {
                    // TODO
                    Some(_size) => TyKind::Array(ty, 1),
                    None => TyKind::DynArray(ty),
                }
            }
            hir::TypeKind::Function(f) => {
                let parameters =
                    self.intern_ty_iter(f.parameters.iter().map(|ty| self.type_of_hir_ty(ty)));
                let returns =
                    self.intern_ty_iter(f.returns.iter().map(|ty| self.type_of_hir_ty(ty)));
                TyKind::FnPtr(self.intern_fn_ptr(TyFnPtr {
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
    ($($(#[$attr:meta])* $vis:vis fn $name:ident($gcx:ident, $key:ident : $key_type:ty) -> $value:ty $imp:block)*) => {
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
                    let mut hit = true;
                    let r = once_map_insert(&self.cache.$name, $key, |&$key| {
                        hit = false;
                        let $gcx = self;
                        $imp
                    });
                    log_cache_query(stringify!($name), &$key, &r, hit);
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
pub fn interface_id(gcx, id: hir::ContractId) -> Selector {
    let kind = gcx.hir.contract(id).kind;
    assert!(kind.is_interface(), "{kind} {id:?} is not an interface");
    let selectors = gcx.interface_functions(id).own_functions().iter().map(|f| f.selector);
    selectors.fold(Selector::ZERO, std::ops::BitXor::bitxor)
}

/// Returns all the exported functions of the given contract.
///
/// The contract doesn't have to be an interface.
pub fn interface_functions(gcx, id: hir::ContractId) -> InterfaceFunctions<'gcx> {
    let c = gcx.hir.contract(id);
    let mut inheritance_start = None;
    let mut duplicates = FxHashMap::default();
    let functions = c.linearized_bases.iter().flat_map(|&base| {
        let b = gcx.hir.contract(base);
        let functions =
            b.functions().filter(|&f| gcx.hir.function(f).is_part_of_external_interface());
        if base == id {
            assert!(inheritance_start.is_none(), "duplicate self ID in linearized_bases");
            inheritance_start = Some(functions.clone().count());
        }
        functions
    }).map(|f_id| {
        let f = gcx.hir.function(f_id);

        let ty = gcx.type_of_item(f_id.into());
        let TyKind::FnPtr(ty_f) = ty.kind else { unreachable!() };
        let mut result = Ok(());
        for (var_id, ty) in f.vars().zip(ty_f.tys()) {
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
        if let Err(guar) = result {
            return InterfaceFunction {
                selector: Selector::ZERO,
                id: f_id,
                ty: gcx.mk_ty_err(guar),
            };
        }

        let selector = gcx.function_selector(f_id);
        if let Some(prev) = duplicates.insert(selector, f_id) {
            let f2 = gcx.hir.function(prev);
            let msg = "function signature hash collision";
            let full_note = format!(
                "the function signatures `{}` and `{}` produce the same 4-byte selector `{selector}`",
                gcx.item_signature(f_id.into()),
                gcx.item_signature(prev.into()),
            );
            gcx.dcx().err(msg).span(c.name.span).span_note(f.span, "first function").span_note(f2.span, "second function").note(full_note).emit();
        }
        InterfaceFunction {
            selector,
            id: f_id,
            ty,
        }
    });
    let functions = gcx.bump().alloc_from_iter(functions);
    let inheritance_start = inheritance_start.expect("linearized_bases did not contain self ID");
    InterfaceFunctions { functions, inheritance_start }
}

/// Returns the ABI signature of the given item. Only accepts functions, errors, and events.
pub fn item_signature(gcx, id: hir::ItemId) -> &'gcx str {
    let name = gcx.item_name(id);
    let ty = gcx.type_of_item(id);
    let tys = match ty.kind {
        TyKind::FnPtr(f) => f.parameters,
        TyKind::Event(parameters, _) | TyKind::Error(parameters, _) => parameters,
        _ => panic!("item_signature: invalid item type {ty:?}"),
    };
    gcx.bump().alloc_str(&gcx.mk_signature(name.as_str(), tys.iter().copied()))
}

fn item_selector(gcx, id: hir::ItemId) -> B256 {
    keccak256(gcx.item_signature(id))
}

/// Returns the type of the given item.
pub fn type_of_item(gcx, id: hir::ItemId) -> Ty<'gcx> {
    let kind = match id {
        hir::ItemId::Contract(id) => TyKind::Contract(id),
        hir::ItemId::Function(id) => {
            let f = gcx.hir.function(id);
            TyKind::FnPtr(gcx.intern_fn_ptr(TyFnPtr {
                parameters: gcx
                .intern_ty_iter(f.parameters.iter().map(|&var| gcx.type_of_item(var.into()))),
                returns: gcx
                .intern_ty_iter(f.returns.iter().map(|&var| gcx.type_of_item(var.into()))),
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
        hir::ItemId::Struct(id) => TyKind::Struct(id, ),
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
            let tys = gcx.hir.error(id).parameters.iter().map(|p| gcx.type_of_hir_ty(&p.ty));
            TyKind::Error(gcx.intern_ty_iter(tys), id)
        }
        hir::ItemId::Event(id) => {
            let tys = gcx.hir.event(id).parameters.iter().map(|p| gcx.type_of_hir_ty(&p.ty));
            TyKind::Event(gcx.intern_ty_iter(tys), id)
        }
    };
    gcx.mk_ty(kind)
}

/// Returns the types of the fields of the given struct.
pub fn struct_field_types(gcx, id: hir::StructId) -> &'gcx [Ty<'gcx>] {
    let fields = gcx.hir.strukt(id).fields;
    gcx.intern_ty_iter(fields.iter().map(|f| gcx.type_of_hir_ty(&f.ty)))
}

/// Returns the members of the given type.
pub fn members_of(gcx, ty: Ty<'gcx>) -> MemberMap<'gcx> {
    members::members_of(gcx, ty)
}
}

struct TyPrinter<'gcx, W> {
    gcx: Gcx<'gcx>,
    buf: W,
}

impl<'gcx, W: fmt::Write> TyPrinter<'gcx, W> {
    fn new(gcx: Gcx<'gcx>, buf: W) -> Self {
        Self { gcx, buf }
    }

    fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        debug_assert!(ty.can_be_exported(), "{ty:?} cannot be exported");
        match ty.kind {
            TyKind::Elementary(ty) => ty.write_abi_str(&mut self.buf),
            TyKind::Contract(_) => self.buf.write_str("address"),
            TyKind::FnPtr(_) => self.buf.write_str("function"),
            TyKind::Struct(id) => self.print_tuple(self.gcx.struct_field_types(id).iter().copied()),
            TyKind::Enum(_) => self.buf.write_str("uint8"),
            TyKind::Udvt(ty, _) => self.print(ty),
            TyKind::Ref(ty, _loc) => self.print(ty),
            TyKind::DynArray(ty) => {
                self.print(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print(ty)?;
                write!(self.buf, "[{len}]")
            }
            _ => panic!("printing invalid type: {ty:?}"),
        }
    }

    fn print_tuple(&mut self, tys: impl IntoIterator<Item = Ty<'gcx>>) -> fmt::Result {
        write!(self.buf, "(")?;
        for (i, ty) in tys.into_iter().enumerate() {
            if i > 0 {
                write!(self.buf, ",")?;
            }
            self.print(ty)?;
        }
        write!(self.buf, ")")
    }
}

struct Interner<'gcx> {
    arena: &'gcx ThreadLocal<hir::Arena>,
    tys: FxDashSet<&'gcx TyData<'gcx>>,
    ty_lists: FxDashSet<&'gcx [Ty<'gcx>]>,
    fn_ptrs: FxDashSet<&'gcx TyFnPtr<'gcx>>,
}

impl<'gcx> Interner<'gcx> {
    fn new(arena: &'gcx ThreadLocal<hir::Arena>) -> Self {
        Self {
            arena,
            tys: Default::default(),
            ty_lists: Default::default(),
            fn_ptrs: Default::default(),
        }
    }

    fn intern_ty_with_flags(
        &self,
        kind: TyKind<'gcx>,
        mk_flags: impl FnOnce(&TyKind<'gcx>) -> TyFlags,
    ) -> Ty<'gcx> {
        Ty(Interned::new_unchecked(self.tys.intern(kind, |kind| {
            self.arena.get_or_default().alloc(TyData { flags: mk_flags(&kind), kind })
        })))
    }
}

/// Pre-interned types.
pub struct CommonTypes<'gcx> {
    /// Empty tuple `()`, AKA unit, void.
    pub unit: Ty<'gcx>,
    /// `bool`.
    pub bool: Ty<'gcx>,

    /// `address`.
    pub address: Ty<'gcx>,
    /// `address payable`.
    pub address_payable: Ty<'gcx>,

    /// `string`.
    pub string: Ty<'gcx>,
    /// `string` references.
    pub string_ref: EachDataLoc<Ty<'gcx>>,

    /// `bytes`.
    pub bytes: Ty<'gcx>,
    /// `bytes` references.
    pub bytes_ref: EachDataLoc<Ty<'gcx>>,

    ints: [Ty<'gcx>; 32],
    uints: [Ty<'gcx>; 32],
    fbs: [Ty<'gcx>; 32],
}

impl<'gcx> CommonTypes<'gcx> {
    #[instrument(name = "new_common_types", level = "debug", skip_all)]
    #[inline]
    fn new(interner: &Interner<'gcx>) -> Self {
        use std::array::from_fn;
        use ElementaryType::*;
        use TyKind::*;

        // NOTE: We need to skip calculating flags here because it would require `Gcx` when we
        // haven't built one yet. This is fine since elementary types don't have any flags.
        // If that ever changes, then this closure should also reflect that.
        let mk = |kind| interner.intern_ty_with_flags(kind, |_| TyFlags::empty());
        let mk_refs = |ty| EachDataLoc {
            storage: mk(Ref(ty, DataLocation::Storage)),
            transient: mk(Ref(ty, DataLocation::Transient)),
            memory: mk(Ref(ty, DataLocation::Memory)),
            calldata: mk(Ref(ty, DataLocation::Calldata)),
        };

        let string = mk(Elementary(String));
        let bytes = mk(Elementary(Bytes));

        Self {
            unit: mk(Tuple(&[])),
            // never: mk(Elementary(Never)),
            bool: mk(Elementary(Bool)),

            address: mk(Elementary(Address(false))),
            address_payable: mk(Elementary(Address(true))),

            string,
            string_ref: mk_refs(string),

            bytes,
            bytes_ref: mk_refs(bytes),

            ints: from_fn(|i| mk(Elementary(Int(TypeSize::new(i as u8 + 1).unwrap())))),
            uints: from_fn(|i| mk(Elementary(UInt(TypeSize::new(i as u8 + 1).unwrap())))),
            fbs: from_fn(|i| mk(Elementary(FixedBytes(TypeSize::new(i as u8 + 1).unwrap())))),
        }
    }

    /// `int<bits>`.
    #[inline]
    #[track_caller]
    pub fn int(&self, bits: u16) -> Ty<'gcx> {
        self.int_(TypeSize::new_int_bits(bits))
    }
    /// `int<size>`.
    pub fn int_(&self, size: TypeSize) -> Ty<'gcx> {
        self.ints[size.bytes() as usize - 1]
    }

    /// `uint<bits>`.
    #[inline]
    #[track_caller]
    pub fn uint(&self, bits: u16) -> Ty<'gcx> {
        self.uint_(TypeSize::new_int_bits(bits))
    }
    /// `uint<size>`.
    pub fn uint_(&self, size: TypeSize) -> Ty<'gcx> {
        self.uints[size.bytes() as usize - 1]
    }

    /// `bytes<bytes>`.
    #[inline]
    #[track_caller]
    pub fn fixed_bytes(&self, bytes: u8) -> Ty<'gcx> {
        self.fixed_bytes_(TypeSize::new_fb_bytes(bytes))
    }
    /// `bytes<size>`.
    pub fn fixed_bytes_(&self, size: TypeSize) -> Ty<'gcx> {
        self.fbs[size.bytes() as usize - 1]
    }
}

/// Holds an instance of `T` for each data location.
pub struct EachDataLoc<T> {
    pub storage: T,
    pub transient: T,
    pub memory: T,
    pub calldata: T,
}

impl<T> EachDataLoc<T> {
    /// Gets a copy for the given data location.
    #[inline]
    pub fn get(&self, loc: DataLocation) -> T
    where
        T: Copy,
    {
        match loc {
            DataLocation::Storage => self.storage,
            DataLocation::Transient => self.transient,
            DataLocation::Memory => self.memory,
            DataLocation::Calldata => self.calldata,
        }
    }

    /// Gets a reference for the given data location.
    #[inline]
    pub fn get_ref(&self, loc: DataLocation) -> &T {
        match loc {
            DataLocation::Storage => &self.storage,
            DataLocation::Transient => &self.transient,
            DataLocation::Memory => &self.memory,
            DataLocation::Calldata => &self.calldata,
        }
    }

    /// Gets a mutable reference for the given data location.
    #[inline]
    pub fn get_mut(&mut self, loc: DataLocation) -> &mut T {
        match loc {
            DataLocation::Storage => &mut self.storage,
            DataLocation::Transient => &mut self.transient,
            DataLocation::Memory => &mut self.memory,
            DataLocation::Calldata => &mut self.calldata,
        }
    }
}

impl<T> std::ops::Index<DataLocation> for EachDataLoc<T> {
    type Output = T;

    #[inline]
    fn index(&self, loc: DataLocation) -> &Self::Output {
        self.get_ref(loc)
    }
}

impl<T> std::ops::IndexMut<DataLocation> for EachDataLoc<T> {
    #[inline]
    fn index_mut(&mut self, loc: DataLocation) -> &mut Self::Output {
        self.get_mut(loc)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ty<'gcx>(Interned<'gcx, TyData<'gcx>>);

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
        gcx.intern_ty(kind)
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

    pub fn as_externally_callable_function(self, gcx: Gcx<'gcx>) -> Self {
        let TyKind::FnPtr(f) = self.kind else { return self };
        let is_calldata = |param: &Ty<'_>| param.is_ref_at(DataLocation::Calldata);
        let any_parameter = f.parameters.iter().any(is_calldata);
        let any_return = f.returns.iter().any(is_calldata);
        if !any_parameter && !any_return {
            return self;
        }
        gcx.mk_ty_fn_ptr(TyFnPtr {
            parameters: if any_parameter {
                gcx.intern_ty_iter(f.parameters.iter().map(|param| {
                    if is_calldata(param) {
                        param.with_loc(gcx, DataLocation::Memory)
                    } else {
                        *param
                    }
                }))
            } else {
                f.parameters
            },
            returns: if any_return {
                gcx.intern_ty_iter(f.returns.iter().map(|ret| {
                    if is_calldata(ret) {
                        ret.with_loc(gcx, DataLocation::Memory)
                    } else {
                        *ret
                    }
                }))
            } else {
                f.returns
            },
            state_mutability: f.state_mutability,
            visibility: f.visibility,
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

    /// Returns `true` if the type is recursive.
    pub fn is_recursive(self) -> bool {
        self.flags.contains(TyFlags::IS_RECURSIVE)
    }

    /// Returns `true` if this type contains a mapping.
    pub fn has_mapping(self) -> bool {
        self.flags.contains(TyFlags::HAS_MAPPING)
    }

    /// Returns `true` if this type contains an error.
    pub fn has_error(self) -> bool {
        self.flags.contains(TyFlags::HAS_ERROR)
    }

    /// Returns `true` if this type can be part of an externally callable function.
    #[inline]
    pub fn can_be_exported(self) -> bool {
        !self.is_recursive() && !self.has_mapping()
    }
}

#[derive(PartialEq, Eq, Hash)]
pub struct TyData<'gcx> {
    pub kind: TyKind<'gcx>,
    pub flags: TyFlags,
}

impl<'gcx> Borrow<TyKind<'gcx>> for &TyData<'gcx> {
    fn borrow(&self) -> &TyKind<'gcx> {
        &self.kind
    }
}

impl fmt::Debug for TyData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
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
    Array(Ty<'gcx>, u64),

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
    fn calculate<'gcx>(gcx: Gcx<'gcx>, ty: &TyKind<'gcx>) -> Self {
        let mut flags = TyFlags::empty();
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
                self.add(TyFlags::HAS_MAPPING);
            }

            TyKind::Struct(id) => {
                if struct_is_recursive(gcx, id) {
                    self.add(TyFlags::IS_RECURSIVE);
                } else {
                    self.add_tys(gcx.struct_field_types(id));
                }
            }

            TyKind::Err(_) => self.add(TyFlags::HAS_ERROR),
        }
    }

    #[inline]
    fn add(&mut self, other: TyFlags) {
        *self |= other;
    }

    #[inline]
    fn add_ty(&mut self, ty: Ty<'_>) {
        self.add(ty.flags);
    }

    #[inline]
    fn add_tys(&mut self, tys: &[Ty<'_>]) {
        for ty in tys {
            self.add_ty(*ty);
        }
    }
}

fn struct_is_recursive(gcx: Gcx<'_>, id: hir::StructId) -> bool {
    let mut seen = FxHashSet::default();
    let mut ids = SmallVec::<[_; 16]>::new();
    seen.insert(id);
    ids.push(id);
    while let Some(id) = ids.pop() {
        for field in gcx.hir.strukt(id).fields {
            let r = field.ty.visit(&mut |ty| {
                if let hir::TypeKind::Custom(hir::ItemId::Struct(other_id)) = ty.kind {
                    if !seen.insert(other_id) {
                        return ControlFlow::Break(());
                    }
                    ids.push(other_id);
                }
                ControlFlow::Continue(())
            });
            if r.is_break() {
                return true;
            }
        }
    }
    false
}

trait DashMapExt<K> {
    fn intern_ref<Q>(&self, value: &Q, make: impl FnOnce() -> K) -> K
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq;

    fn intern<Q>(&self, value: Q, make: impl FnOnce(Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: Hash + Eq;
}

impl<K: Eq + Hash + Copy, S: BuildHasher + Clone> DashMapExt<K> for dashmap::DashMap<K, (), S> {
    fn intern_ref<Q>(&self, key: &Q, make: impl FnOnce() -> K) -> K
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        use dashmap::Map;

        let hash = self.hasher().hash_one(key);
        let shard = self.determine_shard(hash as usize);
        let mut shard = unsafe { self._yield_write_shard(shard) };

        let bucket = match shard.find_or_find_insert_slot(
            hash,
            |(k, _v)| k.borrow() == key,
            |(k, _v)| self.hasher().hash_one(k),
        ) {
            Ok(elem) => elem,
            Err(slot) => unsafe {
                shard.insert_in_slot(hash, slot, (make(), SharedValue::new(())))
            },
        };
        unsafe { bucket.as_ref() }.0
    }

    fn intern<Q>(&self, key: Q, make: impl FnOnce(Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        use dashmap::Map;

        let hash = self.hasher().hash_one(&key);
        let shard = self.determine_shard(hash as usize);
        let mut shard = unsafe { self._yield_write_shard(shard) };

        let bucket = match shard.find_or_find_insert_slot(
            hash,
            |(k, _v)| *k.borrow() == key,
            |(k, _v)| self.hasher().hash_one(k),
        ) {
            Ok(elem) => elem,
            Err(slot) => unsafe {
                shard.insert_in_slot(hash, slot, (make(key), SharedValue::new(())))
            },
        };
        unsafe { bucket.as_ref() }.0
    }
}

/// `OnceMap::insert` but with `Copy` keys and values.
fn once_map_insert<K, V, S>(
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

fn log_cache_query(name: &str, key: &dyn fmt::Debug, value: &dyn fmt::Debug, hit: bool) {
    trace!("`gcx.{name}` {kind}: {key:?} -> {value:?}", kind = if hit { " hit" } else { "miss" });
}
