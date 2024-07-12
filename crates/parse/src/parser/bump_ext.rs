use bumpalo::Bump;
use smallvec::SmallVec;
use std::mem::ManuallyDrop;

/// Extension trait for [`Bump`].
#[allow(dead_code)]
#[allow(clippy::mut_from_ref)] // Arena.
pub(crate) trait BumpExt {
    /// Allocates a vector of items on the arena.
    fn alloc_vec<T>(&self, values: Vec<T>) -> &mut [T];

    /// Allocates an array of items on the arena.
    fn alloc_array<T, const N: usize>(&self, values: [T; N]) -> &mut [T; N];

    /// Allocates a `SmallVector` of items on the arena.
    fn alloc_smallvec<T, const N: usize>(&self, values: SmallVec<[T; N]>) -> &mut [T; N]
    where
        [T; N]: smallvec::Array<Item = T>;

    unsafe fn alloc_slice_unchecked<T>(&self, slice: &[T]) -> &mut [T];
}

impl BumpExt for Bump {
    fn alloc_vec<T>(&self, values: Vec<T>) -> &mut [T] {
        // SAFETY:
        // - `T` and `ManuallyDrop<T>` have the same layout.
        // - We move the values into a new arena allocation, and deallocate the vector.
        unsafe {
            let values = std::mem::transmute::<Vec<T>, Vec<ManuallyDrop<T>>>(values);
            let slice = std::mem::transmute::<&[ManuallyDrop<T>], &[T]>(values.as_slice());
            self.alloc_slice_unchecked(slice)
        }
    }

    fn alloc_array<T, const N: usize>(&self, values: [T; N]) -> &mut [T; N] {
        let values = ManuallyDrop::new(values);
        // SAFETY:
        // - `T` and `ManuallyDrop<T>` have the same layout.
        unsafe { self.alloc_slice_unchecked(values.as_slice()).try_into().unwrap() }
    }

    fn alloc_smallvec<T, const N: usize>(&self, values: SmallVec<[T; N]>) -> &mut [T; N]
    where
        [T; N]: smallvec::Array<Item = T>,
    {
        match values.into_inner() {
            Ok(array) => self.alloc_array(array),
            Err(vec) => self.alloc_vec(vec.into_vec()).try_into().unwrap(),
        }
    }

    unsafe fn alloc_slice_unchecked<'a, T>(&'a self, slice: &[T]) -> &'a mut [T] {
        let src = slice.as_ptr();
        let data = self.alloc_layout(std::alloc::Layout::for_value(slice)).as_ptr().cast::<T>();
        let len = slice.len();
        unsafe {
            std::ptr::copy_nonoverlapping(src, data, len);
            std::slice::from_raw_parts_mut(data, len)
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
