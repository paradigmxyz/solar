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

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({:?})", stringify!($name), self.get())
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
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            $vis const fn new(value: u32) -> Self {
                Self($crate::index::BaseIndex32::new(value))
            }

            /// Creates a new `$name` from the given `value`.
            ///
            /// Returns `None` if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            $vis const fn try_new(value: u32) -> Option<Self> {
                match $crate::index::BaseIndex32::try_new(value) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            /// Returns the underlying index value.
            #[inline(always)]
            #[must_use]
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
            // NOTE: Use `value()` instead of projecting the field directly.
            #[cfg(feature = "nightly")]
            value: $primitive,
            #[cfg(not(feature = "nightly"))]
            value: std::num::NonZero<$primitive>,
        }

        impl fmt::Debug for $name {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

        impl Idx for $name {
            #[inline(always)]
            fn from_usize(value: usize) -> Self {
                if value > Self::MAX_AS as usize {
                    index_overflow();
                }
                // SAFETY: `value` is less than or equal to `MAX`.
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
            #[must_use]
            pub const fn new(value: $primitive) -> Self {
                if value > Self::MAX_AS {
                    index_overflow();
                }
                // SAFETY: `value` is less than or equal to `MAX`.
                unsafe { Self::new_unchecked(value) }
            }

            /// Creates a new `$name` from the given `value`.
            ///
            /// Returns `None` if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            pub const fn try_new(value: $primitive) -> Option<Self> {
                if value > Self::MAX_AS {
                    None
                } else {
                    // SAFETY: `value` is less than or equal to `MAX`.
                    Some(unsafe { Self::new_unchecked(value) })
                }
            }

            /// Creates a new `$name` from the given `value`, without checking for overflow.
            ///
            /// # Safety
            ///
            /// The caller must ensure that `value` is less than or equal to `MAX`.
            #[inline(always)]
            #[must_use]
            pub const unsafe fn new_unchecked(value: $primitive) -> Self {
                // SAFETY: guaranteed by the caller.
                #[cfg(feature = "nightly")]
                return unsafe { std::intrinsics::transmute_unchecked(value) };

                #[cfg(not(feature = "nightly"))]
                return unsafe { Self { value: std::num::NonZero::new_unchecked(value.unchecked_add(1)) } };
            }

            /// Returns the underlying index value.
            #[inline(always)]
            #[must_use]
            pub const fn get(self) -> $primitive {
                // SAFETY: Transmute instead of projecting the field directly.
                //
                // See:
                // - https://github.com/rust-lang/rust/pull/133651
                // - https://github.com/rust-lang/compiler-team/issues/807
                #[cfg(feature = "nightly")]
                return unsafe { std::intrinsics::transmute_unchecked(self) };

                // SAFETY: non-zero minus one doesn't overflow.
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
        assert_eq!(BaseIndex32::MAX.get(), BaseIndex32::MAX_AS);
        assert_eq!(BaseIndex32::MAX.get(), 0xFFFF_FF00);
        assert_eq!(BaseIndex32::new(0xFFFF_FF00).get(), 0xFFFF_FF00);
    }
}
