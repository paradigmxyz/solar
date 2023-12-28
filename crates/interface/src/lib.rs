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
    html_logo_url = "https://raw.githubusercontent.com/danipopes/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/danipopes/sulk/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "nightly", feature(min_specialization))]

pub mod diagnostics;
mod globals;
mod pos;
mod span;
mod symbol;

use diagnostics::{ErrorGuaranteed, FatalError};
pub use globals::{
    create_default_session_if_not_set_then, create_session_globals_then, set_session_globals_then,
    with_session_globals, SessionGlobals,
};
pub use pos::{BytePos, CharPos, Pos};
pub use span::Span;
pub use symbol::{kw, sym, Ident, Symbol};

/// TODO
pub struct SourceMap(());

impl SourceMap {
    /// Creates a new empty source map.
    pub fn empty() -> Self {
        Self(())
    }

    /// Returns `true` if the given span is multi-line.
    pub fn is_multiline(&self, span: Span) -> bool {
        // TODO
        let _ = span;
        false
    }
}

/// Creates a new compiler session on the current thread if it doesn't exist already and then
/// executes the given closure. Catching fatal errors and returning them as [`ErrorGuaranteed`].
///
/// # Errors
///
/// Returns [`ErrorGuaranteed`] if a [`FatalError`] was caught. Other panics are propagated.
pub fn enter<R>(f: impl FnOnce() -> R) -> Result<R, ErrorGuaranteed> {
    create_default_session_if_not_set_then(|_| FatalError::catch(f))
}
