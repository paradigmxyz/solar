#![allow(dead_code)]

use crate::hir;
use dashmap::SharedValue;
use std::{
    borrow::Borrow,
    fmt,
    hash::{BuildHasher, Hash},
};
use sulk_ast::ast::{DataLocation, ElementaryType, StateMutability, TypeSize, Visibility};
use sulk_data_structures::Interned;
use sulk_interface::{diagnostics::ErrorGuaranteed, Symbol};
use thread_local::ThreadLocal;

type FxDashSet<T> = dashmap::DashMap<T, (), sulk_data_structures::map::FxBuildHasher>;

/// Reference to the [global context](GlobalCtxt).
pub struct Gcx<'gcx>(pub(crate) &'gcx GlobalCtxt<'gcx>);

impl<'gcx> std::ops::Deref for Gcx<'gcx> {
    type Target = &'gcx GlobalCtxt<'gcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The global compilation context.
pub struct GlobalCtxt<'gcx> {
    pub interner: Interner<'gcx>,
    pub types: CommonTypes<'gcx>,
}

impl<'gcx> Gcx<'gcx> {
    pub fn mk_signature(self, name: Symbol, tys: impl IntoIterator<Item = Ty<'gcx>>) -> String {
        let mut s = String::with_capacity(64);
        s.push_str(name.as_str());
        TyPrinter::new(self, &mut s).print_tuple(tys).unwrap();
        s
    }
}

struct TyPrinter<'gcx, W> {
    #[allow(dead_code)]
    gcx: Gcx<'gcx>,
    buf: W,
}

impl<'gcx, W: fmt::Write> TyPrinter<'gcx, W> {
    fn new(gcx: Gcx<'gcx>, buf: W) -> Self {
        Self { gcx, buf }
    }

    fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        use TyKind::*;
        match ty.kind {
            Elementary(ty) => write!(self.buf, "{}", ty.to_abi_str()),
            Ref(ty, _loc) => self.print(ty),
            DynArray(ty) => {
                self.print(ty)?;
                write!(self.buf, "[]")
            }
            Array(ty, len) => {
                self.print(ty)?;
                write!(self.buf, "[{len}]")
            }
            Tuple(tys) => self.print_tuple(tys.iter().copied()),
            _ => panic!("printing invalid type: {ty:?}"),
        }
    }

    fn print_tuple(&mut self, tys: impl IntoIterator<Item = Ty<'gcx>>) -> fmt::Result {
        write!(self.buf, "(")?;
        for (i, ty) in tys.into_iter().enumerate() {
            if i > 0 {
                write!(self.buf, ", ")?;
            }
            self.print(ty)?;
        }
        write!(self.buf, ")")
    }
}

pub struct Interner<'gcx> {
    arena: &'gcx ThreadLocal<hir::Arena>,
    tys: FxDashSet<&'gcx TyData<'gcx>>,
    ty_lists: FxDashSet<&'gcx [Ty<'gcx>]>,
    fn_ptrs: FxDashSet<&'gcx TyFnPtr<'gcx>>,
}

impl<'gcx> Interner<'gcx> {
    pub fn new(arena: &'gcx ThreadLocal<hir::Arena>) -> Self {
        Self {
            arena,
            tys: Default::default(),
            ty_lists: Default::default(),
            fn_ptrs: Default::default(),
        }
    }

    pub fn intern_ty(&self, kind: TyKind<'gcx>) -> Ty<'gcx> {
        let key = TyData { kind };
        Ty(Interned::new_unchecked(
            self.tys.intern(key, |key| self.arena.get_or_default().bump.alloc(key)),
        ))
    }

    pub fn intern_tys(&self, tys: &[Ty<'gcx>]) -> &'gcx [Ty<'gcx>] {
        if tys.is_empty() {
            return &[];
        }
        self.ty_lists.intern_ref(tys, || self.arena.get_or_default().bump.alloc_slice_copy(tys))
    }

    pub fn intern_ty_iter(&self, tys: impl Iterator<Item = Ty<'gcx>>) -> &'gcx [Ty<'gcx>] {
        sulk_data_structures::CollectAndApply::collect_and_apply(tys, |tys| self.intern_tys(tys))
    }

    pub fn intern_fn_ptr(&self, ptr: TyFnPtr<'gcx>) -> &'gcx TyFnPtr<'gcx> {
        self.fn_ptrs.intern(ptr, |ptr| self.arena.get_or_default().bump.alloc(ptr))
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
    pub(crate) fn new(interner: &Interner<'gcx>) -> Self {
        use std::array::from_fn;
        use ElementaryType::*;
        use TyKind::*;

        let mk = |kind| interner.intern_ty(kind);
        let mk_refs = |kind| EachDataLoc {
            storage: mk(Ref(kind, DataLocation::Storage)),
            transient: mk(Ref(kind, DataLocation::Transient)),
            memory: mk(Ref(kind, DataLocation::Memory)),
            calldata: mk(Ref(kind, DataLocation::Calldata)),
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

    /// `int<size>`.
    pub fn int(&self, size: TypeSize) -> Ty<'gcx> {
        self.ints[size.bytes() as usize - 1]
    }

    /// `uint<size>`.
    pub fn uint(&self, size: TypeSize) -> Ty<'gcx> {
        self.uints[size.bytes() as usize - 1]
    }

    /// `bytes<size>`.
    pub fn fixed_bytes(&self, size: TypeSize) -> Ty<'gcx> {
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
        f.write_str("Ty(")?;
        self.0.fmt(f)?;
        f.write_str(")")
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
    pub fn new(interner: &Interner<'gcx>, kind: TyKind<'gcx>) -> Self {
        interner.intern_ty(kind)
    }

    pub fn new_elementary(interner: &Interner<'gcx>, ty: ElementaryType) -> Self {
        Self::new(interner, TyKind::Elementary(ty))
    }

    pub fn new_string_literal(interner: &Interner<'gcx>, s: &[u8]) -> Self {
        Self::new(
            interner,
            TyKind::StringLiteral(
                core::str::from_utf8(s).is_ok(),
                TypeSize::new(s.len().min(32) as u8).unwrap(),
            ),
        )
    }

    pub fn new_int_literal(interner: &Interner<'gcx>, size: TypeSize) -> Self {
        Self::new(interner, TyKind::IntLiteral(size))
    }

    pub fn new_ref(interner: &Interner<'gcx>, ty: Self, loc: DataLocation) -> Self {
        Self::new(interner, TyKind::Ref(ty, loc))
    }

    pub fn new_dyn_array(interner: &Interner<'gcx>, ty: Self) -> Self {
        Self::new(interner, TyKind::DynArray(ty))
    }

    pub fn new_array(interner: &Interner<'gcx>, ty: Self, len: u64) -> Self {
        Self::new(interner, TyKind::Array(ty, len))
    }

    pub fn new_tuple(interner: &Interner<'gcx>, tys: &'gcx [Self]) -> Self {
        Self::new(interner, TyKind::Tuple(tys))
    }

    pub fn new_mapping(interner: &Interner<'gcx>, key: Self, value: Self) -> Self {
        Self::new(interner, TyKind::Mapping(key, value))
    }

    pub fn new_fn_ptr(interner: &Interner<'gcx>, ptr: &'gcx TyFnPtr<'gcx>) -> Self {
        Self::new(interner, TyKind::FnPtr(ptr))
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TyData<'gcx> {
    pub kind: TyKind<'gcx>,
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

    /// An invalid type. Silences further errors.
    Err(ErrorGuaranteed),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TyFnPtr<'gcx> {
    pub parameters: &'gcx [Ty<'gcx>],
    pub returns: &'gcx [Ty<'gcx>],
    pub mutability: StateMutability,
    pub visibility: Visibility,
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
