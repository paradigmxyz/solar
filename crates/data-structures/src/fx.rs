//! Stolen from [rustc_data_structures](https://github.com/rust-lang/rust/blob/661b33f5247debc4e0cd948caa388997e18e9cb8/compiler/rustc_data_structures/src/fx.rs).

use std::hash::BuildHasherDefault;

pub use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

pub type StdEntry<'a, K, V> = std::collections::hash_map::Entry<'a, K, V>;

pub type FxIndexMap<K, V> = indexmap::IndexMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FxIndexSet<V> = indexmap::IndexSet<V, BuildHasherDefault<FxHasher>>;
pub type IndexEntry<'a, K, V> = indexmap::map::Entry<'a, K, V>;
