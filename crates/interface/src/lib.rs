//! Source positions and related helper functions.
//!
//! Important concepts in this module include:
//!
//! - the *span*, represented by [`Span`] and related types;
//! - source code as represented by a [`SourceMap`]; and
//! - interned strings, represented by [`Symbol`]s, with some common symbols available statically in
//!   the [`sym`] module.
//!
//! ## Note
//!
//! This API is completely unstable and subject to change.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "nightly", feature(min_specialization))]

#[macro_use]
extern crate tracing;

pub mod diagnostics;
use diagnostics::ErrorGuaranteed;

mod globals;
pub use globals::SessionGlobals;

mod pos;
pub use pos::{BytePos, CharPos, Pos};

mod session;
pub use session::Session;

pub mod source_map;
pub use source_map::SourceMap;

mod span;
pub use span::Span;

mod symbol;
pub use symbol::{kw, sym, Ident, Symbol};

pub use anstream::ColorChoice;

/// Compiler result type.
pub type Result<T = (), E = ErrorGuaranteed> = std::result::Result<T, E>;

/// Creates a new compiler session on the current thread if it doesn't exist already and then
/// executes the given closure.
pub fn enter<R>(f: impl FnOnce() -> R) -> R {
    SessionGlobals::with_or_default(|_| f())
}

#[macro_export]
macro_rules! time {
    ($level:expr, $what:literal, || $e:expr) => {
        if enabled!($level) {
            let timer = std::time::Instant::now();
            let res = $e;
            event!($level, elapsed=?timer.elapsed(), $what);
            res
        } else {
            $e
        }
    };
}

#[macro_export]
macro_rules! debug_time {
    ($what:literal, || $e:expr) => {
        $crate::time!(tracing::Level::DEBUG, $what, || $e)
    };
}

#[macro_export]
macro_rules! trace_time {
    ($what:literal, || $e:expr) => {
        $crate::time!(tracing::Level::TRACE, $what, || $e)
    };
}
