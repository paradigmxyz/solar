//! A map keyed by expression ids that merges in parallel.

use crate::hir::ExprId;
use rayon::prelude::*;
use solar_data_structures::map::FxHashMap;
use std::ops::Index;

const SHARDS: usize = 16;

/// The fewest entries for which merging in parallel pays.
const PAR_MERGE_LEN: usize = 4096;

/// A map keyed by expression ids, split into shards by id.
///
/// Maps built independently by different threads merge one shard per task, so merging does not
/// move every entry through a single thread.
#[derive(Clone, Debug)]
pub(crate) struct ExprMap<V> {
    shards: Box<[FxHashMap<ExprId, V>; SHARDS]>,
}

impl<V> Default for ExprMap<V> {
    fn default() -> Self {
        Self { shards: Box::new(std::array::from_fn(|_| FxHashMap::default())) }
    }
}

impl<V> ExprMap<V> {
    #[inline]
    fn shard(&self, id: &ExprId) -> &FxHashMap<ExprId, V> {
        &self.shards[id.index() % SHARDS]
    }

    #[inline]
    pub(crate) fn get(&self, id: &ExprId) -> Option<&V> {
        self.shard(id).get(id)
    }

    #[inline]
    pub(crate) fn contains_key(&self, id: &ExprId) -> bool {
        self.shard(id).contains_key(id)
    }

    fn len(&self) -> usize {
        self.shards.iter().map(FxHashMap::len).sum()
    }

    #[inline]
    pub(crate) fn insert(&mut self, id: ExprId, value: V) -> Option<V> {
        self.shards[id.index() % SHARDS].insert(id, value)
    }
}

impl<V: Copy + Send> ExprMap<V> {
    /// Merges `maps`, calling `duplicate` with the previous and the new value for every id that
    /// more than one map has.
    pub(crate) fn merge(mut maps: Vec<Self>, duplicate: impl Fn(ExprId, V, V) + Sync) -> Self {
        if maps.len() <= 1 {
            return maps.pop().unwrap_or_default();
        }
        let len = maps.iter().map(Self::len).sum::<usize>();
        let mut shards = (0..SHARDS).map(|_| Vec::with_capacity(maps.len())).collect::<Vec<_>>();
        for map in maps {
            for (shard, map) in shards.iter_mut().zip(*map.shards) {
                shard.push(map);
            }
        }
        let merge_shard = |mut maps: Vec<FxHashMap<ExprId, V>>| {
            let Some(largest) = (0..maps.len()).max_by_key(|&i| maps[i].len()) else {
                return FxHashMap::default();
            };
            let mut merged = maps.swap_remove(largest);
            for map in maps {
                merged.reserve(map.len());
                for (id, value) in map {
                    if let Some(prev) = merged.insert(id, value) {
                        duplicate(id, prev, value);
                    }
                }
            }
            merged
        };
        // Small maps merge faster than the pool wakes up.
        let merged = if len < PAR_MERGE_LEN {
            shards.into_iter().map(merge_shard).collect::<Vec<_>>()
        } else {
            shards.into_par_iter().map(merge_shard).collect::<Vec<_>>()
        };
        Self { shards: merged.into_boxed_slice().try_into().unwrap_or_else(|_| unreachable!()) }
    }
}

impl<V> Index<&ExprId> for ExprMap<V> {
    type Output = V;

    #[inline]
    fn index(&self, id: &ExprId) -> &V {
        &self.shard(id)[id]
    }
}
