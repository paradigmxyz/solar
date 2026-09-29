//! Program data, library identities, and relocatable bytecode shared by MIR and the backend.

use alloy_primitives::{Bytes, U256};
use solar_data_structures::{fmt::FmtIteratorExt, index::IndexVec, map::FxHashMap, newtype_index};
use solar_interface::Symbol;
use solar_sema::{Gcx, hir::ContractId};
use std::fmt;

newtype_index! {
    /// An index into a module's library table.
    pub struct LibraryId;

    /// A unique identifier for constant data in a MIR or EVM IR module.
    pub(crate) struct DataId;
}

/// Bits that bound the length of any program data, checked when deferred data is linked.
pub(crate) const DATA_SIZE_BITS: u32 = 32;

/// One constant byte string and its optional display name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Data {
    pub(crate) bytes: Bytes,
    pub(crate) name: Option<Symbol>,
    /// Whether the data must be emitted at the end of the runtime program.
    pub(crate) emit_in_runtime: bool,
    /// Identities and byte offsets of unresolved library addresses in this data.
    pub(crate) library_relocations: Vec<LibraryRelocation>,
    /// Embedded contract bytecode that final assembly links in. Its bytes stay empty
    /// until then, so passes must treat it as opaque.
    pub(crate) deferred: Option<ContractCode>,
}

impl Data {
    /// Creates literal data.
    pub(crate) fn new(bytes: Bytes, name: Option<Symbol>) -> Self {
        Self {
            bytes,
            name,
            emit_in_runtime: false,
            library_relocations: Vec::new(),
            deferred: None,
        }
    }

    /// Displays the textual contents: `deferred creation|runtime <contract>`, or the hex bytes
    /// followed by any library relocations.
    pub(crate) fn display_contents<'a>(
        &'a self,
        libraries: &'a LibraryTable,
    ) -> impl fmt::Display + 'a {
        solar_data_structures::fmt::from_fn(move |f| {
            if let Some(code) = self.deferred {
                return write!(f, "deferred {code}");
            }
            write!(f, "hex\"{}\"", alloy_primitives::hex::display(&self.bytes))?;
            if !self.library_relocations.is_empty() {
                let relocations =
                    self.library_relocations.iter().map(|reloc| reloc.display(libraries));
                write!(f, " library_relocations [{}]", relocations.format(", "))?;
            }
            Ok(())
        })
    }
}

/// A relocatable reference to a byte within a data entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DataRef {
    pub(crate) id: DataId,
    pub(crate) offset: u32,
}

impl DataRef {
    pub(crate) const fn new(id: DataId, offset: u32) -> Self {
        Self { id, offset }
    }
}

/// A size derived from the byte length of deferred data: the length plus `addend`,
/// rounded down to a multiple of 32 when `aligned` is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DataSize {
    pub(crate) data: DataId,
    pub(crate) addend: u64,
    pub(crate) aligned: bool,
}

impl DataSize {
    /// Returns the size for data of `len` bytes.
    pub(crate) fn value(self, len: usize) -> U256 {
        let size = U256::from(len) + U256::from(self.addend);
        if self.aligned { size & !U256::from(31) } else { size }
    }

    /// Returns an upper bound on the size, from the bound on any data length.
    pub(crate) fn bound(self) -> U256 {
        U256::from((1u64 << DATA_SIZE_BITS) - 1) + U256::from(self.addend)
    }

    /// Returns whether this is the exact length of the bytes `data` refers to.
    pub(crate) fn is_length_of(self, data: DataRef) -> bool {
        self.data == data.id && data.offset == 0 && self.addend == 0 && !self.aligned
    }
}

/// A source-qualified contract name, identifying a library or an embedded contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Library {
    pub source: Symbol,
    pub name: Symbol,
}

impl Library {
    /// Returns the source-qualified name of a contract.
    pub fn of_contract(gcx: Gcx<'_>, id: ContractId) -> Self {
        let contract = gcx.hir.contract(id);
        let source = gcx.hir.source(contract.source).file.name.display().to_string();
        Self { source: Symbol::intern(&source), name: contract.name.name }
    }
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

impl RelocatableBytecode {
    /// Returns this bytecode's library relocations with identities interned into `libraries`.
    pub(crate) fn relocations_in(&self, libraries: &mut LibraryTable) -> Vec<LibraryRelocation> {
        self.relocations
            .iter()
            .map(|reloc| LibraryRelocation {
                offset: reloc.offset,
                library: libraries
                    .intern(*self.libraries.get(reloc.library).expect("valid bytecode library ID")),
            })
            .collect()
    }
}

/// Bytecode of another contract that a module embeds as program data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ContractCode {
    /// The source-qualified name of the embedded contract.
    pub(crate) contract: Library,
    /// Whether this is the creation bytecode rather than the runtime bytecode.
    pub(crate) creation: bool,
}

impl ContractCode {
    /// Returns the embedded bytecode in `bytecodes`.
    pub(crate) fn bytecode(self, bytecodes: &EmbeddedBytecodes) -> &RelocatableBytecode {
        let bytecodes =
            bytecodes.get(&self.contract).expect("embedded contract bytecode must be supplied");
        if self.creation { &bytecodes.deployment } else { &bytecodes.runtime }
    }
}

impl fmt::Display for ContractCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.creation { "creation" } else { "runtime" };
        write!(f, "{kind} {}", self.contract)
    }
}

/// Generated bytecode of a contract that other contracts embed.
#[derive(Clone, Debug, Default)]
pub struct ContractBytecodes {
    /// Deployment bytecode, including the initcode prefix.
    pub deployment: RelocatableBytecode,
    /// Deployed runtime bytecode.
    pub runtime: RelocatableBytecode,
}

/// Generated bytecode of the contracts a module embeds, by source-qualified contract name.
pub type EmbeddedBytecodes = FxHashMap<Library, ContractBytecodes>;
