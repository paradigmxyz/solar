//! Index types. See [`::index_vec`].

use std::fmt;

pub use index_vec::{
    Idx, IdxRangeBounds, IdxSliceIndex, IndexBox, IndexSlice, IndexVec, index_box, index_vec,
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
                Self::from_usize(value)
            }

            #[inline(always)]
            fn index(self) -> usize {
                self.index()
            }
        }

        impl $name {
            /// The maximum index value.
            $vis const MAX: Self = Self($crate::index::BaseIndex32::MAX);

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            #[cfg_attr(debug_assertions, track_caller)]
            $vis const fn new(value: u32) -> Self {
                Self($crate::index::BaseIndex32::new(value))
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// # Safety
            ///
            /// The caller must ensure that `value` is less than or equal to `MAX`.
            #[inline(always)]
            #[must_use]
            $vis const unsafe fn new_unchecked(value: u32) -> Self {
                Self(unsafe { $crate::index::BaseIndex32::new_unchecked(value) })
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
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

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            #[cfg_attr(debug_assertions, track_caller)]
            $vis const fn from_usize(value: usize) -> Self {
                Self($crate::index::BaseIndex32::from_usize(value))
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// # Safety
            ///
            /// The caller must ensure that `value` is less than or equal to `MAX`.
            #[inline(always)]
            #[must_use]
            $vis const unsafe fn from_usize_unchecked(value: usize) -> Self {
                Self(unsafe { $crate::index::BaseIndex32::from_usize_unchecked(value) })
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// Returns `None` if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            $vis const fn try_from_usize(value: usize) -> Option<Self> {
                match $crate::index::BaseIndex32::try_from_usize(value) {
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

            /// Returns the underlying index value.
            #[inline(always)]
            #[must_use]
            $vis const fn index(self) -> usize {
                self.0.index()
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
                Self::from_usize(value)
            }

            #[inline(always)]
            fn index(self) -> usize {
                self.index()
            }
        }

        impl $name {
            /// The maximum index value, as the underlying primitive type.
            pub const MAX_AS: $primitive = $max;

            /// The maximum index value.
            pub const MAX: Self = Self::new(Self::MAX_AS);

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            #[cfg_attr(debug_assertions, track_caller)]
            pub const fn new(value: $primitive) -> Self {
                match Self::try_new(value) {
                    Some(value) => value,
                    None => index_overflow(),
                }
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
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

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            #[cfg_attr(debug_assertions, track_caller)]
            pub const fn from_usize(value: usize) -> Self {
                match Self::try_from_usize(value) {
                    Some(value) => value,
                    None => index_overflow(),
                }
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`."]
            ///
            /// Returns `None` if `value` exceeds `MAX`.
            #[inline(always)]
            #[must_use]
            pub const fn try_from_usize(value: usize) -> Option<Self> {
                if value > Self::MAX_AS as usize {
                    None
                } else {
                    // SAFETY: `value` is less than or equal to `MAX`.
                    Some(unsafe { Self::new_unchecked(value as $primitive) })
                }
            }

            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`, without checking for overflow."]
            ///
            /// # Safety
            ///
            /// The caller must ensure that `value` is less than or equal to `MAX`.
            #[inline(always)]
            #[must_use]
            pub const unsafe fn new_unchecked(value: $primitive) -> Self {
                debug_assert!(value <= Self::MAX_AS);

                // SAFETY: guaranteed by the caller.
                #[cfg(feature = "nightly")]
                return unsafe { std::intrinsics::transmute_unchecked(value) };

                #[cfg(not(feature = "nightly"))]
                return unsafe { Self { value: std::num::NonZero::new_unchecked(value.unchecked_add(1)) } };
            }


            #[doc = "Creates a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from the given `value`, without checking for overflow."]
            ///
            /// # Safety
            ///
            /// The caller must ensure that `value` is less than or equal to `MAX`.
            #[inline(always)]
            #[must_use]
            pub const unsafe fn from_usize_unchecked(value: usize) -> Self {
                debug_assert!(value <= Self::MAX_AS as usize);
                unsafe { Self::new_unchecked(value as $primitive) }
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

            /// Returns the underlying index value.
            #[inline(always)]
            #[must_use]
            pub const fn index(self) -> usize {
                self.get() as usize
            }
        }
    };
}

base_index!(BaseIndex32(u32 <= 0xFFFF_FF00));

#[inline(never)]
#[cold]
#[cfg_attr(debug_assertions, track_caller)]
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
