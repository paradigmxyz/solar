use bumpalo::Bump;
use std::{
    cell::RefCell,
    ptr::{self, NonNull},
};

/// A bump arena that runs destructors for allocated values.
pub struct DropArena {
    bump: Bump,
    drops: RefCell<Vec<DropEntry>>,
}

// SAFETY: All values registered through the public allocation methods are `Send`.
unsafe impl Send for DropArena {}

struct DropEntry {
    ptr: NonNull<()>,
    len: usize,
    drop: unsafe fn(NonNull<()>, usize),
}

struct DropEntries(*mut DropArena);

impl Drop for DropEntries {
    fn drop(&mut self) {
        // SAFETY: The pointer comes from `DropArena::drop` and remains valid for this guard.
        unsafe { (*self.0).drop_entries() };
    }
}

impl DropArena {
    /// Creates an empty arena.
    #[inline]
    pub fn new() -> Self {
        Self { bump: Bump::new(), drops: RefCell::new(Vec::new()) }
    }

    /// Allocates a value in the arena.
    #[inline]
    pub fn alloc<T: Send>(&self, value: T) -> &mut T {
        let value = self.bump.alloc(value);
        self.register::<T>(NonNull::from(&mut *value).cast(), 1, drop_value::<T>);
        value
    }

    /// Allocates an iterator's values in the arena.
    #[inline]
    pub fn alloc_slice_fill_iter<T: Send, I>(&self, iter: I) -> &mut [T]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let values = self.bump.alloc_slice_fill_iter(iter);
        if let Some(ptr) = NonNull::new(values.as_mut_ptr()) {
            self.register::<T>(ptr.cast(), values.len(), drop_slice::<T>);
        }
        values
    }

    #[inline]
    fn register<T>(&self, ptr: NonNull<()>, len: usize, drop: unsafe fn(NonNull<()>, usize)) {
        if std::mem::needs_drop::<T>() && len != 0 {
            self.drops.borrow_mut().push(DropEntry { ptr, len, drop });
        }
    }

    unsafe fn drop_entries(&self) {
        loop {
            let Some(current) = self.drops.borrow_mut().pop() else { return };
            // SAFETY: `current` records the allocation's pointer, length, and destructor.
            unsafe { (current.drop)(current.ptr, current.len) };
        }
    }
}

impl Default for DropArena {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DropArena {
    fn drop(&mut self) {
        let _guard = DropEntries(self);
        // SAFETY: `drops` contains only entries added by `register`.
        unsafe { self.drop_entries() };
    }
}

unsafe fn drop_value<T>(ptr: NonNull<()>, _: usize) {
    // SAFETY: `ptr` points to a `T` allocated by `DropArena::alloc`.
    unsafe { ptr.cast::<T>().drop_in_place() };
}

unsafe fn drop_slice<T>(ptr: NonNull<()>, len: usize) {
    // SAFETY: `ptr` points to `len` contiguous `T`s allocated by
    // `DropArena::alloc_slice_fill_iter`.
    unsafe { ptr::drop_in_place(ptr::slice_from_raw_parts_mut(ptr.cast::<T>().as_ptr(), len)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{Arc, Mutex},
    };

    struct DropCounter(Arc<Mutex<Vec<usize>>>, usize);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.lock().unwrap().push(self.1);
        }
    }

    struct PanicOnDrop;

    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("boom");
        }
    }

    #[test]
    fn drops_allocated_values() {
        let dropped = Arc::new(Mutex::new(Vec::new()));
        {
            let arena = DropArena::new();
            arena.alloc(DropCounter(dropped.clone(), 1));
            arena.alloc_slice_fill_iter([2, 3].map(|i| DropCounter(dropped.clone(), i)));
        }
        assert_eq!(*dropped.lock().unwrap(), [2, 3, 1]);
    }

    #[test]
    fn drops_remaining_values_after_panic() {
        let dropped = Arc::new(Mutex::new(Vec::new()));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let arena = DropArena::new();
            arena.alloc(DropCounter(dropped.clone(), 1));
            arena.alloc(PanicOnDrop);
        }));
        assert!(result.is_err());
        assert_eq!(*dropped.lock().unwrap(), [1]);
    }
}
