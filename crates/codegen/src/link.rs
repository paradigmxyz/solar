//! Library identities and relocatable bytecode shared by MIR and the backend.

use alloy_primitives::Bytes;
use solar_data_structures::{index::IndexVec, newtype_index};
use solar_interface::Symbol;
use std::fmt;

newtype_index! {
    /// An index into a module's library table.
    pub struct LibraryId;
}

/// A source-qualified library name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Library {
    pub source: Symbol,
    pub name: Symbol,
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\"{}\":\"{}\"",
            self.source.as_str().as_bytes().escape_ascii(),
            self.name.as_str().as_bytes().escape_ascii()
        )
    }
}

/// Source-qualified libraries referenced by one module or bytecode artifact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LibraryTable {
    entries: IndexVec<LibraryId, Library>,
}

impl LibraryTable {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns the existing ID or adds the library to this table.
    pub fn intern(&mut self, library: Library) -> LibraryId {
        if let Some((id, _)) = self.entries.iter_enumerated().find(|(_, entry)| **entry == library)
        {
            id
        } else {
            self.entries.push(library)
        }
    }

    /// Returns the library named by a module-local ID.
    pub fn get(&self, id: LibraryId) -> Option<&Library> {
        self.entries.get(id)
    }
}

/// A linker-supplied address at a byte offset in code or program data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LibraryRelocation {
    pub offset: usize,
    pub library: LibraryId,
}

impl LibraryRelocation {
    pub(crate) fn display<'a>(&'a self, libraries: &'a LibraryTable) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            write!(f, "{}: {}", self.offset, libraries.get(self.library).expect("valid library ID"))
        })
    }
}

/// Bytecode and the library addresses that must be linked before execution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RelocatableBytecode {
    pub libraries: LibraryTable,
    pub bytes: Bytes,
    pub relocations: Vec<LibraryRelocation>,
}

impl From<Bytes> for RelocatableBytecode {
    fn from(bytes: Bytes) -> Self {
        Self { bytes, relocations: Vec::new(), libraries: LibraryTable::default() }
    }
}
