//! Library identities and relocatable bytecode shared by MIR and the backend.

use alloy_primitives::Bytes;
use solar_interface::Symbol;
use std::fmt;

/// A source-qualified library identity, shared across embedded contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LibraryId {
    pub source: Symbol,
    pub name: Symbol,
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
pub struct LibraryRelocation {
    pub offset: usize,
    pub library: LibraryId,
}

impl fmt::Display for LibraryRelocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.offset, self.library)
    }
}

/// Bytecode and the library addresses that must be linked before execution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RelocatableBytecode {
    pub bytes: Bytes,
    pub relocations: Vec<LibraryRelocation>,
}

impl From<Bytes> for RelocatableBytecode {
    fn from(bytes: Bytes) -> Self {
        Self { bytes, relocations: Vec::new() }
    }
}
