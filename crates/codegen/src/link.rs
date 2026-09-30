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

    /// Creates deferred data for another contract's bytecode, which final assembly links in.
    pub(crate) fn contract_code(code: ContractCode, name: Option<Symbol>) -> Self {
        Self { deferred: Some(code), ..Self::new(Bytes::new(), name) }
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

    /// Returns an upper bound on the size, from the `u32` bound on any linked data length.
    pub(crate) fn bound(self) -> U256 {
        U256::from(u32::MAX) + U256::from(self.addend)
    }

    /// Returns whether this is the exact length of the bytes `data` refers to.
    pub(crate) fn is_length_of(self, data: DataRef) -> bool {
        self.data == data.id && data.offset == 0 && self.addend == 0 && !self.aligned
    }
}

/// A fully qualified contract name, `source:Name`, identifying a library or an embedded contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QualifiedName(Symbol);

impl QualifiedName {
    /// Returns the fully qualified name of a contract.
    pub fn of_contract(gcx: Gcx<'_>, id: ContractId) -> Self {
        Self(Symbol::intern(&gcx.contract_fully_qualified_name(id).to_string()))
    }

    /// Parses `source:Name`.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        text.contains(':').then(|| Self(Symbol::intern(text)))
    }

    /// Returns the fully qualified name.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the source unit name and the contract name.
    pub fn split(&self) -> (&str, &str) {
        self.as_str().rsplit_once(':').expect("qualified names contain `:`")
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"", self.as_str().as_bytes().escape_ascii())
    }
}

/// Source-qualified libraries referenced by one module or bytecode artifact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LibraryTable {
    entries: IndexVec<LibraryId, QualifiedName>,
}

impl LibraryTable {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns the existing ID or adds the library to this table.
    pub fn intern(&mut self, library: QualifiedName) -> LibraryId {
        if let Some((id, _)) = self.entries.iter_enumerated().find(|(_, entry)| **entry == library)
        {
            id
        } else {
            self.entries.push(library)
        }
    }

    /// Returns the library named by a module-local ID.
    pub fn get(&self, id: LibraryId) -> Option<&QualifiedName> {
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
    pub(crate) contract: QualifiedName,
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
pub type EmbeddedBytecodes = FxHashMap<QualifiedName, ContractBytecodes>;
