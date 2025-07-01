#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "nightly", feature(panic_update_hook))]

#[macro_use]
extern crate tracing;

pub mod diagnostics;
use diagnostics::ErrorGuaranteed;

mod globals;
pub use globals::SessionGlobals;

mod pos;
pub use pos::{BytePos, CharPos, RelativeBytePos};

mod session;
pub use session::{Session, SessionBuilder};

pub mod source_map;
pub use source_map::SourceMap;

mod span;
pub use span::{Span, Spanned};

mod symbol;
pub use symbol::{kw, sym, Ident, Symbol};

pub mod panic_hook;

pub use anstream::ColorChoice;
pub use dunce::canonicalize;
pub use solar_config as config;
pub use solar_data_structures as data_structures;

/// The current version of the Solar compiler.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Compiler result type.
pub type Result<T = (), E = ErrorGuaranteed> = std::result::Result<T, E>;

/// Pluralize a word based on a count.
#[macro_export]
#[rustfmt::skip]
macro_rules! pluralize {
    // Pluralize based on count (e.g., apples)
    ($x:expr) => {
        if $x == 1 { "" } else { "s" }
    };
    ("has", $x:expr) => {
        if $x == 1 { "has" } else { "have" }
    };
    ("is", $x:expr) => {
        if $x == 1 { "is" } else { "are" }
    };
    ("was", $x:expr) => {
        if $x == 1 { "was" } else { "were" }
    };
    ("this", $x:expr) => {
        if $x == 1 { "this" } else { "these" }
    };
}

/// Creates new session globals on the current thread if they doesn't exist already and then
/// executes the given closure.
///
/// Prefer [`Session::enter`] to this function if possible to also set the source map and thread
/// pool.
///
/// Using this instead of [`Session::enter`] may cause unexpected panics.
#[inline]
pub fn enter<R>(f: impl FnOnce() -> R) -> R {
    SessionGlobals::with_or_default(|_| f())
}
