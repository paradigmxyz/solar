//! Map types.

use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};

pub use rustc_hash::{self, FxBuildHasher, FxHasher};

/// [`HashMap`] entry type.
pub type StdEntry<'a, K, V> = std::collections::hash_map::Entry<'a, K, V>;
/// [`IndexMap`] entry type.
pub type IndexEntry<'a, K, V> = indexmap::map::Entry<'a, K, V>;
pub type IndexSetSlice<V> = indexmap::set::Slice<V>;

/// A [`HashMap`] using [`FxHasher`] as its hasher.
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
/// A [`HashSet`] using [`FxHasher`] as its hasher.
pub type FxHashSet<V> = HashSet<V, FxBuildHasher>;
/// An [`IndexMap`] using [`FxHasher`] as its hasher.
pub type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;
/// An [`IndexSet`] using [`FxHasher`] as its hasher.
pub type FxIndexSet<V> = IndexSet<V, FxBuildHasher>;
