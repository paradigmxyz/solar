#[cfg(not(feature = "nightly"))]
pub use std::convert::{identity as likely, identity as unlikely};
#[cfg(feature = "nightly")]
pub use std::hint::{likely, unlikely};
