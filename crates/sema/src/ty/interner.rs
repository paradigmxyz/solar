//! Type interner.
//!
//! Creates and stores unique instances of types, type lists, and function pointers.

use super::{Ty, TyData, TyFlags, TyFnPtr, TyKind};
use crate::hir::{self};
use solar_data_structures::{map::FxBuildHasher, Interned};
use std::{
    borrow::Borrow,
    hash::{BuildHasher, Hash},
};
use thread_local::ThreadLocal;

type InternSet<T> = once_map::OnceMap<T, (), FxBuildHasher>;

pub(super) struct Interner<'gcx> {
    pub(super) arena: &'gcx ThreadLocal<hir::Arena>,

    pub(super) tys: InternSet<&'gcx TyData<'gcx>>,
    pub(super) ty_lists: InternSet<&'gcx [Ty<'gcx>]>,
    pub(super) fn_ptrs: InternSet<&'gcx TyFnPtr<'gcx>>,
}

impl<'gcx> Interner<'gcx> {
    pub(super) fn new(arena: &'gcx ThreadLocal<hir::Arena>) -> Self {
        Self {
            arena,
            tys: Default::default(),
            ty_lists: Default::default(),
            fn_ptrs: Default::default(),
        }
    }

    fn bump(&self) -> &'gcx bumpalo::Bump {
        &self.arena.get_or_default().bump
    }

    pub(super) fn intern_ty_with_flags(
        &self,
        kind: TyKind<'gcx>,
        mk_flags: impl FnOnce(&TyKind<'gcx>) -> TyFlags,
    ) -> Ty<'gcx> {
        Ty(Interned::new_unchecked(
            self.tys
                .intern(kind, |kind| self.bump().alloc(TyData { flags: mk_flags(&kind), kind })),
        ))
    }

    pub(super) fn intern_tys(&self, tys: &[Ty<'gcx>]) -> &'gcx [Ty<'gcx>] {
        if tys.is_empty() {
            return &[];
        }
        self.ty_lists.intern_ref(tys, |tys| self.bump().alloc_slice_copy(tys))
    }

    pub(super) fn intern_ty_iter(&self, tys: impl Iterator<Item = Ty<'gcx>>) -> &'gcx [Ty<'gcx>] {
        solar_data_structures::CollectAndApply::collect_and_apply(tys, |tys| self.intern_tys(tys))
    }

    pub(super) fn intern_ty_fn_ptr(&self, ptr: TyFnPtr<'gcx>) -> &'gcx TyFnPtr<'gcx> {
        self.fn_ptrs.intern(ptr, |ptr| self.bump().alloc(ptr))
    }
}

trait Intern<K> {
    fn intern<Q>(&self, key: Q, make: impl FnOnce(Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: Hash + Eq;

    fn intern_ref<Q>(&self, key: &Q, make: impl FnOnce(&Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq;
}

/*
use dashmap::{Map, SharedValue};
impl<K: Eq + Hash + Copy, S: BuildHasher + Clone> Intern<K> for dashmap::DashMap<K, (), S> {
    fn intern<Q>(&self, key: Q, make: impl FnOnce(Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
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

    fn intern_ref<Q>(&self, key: &Q, make: impl FnOnce(&Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
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
                shard.insert_in_slot(hash, slot, (make(key), SharedValue::new(())))
            },
        };
        unsafe { bucket.as_ref() }.0
    }
}
*/

/*
impl<K: Eq + Hash + Copy, S: BuildHasher + Clone> Intern<K> for scc::HashMap<K, (), S> {
    fn intern<Q>(&self, key: Q, make: impl FnOnce(Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        if let Some(key) = self.read(&key, intern_reader) {
            return key;
        }
        *self.entry(make(key)).or_insert(()).key()
    }

    fn intern_ref<Q>(&self, key: &Q, make: impl FnOnce(&Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(key) = self.read(key, intern_reader) {
            return key;
        }
        *self.entry(make(key)).or_insert(()).key()
    }
}

#[inline]
fn intern_reader<K: Copy, V>(k: &K, _: &V) -> K {
    *k
}
*/

impl<K: Eq + Hash + Copy, S: BuildHasher> Intern<K> for once_map::OnceMap<K, (), S> {
    #[inline]
    fn intern<Q>(&self, key: Q, make: impl FnOnce(Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        const { assert!(!std::mem::needs_drop::<Q>()) }
        self.map_insert_ref(&key, |key| make(unsafe { std::ptr::read(key) }), make_val, with_result)
    }

    #[inline]
    fn intern_ref<Q>(&self, key: &Q, make: impl FnOnce(&Q) -> K) -> K
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map_insert_ref(key, make, make_val, with_result)
    }
}

#[inline]
fn make_val<K>(_: &K) {}

#[inline]
fn with_result<K: Copy, V>(k: &K, _: &V) -> K {
    *k
}
