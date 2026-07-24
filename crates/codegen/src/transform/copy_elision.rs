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
    analysis::AliasAnalysis,
    mir::{Function, InstId, InstKind, Module, ValueId},
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::map::FxHashSet;

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
        run_function_pass(module, analyses, |func, analyses| {
            CopyElisionCx::default().run(func, &analyses.alias)
        })
    }
}

#[derive(Debug, Default)]
struct CopyElisionCx {
    /// Number of write-only allocations eliminated.
    eliminated: usize,
}

impl CopyElisionCx {
    fn run(&mut self, func: &mut Function, alias: &AliasAnalysis) -> bool {
        let allocs: Vec<ValueId> = func
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().copied())
            .filter_map(|inst_id| {
                matches!(func.instructions[inst_id].kind, InstKind::Alloc { .. })
                    .then(|| func.inst_result_value(inst_id))
                    .flatten()
            })
            .collect();
        if allocs.is_empty() {
            return false;
        }

        let mut dead: FxHashSet<InstId> = FxHashSet::default();
        for object in allocs {
            if alias.value_escapes(func, object) {
                continue;
            }
            let Some(writes) = self.write_only_writes(func, object) else { continue };
            dead.extend(writes);
            self.eliminated += 1;
        }
        if dead.is_empty() {
            return false;
        }
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !dead.contains(inst));
        }
        true
    }

    /// If every access to the allocation writes it, returns the write
    /// instructions to remove; returns `None` if the allocation is read.
    fn write_only_writes(&self, func: &Function, object: ValueId) -> Option<Vec<InstId>> {
        // Address values derived from the allocation. The allocation does not
        // escape, so this stays a small local closure over address arithmetic.
        let mut derived = FxHashSet::default();
        derived.insert(object);
        loop {
            let mut changed = false;
            for (value_id, value) in func.values.iter_enumerated() {
                let crate::mir::Value::Inst(inst_id) = value else { continue };
                let propagates = match &func.instructions[*inst_id].kind {
                    InstKind::Add(a, b) | InstKind::Sub(a, b) => {
                        derived.contains(a) || derived.contains(b)
                    }
                    InstKind::MemoryObjectData(v, _)
                    | InstKind::MemoryObjectFieldAddr { object: v, .. } => derived.contains(v),
                    InstKind::MemoryObjectElementAddr { object: v, .. } => derived.contains(v),
                    _ => false,
                };
                if propagates && derived.insert(value_id) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut writes = Vec::new();
        for (inst_id, inst) in func.instructions.iter_enumerated() {
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
                InstKind::MStore8(addr, _) | InstKind::SetMemoryObjectLen(addr, _, _) => {
                    if derived.contains(addr) {
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

        // Terminators never read a non-escaping allocation (that would escape),
        // but guard defensively.
        for block in &func.blocks {
            if let Some(term) = &block.terminator
                && term.operands().iter().any(|op| derived.contains(op))
            {
                return None;
            }
        }

        (!writes.is_empty()).then_some(writes)
    }
}
