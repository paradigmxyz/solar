#[cfg(not(feature = "nightly"))]
pub use std::convert::{identity as likely, identity as unlikely};

/// See [`std::hint::likely`].
#[cfg(feature = "nightly")]
#[inline(always)]
pub fn likely(b: bool) -> bool {
    std::hint::likely(b)
}

/// See [`std::hint::unlikely`].
#[cfg(feature = "nightly")]
#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    std::hint::unlikely(b)
}
