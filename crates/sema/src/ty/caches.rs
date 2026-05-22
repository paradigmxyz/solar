use crate::{builtins::Builtin, hir, ty::Ty};
use solar_data_structures::{index::Idx, map::FxBuildHasher, sync::Mutex};
use solar_interface::Symbol;
use std::{hash::Hash, marker::PhantomData, sync::OnceLock};

type FxOnceMap<K, V> = once_map::OnceMap<K, V, FxBuildHasher>;

pub(super) type CacheFor<K, V> = <K as QueryKey>::Cache<V>;

pub(super) trait QueryKey: Copy {
    type Cache<V>: QueryCache<Self, V> + Default
    where
        V: Copy;
}

pub(super) trait QueryCache<K, V> {
    fn get_or_insert(&self, key: K, make_val: impl FnOnce(&K) -> V) -> V;
}

pub(super) struct DefaultCache<K, V> {
    cache: FxOnceMap<K, V>,
}

impl<K, V> Default for DefaultCache<K, V> {
    fn default() -> Self {
        Self { cache: Default::default() }
    }
}

impl<K, V> QueryCache<K, V> for DefaultCache<K, V>
where
    K: Copy + Eq + Hash,
    V: Copy,
{
    #[inline]
    fn get_or_insert(&self, key: K, make_val: impl FnOnce(&K) -> V) -> V {
        self.cache.map_insert(key, make_val, cache_insert_with_result)
    }
}

pub(super) trait VecCacheKey: Copy {
    fn index(self) -> usize;
}

impl<I: Idx> VecCacheKey for I {
    #[inline]
    fn index(self) -> usize {
        self.index()
    }
}

impl VecCacheKey for Builtin {
    #[inline]
    fn index(self) -> usize {
        self as usize
    }
}

pub(super) struct VecCache<K, V> {
    cache: Mutex<Vec<Box<OnceLock<V>>>>,
    key: PhantomData<fn(K) -> K>,
}

impl<K, V> Default for VecCache<K, V> {
    fn default() -> Self {
        Self { cache: Default::default(), key: PhantomData }
    }
}

impl<K, V> QueryCache<K, V> for VecCache<K, V>
where
    K: VecCacheKey,
    V: Copy,
{
    #[inline]
    fn get_or_insert(&self, key: K, make_val: impl FnOnce(&K) -> V) -> V {
        let slot = {
            let mut cache = self.cache.lock();
            let index = key.index();
            if index >= cache.len() {
                cache.resize_with(index + 1, || Box::new(OnceLock::new()));
            }
            &*cache[index] as *const OnceLock<V>
        };
        // SAFETY: Slots are boxed and never removed, so growing the vector cannot invalidate this
        // pointer after the lock is released.
        *unsafe { &*slot }.get_or_init(|| make_val(&key))
    }
}

pub(super) struct ItemIdCache<V> {
    contracts: VecCache<hir::ContractId, V>,
    functions: VecCache<hir::FunctionId, V>,
    variables: VecCache<hir::VariableId, V>,
    structs: VecCache<hir::StructId, V>,
    enums: VecCache<hir::EnumId, V>,
    udvts: VecCache<hir::UdvtId, V>,
    errors: VecCache<hir::ErrorId, V>,
    events: VecCache<hir::EventId, V>,
}

impl<V> Default for ItemIdCache<V> {
    fn default() -> Self {
        Self {
            contracts: Default::default(),
            functions: Default::default(),
            variables: Default::default(),
            structs: Default::default(),
            enums: Default::default(),
            udvts: Default::default(),
            errors: Default::default(),
            events: Default::default(),
        }
    }
}

impl<V> QueryCache<hir::ItemId, V> for ItemIdCache<V>
where
    V: Copy,
{
    #[inline]
    fn get_or_insert(&self, key: hir::ItemId, make_val: impl FnOnce(&hir::ItemId) -> V) -> V {
        match key {
            hir::ItemId::Contract(id) => self.contracts.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Function(id) => self.functions.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Variable(id) => self.variables.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Struct(id) => self.structs.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Enum(id) => self.enums.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Udvt(id) => self.udvts.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Error(id) => self.errors.get_or_insert(id, |_| make_val(&key)),
            hir::ItemId::Event(id) => self.events.get_or_insert(id, |_| make_val(&key)),
        }
    }
}

impl QueryKey for Builtin {
    type Cache<V>
        = VecCache<Self, V>
    where
        V: Copy;
}

impl QueryKey for hir::ItemId {
    type Cache<V>
        = ItemIdCache<V>
    where
        V: Copy;
}

macro_rules! vec_query_keys {
    ($($ty:ty),* $(,)?) => {
        $(
            impl QueryKey for $ty {
                type Cache<V> = VecCache<Self, V>
                where
                    V: Copy;
            }
        )*
    };
}

vec_query_keys! {
    hir::SourceId,
    hir::DocId,
    hir::ContractId,
    hir::FunctionId,
    hir::StructId,
    hir::EnumId,
    hir::UdvtId,
    hir::EventId,
    hir::ErrorId,
    hir::VariableId,
    hir::ExprId,
}

macro_rules! default_query_keys {
    ($($ty:ty),* $(,)?) => {
        $(
            impl QueryKey for $ty {
                type Cache<V> = DefaultCache<Self, V>
                where
                    V: Copy;
            }
        )*
    };
}

default_query_keys! { usize, (Symbol, hir::SourceId) }

impl<'gcx> QueryKey for Ty<'gcx> {
    type Cache<V>
        = DefaultCache<Self, V>
    where
        V: Copy;
}

/// Inserts into a query cache with `Copy` keys and values.
#[inline]
pub(super) fn cache_insert<K, V, C>(cache: &C, key: K, make_val: impl FnOnce(&K) -> V) -> V
where
    K: Copy,
    V: Copy,
    C: QueryCache<K, V>,
{
    cache.get_or_insert(key, make_val)
}

#[inline]
fn cache_insert_with_result<K, V: Copy>(_: &K, v: &V) -> V {
    *v
}

macro_rules! cached {
    ($($(#[$attr:meta])* $vis:vis fn $name:ident($gcx:ident: _, $key:ident : $key_type:ty) -> $value:ty $imp:block)*) => {
        #[derive(Default)]
        struct Cache<'gcx> {
            $(
                $name: $crate::ty::caches::CacheFor<$key_type, $value>,
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
                    let r = $crate::ty::caches::cache_insert(&self.cache.$name, $key, |&$key| {
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

pub(super) use cached;
