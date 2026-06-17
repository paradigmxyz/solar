use super::inst::AsmIndex;
use std::marker::PhantomData;

#[derive(Clone, Debug)]
pub(super) struct IdCounter<T> {
    next: usize,
    _index: PhantomData<fn() -> T>,
}

impl<T> IdCounter<T> {
    pub(super) const fn new() -> Self {
        Self { next: 0, _index: PhantomData }
    }

    pub(super) fn clear(&mut self) {
        self.next = 0;
    }
}

impl<T: AsmIndex> IdCounter<T> {
    pub(super) fn next(&mut self) -> T {
        let id = T::from_usize(self.next);
        id.inst_payload();
        self.next += 1;
        id
    }
}

impl<T> Default for IdCounter<T> {
    fn default() -> Self {
        Self::new()
    }
}
