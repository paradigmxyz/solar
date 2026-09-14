//! Shared bounds for source-origin metadata across compiler IR layers.

/// Maximum retained origins of a shared instruction.
///
/// Retaining several origins identifies ambiguity without allowing repeated
/// merging to grow metadata without bound. Consumers must not select an
/// arbitrary origin when more than one is present.
pub const MAX_DEBUG_SPANS: usize = 8;
