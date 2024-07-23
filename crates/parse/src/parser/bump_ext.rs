use bumpalo::Bump;
use smallvec::SmallVec;

/// Extension trait for [`Bump`].
#[allow(clippy::mut_from_ref)] // Arena.
pub trait BumpExt {
    /// Allocates an iterator by first collecting it into a (possibly stack-allocated) vector.
    ///
    /// Prefer using [`Bump::alloc_slice_fill_iter`] if `iter` is [`ExactSizeIterator`] to avoid
    /// the intermediate allocation.
    fn alloc_iter_collect<T>(&self, iter: impl Iterator<Item = T>) -> &mut [T];

    /// Allocates a vector of items on the arena.
    ///
    /// NOTE: This method does not drop the values, so you likely want to wrap the result in a
    /// [`bumpalo::boxed::Box`] if `T: !Copy`.
    fn alloc_vec<T>(&self, values: Vec<T>) -> &mut [T];

    /// Allocates a `SmallVector` of items on the arena.
    ///
    /// NOTE: This method does not drop the values, so you likely want to wrap the result in a
    /// [`bumpalo::boxed::Box`] if `T: !Copy`.
    fn alloc_smallvec<A: smallvec::Array>(&self, values: SmallVec<A>) -> &mut [A::Item];

    /// Allocates a slice of items on the arena and copies them in.
    ///
    /// # Safety
    ///
    /// If `T: !Copy`, the resulting slice must not be wrapped in `Box`, unless ownership is
    /// moved as well, such as through [`alloc_vec`](Self::alloc_vec) and the other methods in this
    /// trait.
    unsafe fn alloc_slice_unchecked<'a, T>(&'a self, slice: &[T]) -> &'a mut [T];
}

impl BumpExt for Bump {
    #[inline]
    fn alloc_iter_collect<T>(&self, iter: impl Iterator<Item = T>) -> &mut [T] {
        self.alloc_smallvec(SmallVec::<[T; 8]>::from_iter(iter))
    }

    #[inline]
    fn alloc_vec<T>(&self, mut values: Vec<T>) -> &mut [T] {
        if values.is_empty() {
            return &mut [];
        }

        // SAFETY: The `Vec` is deallocated, but the elements are not dropped.
        unsafe {
            let r = self.alloc_slice_unchecked(values.as_slice());
            values.set_len(0);
            r
        }
    }

    #[inline]
    fn alloc_smallvec<A: smallvec::Array>(&self, mut values: SmallVec<A>) -> &mut [A::Item] {
        if values.is_empty() {
            return &mut [];
        }

        // SAFETY: See `alloc_vec`.
        unsafe {
            let r = self.alloc_slice_unchecked(values.as_slice());
            values.set_len(0);
            r
        }
    }

    #[inline]
    unsafe fn alloc_slice_unchecked<'a, T>(&'a self, slice: &[T]) -> &'a mut [T] {
        if slice.is_empty() {
            return &mut [];
        }

        let start_ptr =
            self.alloc_layout(std::alloc::Layout::for_value(slice)).as_ptr().cast::<T>();
        let len = slice.len();
        unsafe {
            slice.as_ptr().copy_to_nonoverlapping(start_ptr, len);
            std::slice::from_raw_parts_mut(start_ptr, len)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct DropBomb(i32, bool);
    impl DropBomb {
        fn new(i: i32) -> Self {
            Self(i, true)
        }
        fn defuse(&mut self) {
            self.1 = false;
        }
    }
    impl Drop for DropBomb {
        fn drop(&mut self) {
            if self.1 && !std::thread::panicking() {
                panic!("boom");
            }
        }
    }

    #[test]
    fn test_alloc_vec() {
        let bump = Bump::new();
        let vec = vec![DropBomb::new(1), DropBomb::new(2), DropBomb::new(3)];
        let other_vec = vec![DropBomb::new(1), DropBomb::new(2), DropBomb::new(3)];
        let slice = bump.alloc_vec(vec);
        assert_eq!(slice, &other_vec[..]);
        for item in slice {
            item.defuse();
        }
        for mut item in other_vec {
            item.defuse();
        }
    }
}
