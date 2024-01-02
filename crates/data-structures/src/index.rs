//! Index types.

use std::fmt;

pub use index_vec::*;

macro_rules! base_index {
    ($(#[$attr:meta])* $name:ident($primitive:ident || $non_zero:ident <= $max:literal)) => {
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
            value: std::num::$non_zero,
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
                Self::new(value as $primitive)
            }

            #[inline(always)]
            fn index(self) -> usize {
                self.get() as usize
            }
        }

        impl $name {
            /// Creates a new `$name` from the given value.
            pub const MAX_AS: $primitive = $max;

            /// The maximum index value.
            pub const MAX: Self = Self::new(Self::MAX_AS);

            /// Creates a new `$name` from the given `value`.
            ///
            /// # Panics
            ///
            /// Panics if `value` exceeds `MAX`.
            #[inline]
            pub const fn new(value: $primitive) -> Self {
                if value > Self::MAX_AS {
                    index_overflow();
                }
                unsafe {
                    Self {
                        #[cfg(feature = "nightly")]
                        value,
                        #[cfg(not(feature = "nightly"))]
                        value: std::num::$non_zero::new_unchecked(match value.checked_add(1) {
                            Some(value) => value,
                            None => index_overflow(),
                        }),
                    }
                }
            }

            /// Gets the underlying index value.
            #[inline(always)]
            pub const fn get(self) -> $primitive {
                #[cfg(feature = "nightly")]
                return self.value;

                #[cfg(not(feature = "nightly"))]
                return match self.value.get().checked_sub(1) {
                    Some(value) => value,
                    // SAFETY: Non-zero.
                    None => unsafe { std::hint::unreachable_unchecked() },
                };
            }
        }
    };
}

base_index!(BaseIndex32(u32 || NonZeroU32 <= 0xFFFF_FF00));

#[inline(never)]
#[cold]
const fn index_overflow() -> ! {
    panic!("index overflowed")
}
