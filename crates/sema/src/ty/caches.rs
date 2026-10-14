//! Storage for the cached queries on [`Gcx`](super::Gcx).
//!
//! Each key type picks its cache through [`QueryKey`]: dense HIR indices and builtins use a slice
//! with one slot per key, `()` uses a single slot, and every other key uses a hash map.

use super::{FxOnceMap, Ty, UsingDirectiveKey};
use crate::{builtins::Builtin, hir, typeck::override_checker::OverrideProxy};
use solar_data_structures::index::{Idx, IndexSlice, IndexVec};
use solar_interface::Span;
use std::{hash::Hash, sync::OnceLock};

/// The cache of a query with key `K` and value `V`.
pub(super) type CacheFor<K, V> = <K as QueryKey>::Cache<V>;

/// The key of a cached query.
pub(super) trait QueryKey: Copy {
    /// The cache that stores the query's values.
    type Cache<V: Copy>: QueryCache<Self, V>;
}

/// The storage of a cached query.
pub(super) trait QueryCache<K, V>: Default {
    /// Returns the value for `key`, computing it with `make_val` if it is not cached.
    fn get_or_insert(&self, hir: &hir::Hir<'_>, key: K, make_val: impl FnOnce() -> V) -> V;
}

impl<K: Copy + Eq + Hash, V: Copy> QueryCache<K, V> for FxOnceMap<K, V> {
    #[inline]
    fn get_or_insert(&self, _: &hir::Hir<'_>, key: K, make_val: impl FnOnce() -> V) -> V {
        self.map_insert(key, |_| make_val(), |_, &v| v)
    }
}

impl<V: Copy> QueryCache<(), V> for OnceLock<V> {
    #[inline]
    fn get_or_insert(&self, _: &hir::Hir<'_>, (): (), make_val: impl FnOnce() -> V) -> V {
        *self.get_or_init(make_val)
    }
}

/// Caches values in a slice with one slot per key.
///
/// The slice is allocated on first use. Keys only exist once lowering has finished, so the HIR
/// already holds every key by then.
pub(super) struct DenseCache<K: Idx, V>(OnceLock<IndexVec<K, OnceLock<V>>>);

impl<K: Idx, V> Default for DenseCache<K, V> {
    fn default() -> Self {
        Self(OnceLock::new())
    }
}

impl<K: DenseKey, V: Copy> QueryCache<K, V> for DenseCache<K, V> {
    #[inline]
    fn get_or_insert(&self, hir: &hir::Hir<'_>, key: K, make_val: impl FnOnce() -> V) -> V {
        let slots = self.0.get_or_init(|| (0..K::count(hir)).map(|_| OnceLock::new()).collect());
        *slots[key].get_or_init(make_val)
    }
}

/// An index key whose number of values is known once the HIR is complete.
pub(super) trait DenseKey: Idx {
    /// Returns the number of keys.
    fn count(hir: &hir::Hir<'_>) -> usize;
}

impl QueryKey for Builtin {
    type Cache<V: Copy> = DenseCache<Self, V>;
}

impl DenseKey for Builtin {
    fn count(_: &hir::Hir<'_>) -> usize {
        Self::COUNT
    }
}

macro_rules! dense_keys {
    ($($ty:ty => $field:ident),* $(,)?) => {$(
        impl QueryKey for $ty {
            type Cache<V: Copy> = DenseCache<Self, V>;
        }

        impl DenseKey for $ty {
            fn count(hir: &hir::Hir<'_>) -> usize {
                IndexSlice::<Self, [_]>::len(&hir.$field)
            }
        }
    )*};
}

dense_keys! {
    hir::ContractId => contracts,
    hir::StructId => structs,
}

macro_rules! map_keys {
    ($($ty:ty),* $(,)?) => {$(
        impl QueryKey for $ty {
            type Cache<V: Copy> = FxOnceMap<Self, V>;
        }
    )*};
}

map_keys! {
    hir::ItemId,
    OverrideProxy,
    Span,
    Ty<'_>,
    UsingDirectiveKey,
}

impl<A: Copy + Eq + Hash, B: Copy + Eq + Hash> QueryKey for (A, B) {
    type Cache<V: Copy> = FxOnceMap<Self, V>;
}

impl<A: Copy + Eq + Hash, B: Copy + Eq + Hash, C: Copy + Eq + Hash> QueryKey for (A, B, C) {
    type Cache<V: Copy> = FxOnceMap<Self, V>;
}

impl QueryKey for () {
    type Cache<V: Copy> = OnceLock<V>;
}
