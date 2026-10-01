//! Join-semilattices for dataflow domains.
//!
//! A domain value describes a set of concrete program states. [`JoinSemiLattice::join`]
//! computes an upper bound in place and reports whether the value grew, which is all a
//! worklist solver needs to detect convergence. Domains with infinite ascending chains
//! override [`JoinSemiLattice::widen`]; the default widening is the join itself, which is
//! correct for every finite-height domain.
//!
//! The combinators here compose client domains without new lattice code: [`MapLattice`] is
//! the pointwise lift of a value lattice, [`Reachable`] adds an explicit "no path" bottom,
//! and tuples form products. Bitsets use union, which makes them the natural representation
//! of may facts over dense index domains.

use solar_data_structures::{
    bit_set::{BitSetIndex, DenseBitSet},
    map::FxHashMap,
};
use std::hash::Hash;

/// A join-semilattice whose values can be merged in place.
pub(crate) trait JoinSemiLattice: Clone {
    /// Sets `self` to the least upper bound of `self` and `other`.
    ///
    /// Returns whether `self` changed.
    fn join(&mut self, other: &Self) -> bool;

    /// Extrapolates the ascending chain from `self` towards `other`.
    ///
    /// The result must be an upper bound of both values, and repeated widening must reach a
    /// fixed point in finitely many steps. Returns whether `self` changed.
    fn widen(&mut self, other: &Self) -> bool {
        self.join(other)
    }
}

impl JoinSemiLattice for bool {
    fn join(&mut self, other: &Self) -> bool {
        let changed = !*self && *other;
        *self |= *other;
        changed
    }
}

impl JoinSemiLattice for () {
    fn join(&mut self, _other: &Self) -> bool {
        false
    }
}

impl<T: BitSetIndex> JoinSemiLattice for DenseBitSet<T> {
    fn join(&mut self, other: &Self) -> bool {
        self.union(other)
    }
}

impl<A: JoinSemiLattice, B: JoinSemiLattice> JoinSemiLattice for (A, B) {
    fn join(&mut self, other: &Self) -> bool {
        let a = self.0.join(&other.0);
        let b = self.1.join(&other.1);
        a | b
    }

    fn widen(&mut self, other: &Self) -> bool {
        let a = self.0.widen(&other.0);
        let b = self.1.widen(&other.1);
        a | b
    }
}

impl<A: JoinSemiLattice, B: JoinSemiLattice, C: JoinSemiLattice> JoinSemiLattice for (A, B, C) {
    fn join(&mut self, other: &Self) -> bool {
        let a = self.0.join(&other.0);
        let b = self.1.join(&other.1);
        let c = self.2.join(&other.2);
        a | b | c
    }

    fn widen(&mut self, other: &Self) -> bool {
        let a = self.0.widen(&other.0);
        let b = self.1.widen(&other.1);
        let c = self.2.widen(&other.2);
        a | b | c
    }
}

/// The pointwise lift of a value lattice; missing keys map to the value's bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MapLattice<K: Eq + Hash, V>(pub(crate) FxHashMap<K, V>);

impl<K: Eq + Hash, V> Default for MapLattice<K, V> {
    fn default() -> Self {
        Self(FxHashMap::default())
    }
}

impl<K: Clone + Eq + Hash, V: JoinSemiLattice> JoinSemiLattice for MapLattice<K, V> {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (key, value) in &other.0 {
            match self.0.get_mut(key) {
                Some(existing) => changed |= existing.join(value),
                None => {
                    self.0.insert(key.clone(), value.clone());
                    changed = true;
                }
            }
        }
        changed
    }

    fn widen(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (key, value) in &other.0 {
            match self.0.get_mut(key) {
                Some(existing) => changed |= existing.widen(value),
                None => {
                    self.0.insert(key.clone(), value.clone());
                    changed = true;
                }
            }
        }
        changed
    }
}

/// A value reachable only on some paths; `Unreachable` is bottom.
///
/// This lifts a domain whose own least element is not a useful "no path" marker, such as an
/// environment where missing keys mean "unknown" rather than "unreachable".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Reachable<T> {
    /// No path reaches this point.
    #[default]
    Unreachable,
    /// At least one path reaches this point with this state.
    State(T),
}

impl<T: JoinSemiLattice> JoinSemiLattice for Reachable<T> {
    fn join(&mut self, other: &Self) -> bool {
        match (&mut *self, other) {
            (_, Self::Unreachable) => false,
            (Self::Unreachable, Self::State(state)) => {
                *self = Self::State(state.clone());
                true
            }
            (Self::State(this), Self::State(other)) => this.join(other),
        }
    }

    fn widen(&mut self, other: &Self) -> bool {
        match (&mut *self, other) {
            (_, Self::Unreachable) => false,
            (Self::Unreachable, Self::State(state)) => {
                *self = Self::State(state.clone());
                true
            }
            (Self::State(this), Self::State(other)) => this.widen(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachable_bottom_adopts_first_state() {
        let mut state = Reachable::Unreachable;
        assert!(!state.join(&Reachable::Unreachable));
        assert!(state.join(&Reachable::State(false)));
        assert!(state.join(&Reachable::State(true)));
        assert_eq!(state, Reachable::State(true));
    }

    #[test]
    fn map_lattice_joins_pointwise() {
        let mut map = MapLattice::default();
        map.0.insert(1, false);
        let mut other = MapLattice::default();
        other.0.insert(1, true);
        other.0.insert(2, false);
        assert!(map.join(&other));
        assert!(map.0[&1]);
        assert!(!map.0[&2]);
        assert!(!map.join(&other));
    }
}
