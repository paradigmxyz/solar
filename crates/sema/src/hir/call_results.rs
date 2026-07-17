//! Indexed abstract results produced by call expressions.

use super::{ExprId, JoinSemiLattice};
use solar_data_structures::{map::FxHashMap, smallvec::SmallVec};

/// Abstract call outputs keyed by expression and declaration-order result index.
///
/// An absent entry means the call has not been summarized. A present empty output list is a known
/// call with no results, so consumers can represent unknown values explicitly in `T` without
/// overloading collection emptiness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallResults<T> {
    outputs: FxHashMap<ExprId, SmallVec<[T; 2]>>,
}

impl<T> Default for CallResults<T> {
    fn default() -> Self {
        Self { outputs: FxHashMap::default() }
    }
}

impl<T> CallResults<T> {
    /// Returns all recorded outputs for `call`, including a known empty list.
    pub fn outputs(&self, call: ExprId) -> Option<&[T]> {
        self.outputs.get(&call).map(SmallVec::as_slice)
    }

    /// Returns output `index` of `call` when it has been summarized.
    pub fn output(&self, call: ExprId, index: usize) -> Option<&T> {
        self.outputs(call)?.get(index)
    }

    /// Replaces the outputs recorded for `call`.
    pub fn set_outputs(&mut self, call: ExprId, outputs: impl IntoIterator<Item = T>) {
        self.outputs.insert(call, outputs.into_iter().collect());
    }

    /// Removes the summary for `call` and returns whether one was present.
    pub fn clear(&mut self, call: ExprId) -> bool {
        self.outputs.remove(&call).is_some()
    }

    /// Removes every recorded call summary.
    pub fn clear_all(&mut self) {
        self.outputs.clear();
    }

    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.outputs.values_mut().flat_map(|outputs| outputs.iter_mut())
    }
}

impl<T> JoinSemiLattice for CallResults<T>
where
    T: Clone + Eq + JoinSemiLattice,
{
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (&call, outputs) in &other.outputs {
            let Some(current) = self.outputs.get_mut(&call) else {
                self.outputs.insert(call, outputs.clone());
                changed = true;
                continue;
            };
            for (index, output) in outputs.iter().enumerate() {
                if let Some(current) = current.get_mut(index) {
                    changed |= current.join(output);
                } else {
                    current.push(output.clone());
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_absent_empty_and_indexed_outputs() {
        let call = ExprId::new(0);
        let mut results = CallResults::<bool>::default();
        assert_eq!(results.outputs(call), None);
        results.set_outputs(call, []);
        assert_eq!(results.outputs(call), Some([].as_slice()));
        results.set_outputs(call, [false, true]);
        assert_eq!(results.output(call, 1), Some(&true));

        let mut other = CallResults::default();
        other.set_outputs(call, [true, false, true]);
        assert!(results.join(&other));
        assert_eq!(results.outputs(call), Some([true, true, true].as_slice()));
        assert!(results.clear(call));
        assert_eq!(results.outputs(call), None);
    }
}
