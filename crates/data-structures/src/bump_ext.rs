use bumpalo::Bump;
use smallvec::SmallVec;

/// Extension trait for [`Bump`].
#[allow(clippy::mut_from_ref)] // Arena.
pub trait BumpExt {
    /// Returns the number of bytes currently in use.
    fn used_bytes(&self) -> usize;

    /// Allocates a value as a slice of length 1.
    fn alloc_as_slice<T>(&self, value: T) -> &mut [T];

    /// Allocates an iterator by first collecting it into a (possibly stack-allocated) vector.
    ///
    /// Does not collect if the iterator is exact size, meaning `size_hint` returns equal values.
    fn alloc_from_iter<T>(&self, iter: impl Iterator<Item = T>) -> &mut [T];

    /// Allocates a vector of items on the arena.
    ///
    /// NOTE: This method does not drop the values, so you likely want to wrap the result in a
    /// [`bumpalo::boxed::Box`] if `T: Drop`.
    fn alloc_vec<T>(&self, values: Vec<T>) -> &mut [T];

    /// Allocates a `SmallVector` of items on the arena.
    ///
    /// NOTE: This method does not drop the values, so you likely want to wrap the result in a
    /// [`bumpalo::boxed::Box`] if `T: Drop`.
    fn alloc_smallvec<A: smallvec::Array>(&self, values: SmallVec<A>) -> &mut [A::Item];

    /// Allocates an array of items on the arena.
    ///
    /// NOTE: This method does not drop the values, so you likely want to wrap the result in a
    /// [`bumpalo::boxed::Box`] if `T: Drop`.
    fn alloc_array<T, const N: usize>(&self, values: [T; N]) -> &mut [T];

    /// Allocates a slice of items on the arena and copies them in.
    ///
    /// # Safety
    ///
    /// If `T: Drop`, the resulting slice must not be wrapped in [`bumpalo::boxed::Box`], unless
    /// ownership is moved as well, such as through [`alloc_vec`](Self::alloc_vec) and the other
    /// methods in this trait.
    unsafe fn alloc_slice_unchecked<'a, T>(&'a self, slice: &[T]) -> &'a mut [T];
}

impl BumpExt for Bump {
    fn used_bytes(&self) -> usize {
        // SAFETY: The data is not read, and the arena is not used during the iteration.
        unsafe { self.iter_allocated_chunks_raw().map(|(_ptr, len)| len).sum::<usize>() }
    }

    #[inline]
    fn alloc_as_slice<T>(&self, value: T) -> &mut [T] {
        std::slice::from_mut(self.alloc(value))
    }

    #[inline]
    fn alloc_from_iter<T>(&self, mut iter: impl Iterator<Item = T>) -> &mut [T] {
        match iter.size_hint() {
            (min, Some(max)) if min == max => self.alloc_slice_fill_with(min, |_| {
                iter.next().expect("Iterator supplied too few elements")
            }),
            _ => self.alloc_smallvec(SmallVec::<[T; 8]>::from_iter(iter)),
        }
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
    fn alloc_array<T, const N: usize>(&self, values: [T; N]) -> &mut [T] {
        if values.is_empty() {
            return &mut [];
        }

        let values = std::mem::ManuallyDrop::new(values);
        // SAFETY: See `alloc_vec`.
        unsafe { self.alloc_slice_unchecked(values.as_slice()) }
    }

    #[inline]
    unsafe fn alloc_slice_unchecked<'a, T>(&'a self, src: &[T]) -> &'a mut [T] {
        // Copied from `alloc_slice_copy`.
        let layout = std::alloc::Layout::for_value(src);
        let dst = self.alloc_layout(layout).cast::<T>();
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_ptr(), src.len());
            std::slice::from_raw_parts_mut(dst.as_ptr(), src.len())
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
