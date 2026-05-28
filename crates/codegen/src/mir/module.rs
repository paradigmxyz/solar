//! MIR module (top-level container).

use super::{Function, FunctionId, MirType};
use solar_data_structures::index::IndexVec;
use solar_interface::Ident;
use std::fmt;

/// A MIR module representing a compiled contract.
#[derive(Clone, Debug)]
pub struct Module {
    /// Module/contract name.
    pub name: Ident,
    /// All functions in this module.
    pub functions: IndexVec<FunctionId, Function>,
    /// Data segments (for string literals, etc.).
    pub data_segments: Vec<DataSegment>,
    /// Storage layout.
    pub storage_layout: Vec<StorageSlot>,
    /// Whether this is an interface (no bytecode generation).
    pub is_interface: bool,
}

impl Module {
    /// Creates a new module.
    #[must_use]
    pub fn new(name: Ident) -> Self {
        Self {
            name,
            functions: IndexVec::new(),
            data_segments: Vec::new(),
            storage_layout: Vec::new(),
            is_interface: false,
        }
    }

    /// Adds a function to the module.
    pub fn add_function(&mut self, function: Function) -> FunctionId {
        self.functions.push(function)
    }

    /// Returns the function for the given ID.
    #[must_use]
    pub fn function(&self, id: FunctionId) -> &Function {
        &self.functions[id]
    }

    /// Returns a mutable reference to the function.
    pub fn function_mut(&mut self, id: FunctionId) -> &mut Function {
        &mut self.functions[id]
    }

    /// Adds a data segment.
    pub fn add_data_segment(&mut self, data: Vec<u8>) -> usize {
        let index = self.data_segments.len();
        self.data_segments.push(DataSegment { data });
        index
    }

    /// Adds a storage slot.
    pub fn add_storage_slot(&mut self, slot: StorageSlot) -> usize {
        let index = self.storage_layout.len();
        self.storage_layout.push(slot);
        index
    }

    /// Returns an iterator over all functions.
    pub fn iter_functions(&self) -> impl Iterator<Item = (FunctionId, &Function)> {
        self.functions.iter_enumerated()
    }
}

/// A data segment in the module.
#[derive(Clone, Debug)]
pub struct DataSegment {
    /// The raw bytes of this segment.
    pub data: Vec<u8>,
}

/// A storage slot in the contract.
#[derive(Clone, Debug)]
pub struct StorageSlot {
    /// The slot number.
    pub slot: u64,
    /// The offset within the slot (for packed storage).
    pub offset: u8,
    /// The type of the value stored.
    pub ty: MirType,
    /// The variable name (for debugging).
    pub name: Option<Ident>,
}

impl StorageSlot {
    /// Creates a new storage slot.
    #[must_use]
    pub fn new(slot: u64, ty: MirType) -> Self {
        Self { slot, offset: 0, ty, name: None }
    }

    /// Creates a new storage slot with an offset.
    #[must_use]
    pub fn with_offset(slot: u64, offset: u8, ty: MirType) -> Self {
        Self { slot, offset, ty, name: None }
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "module {} {{", self.name)?;

        if !self.storage_layout.is_empty() {
            writeln!(f, "  storage:")?;
            for slot in &self.storage_layout {
                writeln!(f, "    slot {} @ {}: {}", slot.slot, slot.offset, slot.ty)?;
            }
            writeln!(f)?;
        }

        for (id, func) in self.functions.iter_enumerated() {
            writeln!(f, "  ; function {}", id.index())?;
            writeln!(f, "  {func}")?;
        }

        writeln!(f, "}}")
    }
}
