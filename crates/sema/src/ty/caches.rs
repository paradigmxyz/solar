use crate::{builtins::Builtin, hir, ty::Ty};
use solar_data_structures::{index::Idx, map::FxBuildHasher};
use solar_interface::Symbol;
use std::{hash::Hash, marker::PhantomData, sync::OnceLock};

type FxOnceMap<K, V> = once_map::OnceMap<K, V, FxBuildHasher>;

pub(super) type CacheFor<K, V> = <K as QueryKey>::Cache<V>;

pub(super) trait QueryKey: Copy {
    type Cache<V>: QueryCache<Self, V> + Default
    where
        V: Copy;

    fn prefill_cache<V>(_: &Self::Cache<V>, _: &hir::Hir<'_>)
    where
        V: Copy,
    {
    }
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

pub(super) struct VecCache<K, V> {
    cache: boxcar::Vec<OnceLock<V>>,
    key: PhantomData<fn(K) -> K>,
}

impl<K, V> Default for VecCache<K, V> {
    fn default() -> Self {
        Self { cache: Default::default(), key: PhantomData }
    }
}

impl<K, V> VecCache<K, V> {
    fn prefill(&self, len: usize) {
        while self.cache.count() < len {
            self.cache.push(OnceLock::new());
        }
    }
}

impl<K, V> QueryCache<K, V> for VecCache<K, V>
where
    K: Idx,
    V: Copy,
{
    #[inline]
    fn get_or_insert(&self, key: K, make_val: impl FnOnce(&K) -> V) -> V {
        let index = key.index();
        let slot = loop {
            if let Some(slot) = self.cache.get(index) {
                break slot;
            }
            self.cache.push(OnceLock::new());
        };
        *slot.get_or_init(|| make_val(&key))
    }
}

impl QueryKey for Builtin {
    type Cache<V>
        = VecCache<Self, V>
    where
        V: Copy;

    fn prefill_cache<V>(cache: &Self::Cache<V>, _: &hir::Hir<'_>)
    where
        V: Copy,
    {
        cache.prefill(Self::COUNT);
    }
}

macro_rules! vec_query_keys {
    ($($ty:ty => $len:expr;)*) => {
        $(
            impl QueryKey for $ty {
                type Cache<V> = VecCache<Self, V>
                where
                    V: Copy;

                fn prefill_cache<V>(cache: &Self::Cache<V>, hir: &hir::Hir<'_>)
                where
                    V: Copy,
                {
                    cache.prefill($len(hir));
                }
            }
        )*
    };
}

vec_query_keys! {
    hir::SourceId => |hir: &hir::Hir<'_>| hir.sources.len();
    hir::DocId => |hir: &hir::Hir<'_>| hir.docs.len();
    hir::ContractId => |hir: &hir::Hir<'_>| hir.contracts.len();
    hir::FunctionId => |hir: &hir::Hir<'_>| hir.functions.len();
    hir::StructId => |hir: &hir::Hir<'_>| hir.structs.len();
    hir::EnumId => |hir: &hir::Hir<'_>| hir.enums.len();
    hir::UdvtId => |hir: &hir::Hir<'_>| hir.udvts.len();
    hir::EventId => |hir: &hir::Hir<'_>| hir.events.len();
    hir::ErrorId => |hir: &hir::Hir<'_>| hir.errors.len();
    hir::VariableId => |hir: &hir::Hir<'_>| hir.variables.len();
    hir::ExprId => |hir: &hir::Hir<'_>| hir.expr_count;
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

default_query_keys! { usize, (Symbol, hir::SourceId), hir::ItemId }

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

        impl<'gcx> Cache<'gcx> {
            fn prefill(&self, hir: &$crate::hir::Hir<'_>) {
                $(
                    <$key_type as $crate::ty::caches::QueryKey>::prefill_cache(&self.$name, hir);
                )*
            }
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

impl<'gcx> super::Gcx<'gcx> {
    pub(crate) fn init_caches(self) {
        self.cache.prefill(&self.hir);
    }
}
