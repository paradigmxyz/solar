use crate::backend::evm::ir::assembly::AsmIndex;
use solar_data_structures::map::FxIndexSet;
use std::{hash::Hash, marker::PhantomData};

#[derive(Clone, Debug)]
pub(in crate::backend::evm) struct LocalInterner<T, I> {
    values: FxIndexSet<T>,
    _index: PhantomData<fn() -> I>,
}

impl<T, I> LocalInterner<T, I> {
    pub(in crate::backend::evm) fn new() -> Self {
        Self { values: FxIndexSet::default(), _index: PhantomData }
    }

    pub(in crate::backend::evm) fn clear(&mut self) {
        self.values.clear();
    }
}

impl<T, I> LocalInterner<T, I>
where
    T: Eq + Hash,
    I: AsmIndex,
{
    pub(in crate::backend::evm) fn intern(&mut self, value: T) -> I {
        let (index, _) = self.values.insert_full(value);
        let index = I::from_usize(index);
        index.inst_payload();
        index
    }

    pub(in crate::backend::evm) fn get(&self, index: I) -> &T {
        self.values.get_index(index.index()).expect("local interner index out of bounds")
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.values.len()
    }
}

impl<T, I> Default for LocalInterner<T, I> {
    fn default() -> Self {
        Self::new()
    }
}
