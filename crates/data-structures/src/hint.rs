#[cfg(feature = "nightly")]
pub use std::intrinsics::{likely, unlikely};

#[cfg(not(feature = "nightly"))]
#[inline(always)]
#[cold]
fn cold() {}

#[cfg(not(feature = "nightly"))]
#[inline(always)]
pub fn likely(b: bool) -> bool {
    if !b {
        cold();
    }
    b
}

#[cfg(not(feature = "nightly"))]
#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}
