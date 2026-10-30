//! Block and function hotness shared by the compiler IRs.

/// Hotness metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Hotness {
    /// The code is expected to be frequently executed.
    #[default]
    Hot,
    /// The code is expected to be infrequently executed.
    Cold,
}

impl Hotness {
    /// Returns whether this is cold code.
    #[must_use]
    pub const fn is_cold(self) -> bool {
        matches!(self, Self::Cold)
    }
}
