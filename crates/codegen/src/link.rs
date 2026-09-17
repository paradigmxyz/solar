//! Library identities and relocatable bytecode shared by MIR and the backend.

use alloy_primitives::Bytes;
use solar_interface::Symbol;
use std::fmt;

/// A source-qualified library identity, shared across embedded contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LibraryId {
    pub(crate) source: Symbol,
    pub(crate) name: Symbol,
}

impl fmt::Display for LibraryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\"{}\":\"{}\"",
            self.source.as_str().as_bytes().escape_ascii(),
            self.name.as_str().as_bytes().escape_ascii()
        )
    }
}

/// A linker-supplied address at a byte offset in code or program data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LibraryRelocation {
    pub(crate) offset: usize,
    pub(crate) library: LibraryId,
}

impl fmt::Display for LibraryRelocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.offset, self.library)
    }
}

/// Bytecode and the library addresses that must be linked before execution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct RelocatableBytecode {
    pub(crate) bytes: Bytes,
    pub(crate) relocations: Vec<LibraryRelocation>,
}

impl From<Bytes> for RelocatableBytecode {
    fn from(bytes: Bytes) -> Self {
        Self { bytes, relocations: Vec::new() }
    }
}
