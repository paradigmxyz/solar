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

mod globals;
mod pos;
mod span;
mod symbol;

pub use globals::*;
pub use pos::{BytePos, CharPos, Pos};
pub use span::Span;
pub use symbol::{kw, sym, Ident, Symbol};
