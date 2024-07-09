//! Map types.
//!
//! - use [`IndexMap`] over [`HashMap`] for deterministic iteration order
//! - use [`AHasher`] for big keys and [`FxHasher`] for everything else
//!   - AHash docs make it look like it's faster than everything, but FxHash beats it for all
//!     primitives and small (<=16 bytes, if not more) strings and byte arrays

use indexmap::{IndexMap, IndexSet};
use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasherDefault,
};

pub use rustc_hash::{self, FxBuildHasher, FxHasher};

/// [`HashMap`] entry type.
pub type StdEntry<'a, K, V> = std::collections::hash_map::Entry<'a, K, V>;
/// [`IndexMap`] entry type.
pub type IndexEntry<'a, K, V> = indexmap::map::Entry<'a, K, V>;

/// A [`HashMap`] using [`FxHasher`] as its hasher.
pub type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;
/// A [`HashSet`] using [`FxHasher`] as its hasher.
pub type FxHashSet<V> = HashSet<V, BuildHasherDefault<FxHasher>>;
/// An [`IndexMap`] using [`FxHasher`] as its hasher.
pub type FxIndexMap<K, V> = IndexMap<K, V, BuildHasherDefault<FxHasher>>;
/// An [`IndexSet`] using [`FxHasher`] as its hasher.
pub type FxIndexSet<V> = IndexSet<V, BuildHasherDefault<FxHasher>>;
