//! Dead free-memory-pointer store elimination.
//!
//! Allocation lowering turns every allocation into a read of the free memory pointer and a store
//! of the bumped pointer, and a `@custom:solar-scratch` block stores the pointer it saved back
//! where it ends. This pass removes the stores to the pointer's slot that no execution observes:
//!
//! ```text
//! p = mload 64
//! mstore 64, p + 96        ; dead: stored again below before anything reads the slot
//! mstore p + 32, x
//! h = keccak256 p + 32, 64
//! mstore 64, p             ; redundant once the bump is gone: the slot already holds p
//! ```
//!
//! A scratch block whose last allocation ends it thus leaves the pointer untouched, and uses the
//! memory past it without moving it.
//!
//! Each block is scanned on its own, in reverse postorder. The scan tracks the value the slot
//! holds, when known, which a block entered from a single predecessor takes over from it, and the
//! latest store to the slot that nothing has read yet. A store is dead when another store to the
//! slot follows before any read, and removing it leaves the slot with the value it held before, so
//! a restore that follows a removed bump stores the value already there: a store of the value the
//! slot holds is redundant. Reads are the loads of the slot and every other read of memory that
//! may reach its word; writes that may reach it forget both the value and the pending store. A
//! pointer derived from the free memory pointer lies at or above the heap, but any address the
//! alias analysis cannot bound that way may reach the slot. Internal and external calls,
//! creations, and gas and memory-size observations end what the scan knows, and a store still
//! pending at the end of its block stays, since a successor may read the pointer.
//!
//! The pass runs on lowered word SSA, where allocations are explicit pointer traffic, after
//! `fmp-cse` forwarded the loads it could, in gas and size builds alike: a removed store saves
//! both.

use crate::mir::{
    BlockId, EffectKind, Function, InstId, InstKind, Module, Value, ValueId,
    analysis::{AliasAnalysis, CfgInfo},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec};

/// Pass removing free-memory-pointer stores no execution observes.
pub(crate) struct FmpDse;

impl MirPass for FmpDse {
    fn name(&self) -> &'static str {
        "fmp-dse"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| remove_dead_fmp_stores(func))
    }
}

/// How an instruction touches the free memory pointer's slot.
enum SlotAccess {
    /// Does not touch it.
    Untouched,
    /// `mload 64`, with its result.
    Load(ValueId),
    /// `mstore 64, value`.
    Store(ValueId),
    /// May read it.
    Read,
    /// May write it, or observe what the scan cannot follow.
    Clobber,
}

fn remove_dead_fmp_stores(func: &mut Function) -> bool {
    let cfg = CfgInfo::new(func);
    let mut predecessors = index_vec![SmallVec::<[BlockId; 2]>::new(); func.blocks.len()];
    for &block in cfg.rpo() {
        for &successor in cfg.successors(block) {
            predecessors[successor].push(block);
        }
    }
    // The value the slot holds where each scanned block ends, when known.
    let mut slot_out = index_vec![None; func.blocks.len()];
    let mut removed = DenseBitSet::new_empty(func.num_insts());
    for &block in cfg.rpo() {
        // The value the slot holds, when known: what the only predecessor left in it.
        let mut slot = match predecessors[block].as_slice() {
            &[predecessor] => slot_out[predecessor],
            _ => None,
        };
        // The latest store nothing has read yet, with the value the slot held before it.
        let mut pending: Option<(InstId, Option<ValueId>)> = None;
        for &inst in &func.blocks[block].instructions {
            match slot_access(func, inst) {
                SlotAccess::Untouched => {}
                SlotAccess::Load(result) => {
                    pending = None;
                    slot.get_or_insert(result);
                }
                SlotAccess::Store(value) => {
                    // mstore 64, a; ...; mstore 64, b => ...; mstore 64, b
                    if let Some((store, before)) = pending.take() {
                        removed.insert(store);
                        slot = before;
                    }
                    // mstore 64, x while the slot holds x => (nothing)
                    if slot.is_some_and(|held| same_value(func, held, value)) {
                        removed.insert(inst);
                    } else {
                        pending = Some((inst, slot));
                        slot = Some(value);
                    }
                }
                SlotAccess::Read => pending = None,
                SlotAccess::Clobber => {
                    pending = None;
                    slot = None;
                }
            }
        }
        slot_out[block] = slot;
    }
    if removed.is_empty() {
        return false;
    }
    for block in &mut func.blocks {
        block.instructions.retain(|inst| !removed.contains(*inst));
    }
    true
}

/// Classifies how `inst` touches the free memory pointer's slot.
fn slot_access(func: &Function, inst: InstId) -> SlotAccess {
    let is_slot = |address: ValueId| func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT);
    let reaches = |address: ValueId, size: Option<u64>| {
        AliasAnalysis::range_may_overlap_fmp(func, address, size)
    };
    let read_if = |reaches: bool| if reaches { SlotAccess::Read } else { SlotAccess::Untouched };
    let clobber_if =
        |reaches: bool| if reaches { SlotAccess::Clobber } else { SlotAccess::Untouched };
    match func.inst(inst).kind {
        InstKind::MLoad(address) if is_slot(address) => match func.inst_result_value(inst) {
            Some(result) => SlotAccess::Load(result),
            None => SlotAccess::Read,
        },
        InstKind::MStore(address, value) if is_slot(address) => SlotAccess::Store(value),
        InstKind::MLoad(address) => read_if(reaches(address, Some(32))),
        InstKind::Keccak256(address, size)
        | InstKind::Log0(address, size)
        | InstKind::Log1(address, size, _)
        | InstKind::Log2(address, size, _, _)
        | InstKind::Log3(address, size, _, _, _)
        | InstKind::Log4(address, size, _, _, _, _) => {
            read_if(reaches(address, func.value_u64(size)))
        }
        InstKind::MStore(address, _) => clobber_if(reaches(address, Some(32))),
        InstKind::MStore8(address, _) => clobber_if(reaches(address, Some(1))),
        InstKind::MCopy(destination, source, size) => {
            let size = func.value_u64(size);
            if reaches(destination, size) {
                SlotAccess::Clobber
            } else {
                read_if(reaches(source, size))
            }
        }
        InstKind::CalldataCopy(destination, _, size)
        | InstKind::DataCopy(_, destination, size)
        | InstKind::CodeCopy(destination, _, size)
        | InstKind::ReturnDataCopy(destination, _, size)
        | InstKind::ExtCodeCopy(_, destination, _, size) => {
            clobber_if(reaches(destination, func.value_u64(size)))
        }
        InstKind::Gas | InstKind::MSize => SlotAccess::Clobber,
        ref kind => match kind.effect_kind() {
            EffectKind::Pure
            | EffectKind::StorageRead
            | EffectKind::StorageWrite
            | EffectKind::TransientRead
            | EffectKind::TransientWrite
            | EffectKind::EnvironmentRead
            | EffectKind::ImmutableRead => SlotAccess::Untouched,
            EffectKind::MemoryRead
            | EffectKind::MemoryWrite
            | EffectKind::ExternalCall
            | EffectKind::ICall
            | EffectKind::Create
            | EffectKind::Log
            | EffectKind::ImmutableWrite => SlotAccess::Clobber,
        },
    }
}

/// Whether `a` and `b` are the same word: one value, or equal immediates.
fn same_value(func: &Function, a: ValueId, b: ValueId) -> bool {
    a == b
        || matches!(
            (func.value(a), func.value(b)),
            (Value::Immediate(a), Value::Immediate(b)) if a == b
        )
}
