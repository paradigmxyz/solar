//! Elision of copies into write-only memory allocations.
//!
//! An allocation that never escapes and is never read is dead no matter what is
//! written into it: the copies and stores that fill it produce no observable
//! effect. The allocation itself remains because its free-memory-pointer bump
//! and failure behavior are independently observable. This arises after other
//! passes strip the readers of a materialized buffer — a scalar-replaced
//! struct, an inlined helper whose result is discarded, a re-encoded argument
//! that is dropped — leaving a copy whose destination no one observes.
//!
//! Ordinary dead-store elimination keeps such copies because they write
//! memory; proving the destination allocation is unread lets them go. The pass
//! is conservative: any read of the allocation (`mload`, `keccak256`, a copy
//! that reads it, or an escape into a call/return) keeps every write.

use crate::{
    mir::{Function, InstId, InstKind, Module, ValueId},
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Copy-elision pass over write-only memory allocations.
pub(crate) struct CopyElision;

impl MirPass for CopyElision {
    fn name(&self) -> &'static str {
        "copy-elision"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| CopyElisionCx::default().run(func))
    }
}

#[derive(Debug, Default)]
struct CopyElisionCx {
    /// Number of write-only allocations eliminated.
    eliminated: usize,
    /// Instruction uses indexed by operand value.
    uses: FxHashMap<ValueId, Vec<InstId>>,
    /// Values used by terminators.
    terminator_uses: FxHashSet<ValueId>,
}

impl CopyElisionCx {
    fn run(&mut self, func: &mut Function) -> bool {
        if func.instructions().any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::MSize)) {
            return false;
        }
        let allocs: Vec<ValueId> = func
            .instructions()
            .filter_map(|inst_id| {
                matches!(func.inst(inst_id).kind, InstKind::Alloc { .. })
                    .then(|| func.inst_result_value(inst_id))
                    .flatten()
            })
            .collect();
        if allocs.is_empty() {
            return false;
        }
        let mut changed = false;
        loop {
            self.uses.clear();
            self.terminator_uses.clear();
            self.index_uses(func);

            let mut dead = FxHashSet::default();
            for &object in &allocs {
                let Some(writes) = self.write_only_writes(func, object) else { continue };
                dead.extend(writes);
                self.eliminated += 1;
            }
            if dead.is_empty() {
                break;
            }
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|inst| !dead.contains(inst));
            }
            changed = true;
        }
        changed
    }

    fn index_uses(&mut self, func: &Function) {
        for inst_id in func.instructions() {
            for operand in func.inst(inst_id).operands() {
                self.uses.entry(operand).or_default().push(inst_id);
            }
        }
        for block in &func.blocks {
            if let Some(terminator) = &block.terminator {
                self.terminator_uses.extend(terminator.operands());
            }
        }
    }

    /// If every access to the allocation writes it, returns the write
    /// instructions to remove; returns `None` if the allocation is read.
    fn write_only_writes(&self, func: &Function, object: ValueId) -> Option<Vec<InstId>> {
        // Address values derived from the allocation. Follow only indexed uses
        // instead of rescanning every instruction for each derived value.
        let mut derived = FxHashSet::default();
        derived.insert(object);
        let mut worklist = vec![object];
        let mut seen = FxHashSet::default();
        while let Some(value) = worklist.pop() {
            for &inst_id in self.uses.get(&value).into_iter().flatten() {
                if !seen.insert(inst_id) {
                    continue;
                }
                let kind = &func.inst(inst_id).kind;
                let propagates = match kind {
                    InstKind::Add(first, second) | InstKind::Sub(first, second) => {
                        derived.contains(first) || derived.contains(second)
                    }
                    InstKind::MemoryObjectData(value, _)
                    | InstKind::MemoryObjectFieldAddr { object: value, .. }
                    | InstKind::MemoryObjectElementAddr { object: value, .. } => {
                        derived.contains(value)
                    }
                    _ => false,
                };
                if propagates
                    && let Some(result) = func.inst_result_value(inst_id)
                    && derived.insert(result)
                {
                    worklist.push(result);
                }
            }
        }

        let mut writes = Vec::new();
        let mut seen = FxHashSet::default();
        for value in &derived {
            for &inst_id in self.uses.get(value).into_iter().flatten() {
                if !seen.insert(inst_id) {
                    continue;
                }
                let inst = func.inst(inst_id);
                match &inst.kind {
                    // Writes to the allocation: the address is a derived value.
                    InstKind::MStore(addr, value) => {
                        if derived.contains(value) {
                            return None; // Storing an interior address elsewhere is a read/escape.
                        }
                        if derived.contains(addr) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MStore8(addr, _) | InstKind::MemoryZero(addr, _) => {
                        if derived.contains(addr) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::SetMemoryObjectLen(addr, len, _) => {
                        if derived.contains(len) {
                            return None;
                        }
                        if derived.contains(addr) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MemoryObjectStoreField { object, value, .. } => {
                        if derived.contains(value) {
                            return None;
                        }
                        if derived.contains(object) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MemoryObjectStoreElement { object, index, value, .. } => {
                        if derived.contains(index) || derived.contains(value) {
                            return None;
                        }
                        if derived.contains(object) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MemoryObjectStoreByte { object, index, value } => {
                        if derived.contains(index) || derived.contains(value) {
                            return None;
                        }
                        if derived.contains(object) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MemoryObjectStoreWord { object, offset, value } => {
                        if derived.contains(offset) || derived.contains(value) {
                            return None;
                        }
                        if derived.contains(object) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MemoryObjectCopy { destination, source, length, .. } => {
                        if derived.contains(source) || derived.contains(length) {
                            return None;
                        }
                        if derived.contains(destination) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::CalldataCopy(dest, _, _)
                    | InstKind::CodeCopy(dest, _, _)
                    | InstKind::ReturnDataCopy(dest, _, _) => {
                        if derived.contains(dest) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::ExtCodeCopy(_, dest, _, _) => {
                        if derived.contains(dest) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MCopy(dest, source, _) => {
                        if derived.contains(source) {
                            return None; // Read as a copy source.
                        }
                        if derived.contains(dest) {
                            writes.push(inst_id);
                        }
                    }
                    // Reads of the allocation keep every write.
                    InstKind::MLoad(addr) | InstKind::MemoryObjectLen(addr, _) => {
                        if derived.contains(addr) {
                            return None;
                        }
                    }
                    InstKind::Keccak256(offset, _) => {
                        if derived.contains(offset) {
                            return None;
                        }
                    }
                    // Address-derivation instructions are the closure itself.
                    InstKind::Add(..)
                    | InstKind::Sub(..)
                    | InstKind::MemoryObjectData(..)
                    | InstKind::MemoryObjectFieldAddr { .. }
                    | InstKind::MemoryObjectElementAddr { .. }
                    | InstKind::Alloc { .. } => {}
                    // Any other use of a derived address is treated as a read.
                    kind => {
                        if kind.operands().iter().any(|op| derived.contains(op)) {
                            return None;
                        }
                    }
                }
            }
        }

        // Terminators never read a non-escaping allocation (that would escape),
        // but guard defensively.
        if self.terminator_uses.iter().any(|value| derived.contains(value)) {
            return None;
        }

        (!writes.is_empty()).then_some(writes)
    }
}
