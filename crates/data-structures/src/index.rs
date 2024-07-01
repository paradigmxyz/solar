//! Index types. See [`::index_vec`].

use std::fmt;

pub use index_vec::{
    index_box, index_vec, Idx, IdxRangeBounds, IdxSliceIndex, IndexBox, IndexSlice, IndexVec,
};

/// Creates a new index to use with [`::index_vec`].
#[macro_export]
macro_rules! newtype_index {
    () => {};
    ($(#[$attr:meta])* $vis:vis struct $name:ident; $($rest:tt)*) => {
        $(#[$attr])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        $vis struct $name($crate::index::BaseIndex32);

        impl std::fmt::Display for $name {
            #[inline(always)]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::fmt::Debug for $name {
            #[inline(always)]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl $crate::index::Idx for $name {
            #[inline(always)]
            fn from_usize(value: usize) -> Self {
                Self(<$crate::index::BaseIndex32 as $crate::index::Idx>::from_usize(value))
            }

            #[inline(always)]
            fn index(self) -> usize {
                <$crate::index::BaseIndex32 as $crate::index::Idx>::index(self.0)
            }
        }

        impl $name {
            /// The maximum index value.
            $vis const MAX: Self = Self($crate::index::BaseIndex32::MAX);

            /// Creates a new `$name` from the given `value`.
            #[inline(always)]
            $vis const fn new(value: u32) -> Self {
                Self($crate::index::BaseIndex32::new(value))
            }

            /// Gets the underlying index value.
            #[inline(always)]
            $vis const fn get(self) -> u32 {
                self.0.get()
            }
        }

        $crate::newtype_index!($($rest)*);
    };
}

// NOTE: The max MUST be less than the maximum value of the underlying integer.
macro_rules! base_index {
    ($(#[$attr:meta])* $name:ident($primitive:ident <= $max:literal)) => {
        /// A specialized wrapper around a primitive number.
        ///
        $(#[$attr])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "nightly", rustc_layout_scalar_valid_range_end($max))]
        #[cfg_attr(feature = "nightly", rustc_nonnull_optimization_guaranteed)]
        #[cfg_attr(feature = "nightly", rustc_pass_by_value)]
        #[repr(transparent)]
        pub struct $name {
            #[cfg(feature = "nightly")]
            value: $primitive,
            #[cfg(not(feature = "nightly"))]
            value: std::num::NonZero<$primitive>,
        }

        impl fmt::Display for $name {
            #[inline(always)]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.value.fmt(f)
            }
        }

        impl fmt::Debug for $name {
            #[inline(always)]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.value.fmt(f)
            }
        }

        impl Idx for $name {
            #[inline(always)]
            fn from_usize(value: usize) -> Self {
                if value > Self::MAX_AS as usize {
                    index_overflow();
                }
                unsafe { Self::new_unchecked(value as $primitive) }
            }

            #[inline(always)]
            fn index(self) -> usize {
                self.get() as usize
            }
        }

        impl $name {
            /// The maximum index value, as the underlying primitive type.
            pub const MAX_AS: $primitive = $max;

            /// The maximum index value.
            pub const MAX: Self = Self::new(Self::MAX_AS);

            /// Creates a new `$name` from the given `value`.
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline(always)]
            pub const fn new(value: $primitive) -> Self {
                if value > Self::MAX_AS {
                    index_overflow();
                }
                unsafe { Self::new_unchecked(value) }
            }

            /// Creates a new `$name` from the given `value`, without checking for overflow.
            ///
            /// # Safety
            ///
            /// The caller must ensure that `value` is less than or equal to `MAX`.
            #[inline(always)]
            pub const unsafe fn new_unchecked(value: $primitive) -> Self {
                unsafe {
                    Self {
                        #[cfg(feature = "nightly")]
                        value,
                        #[cfg(not(feature = "nightly"))]
                        value: std::mem::transmute::<$primitive, std::num::NonZero<$primitive>>(value.unchecked_add(1)),
                    }
                }
            }

            /// Gets the underlying index value.
            #[inline(always)]
            pub const fn get(self) -> $primitive {
                #[cfg(feature = "nightly")]
                return self.value;

                #[cfg(not(feature = "nightly"))]
                return unsafe { self.value.get().unchecked_sub(1) };
            }
        }
    };
}

base_index!(BaseIndex32(u32 <= 0xFFFF_FF00));

#[inline(never)]
#[cold]
const fn index_overflow() -> ! {
    panic!("index overflowed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_index() {
        assert_eq!(BaseIndex32::new(0).get(), 0);
        assert_eq!(BaseIndex32::new(1).get(), 1);
        assert_eq!(BaseIndex32::MAX.get(), 0xFFFF_FF00);
        assert_eq!(BaseIndex32::new(0xFFFF_FF00).get(), 0xFFFF_FF00);
    }
}
