//! A map keyed by expression ids, stored as a table indexed by id.

use crate::hir::ExprId;
use solar_data_structures::map::FxHashMap;
use std::ops::Index;

/// The fewest slots a window may grow to past its current length to take a new id.
const MIN_GROWTH: usize = 64;

/// A map keyed by expression ids.
///
/// Lowering numbers the expressions of an item consecutively, so the ids a checker records for
/// one item fall in a narrow range. The map stores that range as a window of slots indexed by id,
/// and only ids far outside it in a hash map. Merging the maps of every checker gives a single
/// window over every id, so lookups in the merged results are an index rather than a hash probe.
#[derive(Clone, Debug)]
pub(crate) struct ExprMap<V> {
    /// The id of the first slot of `window`.
    base: usize,
    /// One slot per id from `base`.
    window: Vec<Option<V>>,
    /// The entries whose ids are outside `window`.
    rest: FxHashMap<ExprId, V>,
}

impl<V> Default for ExprMap<V> {
    fn default() -> Self {
        Self { base: 0, window: Vec::new(), rest: FxHashMap::default() }
    }
}

impl<V: Copy> ExprMap<V> {
    #[inline]
    pub(crate) fn get(&self, id: &ExprId) -> Option<&V> {
        if let Some(Some(value)) = self.window.get(id.index().wrapping_sub(self.base)) {
            return Some(value);
        }
        if self.rest.is_empty() {
            return None;
        }
        self.rest.get(id)
    }

    #[inline]
    pub(crate) fn contains_key(&self, id: &ExprId) -> bool {
        self.get(id).is_some()
    }

    pub(crate) fn insert(&mut self, id: ExprId, value: V) -> Option<V> {
        if self.window.is_empty() {
            self.base = id.index();
        }
        let i = id.index().wrapping_sub(self.base);
        if i >= self.window.len() {
            if i >= (2 * self.window.len()).max(MIN_GROWTH) {
                return self.rest.insert(id, value);
            }
            self.window.resize(i + 1, None);
        }
        let prev = self.window[i].replace(value);
        // An id the window has grown over may have been recorded before it did.
        if prev.is_none() && !self.rest.is_empty() {
            return self.rest.remove(&id);
        }
        prev
    }

    /// Merges `maps` into one window over every id, calling `duplicate` with the previous and
    /// the new value for every id that more than one map has.
    pub(crate) fn merge(maps: Vec<Self>, duplicate: impl Fn(ExprId, V, V)) -> Self {
        let len = maps
            .iter()
            .map(|map| {
                let rest = map.rest.keys().map(|id| id.index() + 1).max().unwrap_or(0);
                rest.max(map.base + map.window.len())
            })
            .max()
            .unwrap_or(0);
        let mut window = vec![None; len];
        let mut record = |id: ExprId, value: V| {
            if let Some(prev) = window[id.index()].replace(value) {
                duplicate(id, prev, value);
            }
        };
        for map in maps {
            for (i, value) in map.window.into_iter().enumerate() {
                if let Some(value) = value {
                    record(ExprId::from_usize(map.base + i), value);
                }
            }
            for (id, value) in map.rest {
                record(id, value);
            }
        }
        Self { base: 0, window, rest: FxHashMap::default() }
    }
}

impl<V: Copy> Index<&ExprId> for ExprMap<V> {
    type Output = V;

    #[inline]
    fn index(&self, id: &ExprId) -> &V {
        self.get(id).expect("no entry for expression")
    }
}
