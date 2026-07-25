//! Static placement of provably local heap allocations.
//!
//! When a constant-size `alloc` executes at most once per call and its pointer
//! never escapes the function, the allocation does not need a runtime
//! free-memory-pointer bump. This module proves those properties for the EVM
//! backend and retains a conservative MIR rewrite for the explicit
//! `static-alloc` pass.
//!
//! Safety contract:
//! - external entries only: their locals are absolute low-memory addresses;
//! - the block cannot re-execute, so the reused static region cannot expose a previous iteration's
//!   contents where fresh zeroed memory was expected;
//! - every use of the pointer is an in-bounds address derivation into exact loads, stores, hashes,
//!   copies, logs, or external-data terminators — the pointer value never escapes into stored data,
//!   call arguments, or unbounded arithmetic;
//! - functions observing `msize` are skipped: eliding a bump changes the high-water mark.

use crate::{
    analysis::CfgInfo,
    memory::EvmMemoryLayout,
    mir::{BlockId, Function, Immediate, InstId, InstKind, Module, Terminator, Value, ValueId},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Pass that places provably local allocations statically.
pub(crate) struct StaticAlloc;

impl MirPass for StaticAlloc {
    fn name(&self) -> &'static str {
        "static-alloc"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        // Every entry's locals share the same low-memory region — only one
        // entry runs per call — so the tallest entry's frame top is a shadow
        // the others can grow into without moving the shared static-frame
        // region or any spill base above it. Placements stay inside it.
        let shadow = module
            .functions
            .iter()
            .filter(|func| is_entry(func))
            .map(|func| {
                EvmMemoryLayout::HEAP_START
                    + func.internal_frame_size.max(func.external_static_return_size)
            })
            .max()
            .unwrap_or(EvmMemoryLayout::HEAP_START);

        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !is_entry(func) || has_msize(func) {
                continue;
            }
            changed |= run_on_entry(func, shadow);
        }
        changed
    }
}

/// Pass that defers eligible allocations until exact backend layout is known.
pub(crate) struct DeferAlloc;

impl MirPass for DeferAlloc {
    fn name(&self) -> &'static str {
        "defer-alloc"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut candidates = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            candidates.extend(
                eligible_static_allocations(func)
                    .into_iter()
                    .map(|candidate| (func_id, candidate.alloc)),
            );
        }

        let mut changed = false;
        for (func_id, alloc) in candidates {
            let metadata = &mut module.functions[func_id].inst_mut(alloc).metadata;
            if !metadata.deferred_alloc() {
                metadata.set_deferred_alloc();
                changed = true;
            }
        }
        changed
    }
}

fn is_entry(func: &Function) -> bool {
    !func.attributes.is_constructor
        && (func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback)
}

fn run_on_entry(func: &mut Function, shadow: u64) -> bool {
    apply_candidates(func, &eligible_static_allocations(func), shadow)
}

/// One constant-size allocation eligible for static placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StaticAllocCandidate {
    alloc: InstId,
    ptr: ValueId,
    size: u64,
}

/// Returns constant-size, non-escaping allocations that the backend may place
/// in an entry-local static region.
fn eligible_static_allocations(func: &Function) -> Vec<StaticAllocCandidate> {
    if !is_entry(func) || has_msize(func) {
        return Vec::new();
    }

    let cfg = CfgInfo::new(func);
    let mut candidates = Vec::new();
    for block in func.blocks.indices() {
        for &alloc in &func.blocks[block].instructions {
            let InstKind::Alloc { size, semantics, .. } = func.inst(alloc).kind else {
                continue;
            };
            if semantics != crate::mir::AllocationSemantics::INTERNAL {
                continue;
            }
            let Some(size) = func.value_u64(size) else { continue };
            if size == 0
                || size > 0x1000
                || !size.is_multiple_of(32)
                || !cfg.is_reachable(block)
                || cfg.cyclic_blocks().contains(block)
            {
                continue;
            }
            let Some(ptr) = func.inst_result_value(alloc) else { continue };
            candidates.push(StaticAllocCandidate { alloc, ptr, size });
        }
    }
    if candidates.is_empty() {
        return candidates;
    }
    let uses = ValueUses::new(func);
    // The bounded-use proof rejects every unrecognized use, so it also proves non-escape.
    candidates.retain(|candidate| candidate_uses_are_safe(func, candidate, &uses));
    candidates
}

fn has_msize(func: &Function) -> bool {
    func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize))
}

struct ValueUses {
    instructions: IndexVec<ValueId, Vec<InstId>>,
    terminators: IndexVec<ValueId, Vec<BlockId>>,
}

impl ValueUses {
    fn new(func: &Function) -> Self {
        let mut instructions = index_vec![Vec::new(); func.values.len()];
        let mut terminators = index_vec![Vec::new(); func.values.len()];
        for inst_id in func.instructions() {
            for operand in func.inst(inst_id).operands() {
                instructions[operand].push(inst_id);
            }
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    terminators[operand].push(block_id);
                }
            }
        }
        Self { instructions, terminators }
    }
}

/// Verifies every use of the pointer stays in bounds and never escapes.
fn candidate_uses_are_safe(func: &Function, cand: &StaticAllocCandidate, uses: &ValueUses) -> bool {
    // Discover in-bounds address derivations from the pointer through def-use
    // edges so definition order does not matter.
    let mut derived: FxHashMap<ValueId, u64> = FxHashMap::default();
    derived.insert(cand.ptr, 0);
    let mut pending = vec![cand.ptr];
    while let Some(value) = pending.pop() {
        for &inst_id in &uses.instructions[value] {
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            if derived.contains_key(&result) {
                continue;
            }
            let InstKind::Add(a, b) = func.inst(inst_id).kind else { continue };
            let (base, offset) = if derived.contains_key(&a) { (a, b) } else { (b, a) };
            let (Some(base_off), Some(off)) = (derived.get(&base).copied(), func.value_u64(offset))
            else {
                return false;
            };
            let Some(total) = base_off.checked_add(off) else { return false };
            if total >= cand.size {
                return false;
            }
            derived.insert(result, total);
            pending.push(result);
        }
    }

    // Every use of every derived address must be a bounded memory access.
    let in_range = |off: u64, len: u64| off.checked_add(len).is_some_and(|end| end <= cand.size);
    for (&operand, &off) in &derived {
        for &inst_id in &uses.instructions[operand] {
            if inst_id == cand.alloc {
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            let ok = match *kind {
                InstKind::MLoad(addr) => operand == addr && in_range(off, 32),
                InstKind::MStore(addr, value) => {
                    operand == addr && value != operand && in_range(off, 32)
                }
                InstKind::Keccak256(addr, size)
                | InstKind::Log0(addr, size)
                | InstKind::CalldataCopy(addr, _, size)
                | InstKind::ReturnDataCopy(addr, _, size)
                | InstKind::CodeCopy(addr, _, size) => {
                    operand == addr && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                InstKind::Log1(addr, size, _)
                | InstKind::Log2(addr, size, _, _)
                | InstKind::Log3(addr, size, _, _, _)
                | InstKind::Log4(addr, size, _, _, _, _) => {
                    operand == addr && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                InstKind::MCopy(dest, src, size) => {
                    (operand == dest || operand == src)
                        && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                // In-bounds derivations were collected above; anything
                // else consuming an address is an escape.
                InstKind::Add(_, _) => {
                    func.inst_result_value(inst_id).is_some_and(|r| derived.contains_key(&r))
                }
                _ => false,
            };
            if !ok {
                return false;
            }
        }
        for &block in &uses.terminators[operand] {
            let term = func.blocks[block].terminator.as_ref().expect("indexed terminator use");
            let ok = match term {
                Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                    operand == *offset
                        && func.value_u64(*size).is_some_and(|len| in_range(off, len))
                }
                _ => false,
            };
            if !ok {
                return false;
            }
        }
    }

    true
}

/// Rewrites eligible allocations using the conservative placement retained for
/// the explicit `static-alloc` MIR pass.
fn apply_candidates(func: &mut Function, candidates: &[StaticAllocCandidate], shadow: u64) -> bool {
    let mut replacements = FxHashMap::default();
    let mut dead = DenseBitSet::new_empty(func.num_insts());

    for cand in candidates {
        // The region lives past the locals and the static return buffer. It
        // must stay inside the tallest entry's shadow — growing past it pushes
        // the shared static-frame region and can widen every helper and spill
        // push behind it — and must not drag this entry's own spill base across
        // the one-byte address boundary.
        let base = EvmMemoryLayout::HEAP_START
            + func.internal_frame_size.max(func.external_static_return_size);
        if base + cand.size > shadow || (base < 0x100 && base + cand.size > 0x100) {
            continue;
        }
        func.internal_frame_size = (base - EvmMemoryLayout::HEAP_START) + cand.size;
        let replacement = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(base))));
        replacements.insert(cand.ptr, replacement);
        dead.insert(cand.alloc);
    }

    if replacements.is_empty() {
        return false;
    }
    func.replace_uses_canonicalized(&replacements);
    for block in &mut func.blocks {
        block.instructions.retain(|&inst| !dead.contains(inst));
    }
    true
}
