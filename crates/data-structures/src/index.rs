//! Index types. See [`::oxc_index`].

pub use oxc_index::{
    Idx, IdxRangeBounds, IdxSliceIndex, IndexBox, IndexSlice, IndexVec, index_box, index_vec,
    nonmax::NonMaxU32,
};

/// Creates a new index type backed by `NonMaxU32`, with niche optimization for `Option<T>`.
#[macro_export]
macro_rules! newtype_index {
    ($($(#[$attr:meta])* $vis:vis struct $name:ident;)*) => {$(
        $crate::index::define_nonmax_u32_index_type! {
            $(#[$attr])*
            $vis struct $name;
        }

        impl $name {
            /// The maximum index value.
            $vis const MAX: Self = Self::new(Self::MAX_INDEX);
        }

        impl $crate::bit_set::BitSetIndex for $name {
            #[inline]
            fn from_usize(index: usize) -> Self {
                <Self as $crate::index::Idx>::from_usize(index)
            }

            #[inline]
            fn index(self) -> usize {
                <Self as $crate::index::Idx>::index(self)
            }
        }
    )*};
}

pub use oxc_index::define_nonmax_u32_index_type;
