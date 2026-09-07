use crate::backend::assembler::assembly::AsmIndex;
use std::marker::PhantomData;

#[derive(Clone, Debug)]
pub(in crate::backend) struct IdCounter<T> {
    next: usize,
    _index: PhantomData<fn() -> T>,
}

impl<T> IdCounter<T> {
    pub(in crate::backend) const fn new() -> Self {
        Self { next: 0, _index: PhantomData }
    }

    pub(in crate::backend) fn clear(&mut self) {
        self.next = 0;
    }
}

impl<T: AsmIndex> IdCounter<T> {
    pub(in crate::backend) fn next(&mut self) -> T {
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
