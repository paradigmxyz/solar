//! Static placement of provably local heap allocations.
//!
//! Lowering allocates dynamic memory by bumping the free-memory pointer:
//! `ptr = mload(0x40); ...; mstore(0x40, ptr + K)`. When `K` is a constant,
//! the allocating block executes at most once per call, and the pointer never
//! escapes the function, the allocation does not need the free-memory pointer
//! at all: the region can live at a compile-time address appended to the
//! entry's local frame. The load and the bump disappear, every use keeps its
//! shape, and the frame layout machinery accounts for the enlarged frame.
//!
//! This is the first slice of an explicit allocation model: the rewrite
//! performs exactly the escape analysis an `alloc(size)` instruction would
//! make routine, without yet changing the MIR vocabulary.
//!
//! Safety contract:
//! - external entries only: their locals are absolute low-memory addresses;
//! - the paired load and bump sit in one block with no intervening operation that can read or move
//!   the free-memory pointer;
//! - the block cannot re-execute, so the reused static region cannot expose a previous iteration's
//!   contents where fresh zeroed memory was expected;
//! - the bump's sum feeds nothing but the bump itself, and every other use of the pointer is an
//!   in-bounds address derivation into exact loads, stores, hashes, copies, logs, or external-data
//!   terminators — the pointer value never escapes into stored data, call arguments, or unbounded
//!   arithmetic;
//! - functions observing `msize` are skipped: eliding a bump changes the high-water mark.

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, Immediate, InstId, InstKind, Module, Terminator, Value, ValueId},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Pass that places provably local fmp-bump allocations statically.
pub(crate) struct StaticAllocPass;

impl MirPass for StaticAllocPass {
    fn name(&self) -> &'static str {
        "static-alloc"
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        // Every entry's locals share the same low-memory region — only one
        // entry runs per call — so the tallest entry's frame top is a shadow
        // the others can grow into without moving the shared static-frame
        // region or any spill base above it. Placements stay inside it.
        let shadow = module
            .functions
            .iter()
            .filter(|func| is_entry(func))
            .map(|func| 0x80 + func.internal_frame_size.max(func.external_static_return_size))
            .max()
            .unwrap_or(0x80);

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

fn is_entry(func: &Function) -> bool {
    !func.attributes.is_constructor
        && (func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback)
}

fn run_on_entry(func: &mut Function, shadow: u64) -> bool {
    if !func
        .instructions
        .iter()
        .any(|inst| matches!(&inst.kind, InstKind::MLoad(addr) if is_fmp_addr(func, *addr)))
    {
        return false;
    }

    let inst_results = func.inst_results();
    let mut candidates = Vec::new();
    for block_id in func.blocks.indices() {
        collect_block_candidates(func, block_id, &inst_results, &mut candidates);
    }
    if candidates.is_empty() {
        return false;
    }

    let cfg = CfgInfo::new(func);
    let mut cyclic = FxHashMap::default();
    candidates.retain(|cand| {
        cfg.is_reachable(cand.block)
            && !*cyclic.entry(cand.block).or_insert_with(|| block_in_cycle(func, cand.block))
    });

    let mut changed = false;
    for cand in candidates {
        changed |= apply_candidate(func, &cand, shadow);
    }
    changed
}

/// One `ptr = mload(0x40) ... mstore(0x40, ptr + size)` pair.
struct Candidate {
    block: BlockId,
    load: InstId,
    ptr: ValueId,
    bump: InstId,
    /// The `ptr + size` sum consumed by the bump store.
    bump_value: ValueId,
    size: u64,
}

fn has_msize(func: &Function) -> bool {
    func.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|&inst| matches!(func.instructions[inst].kind, InstKind::MSize))
    })
}

/// Returns true when `block` can execute more than once: it can reach itself.
fn block_in_cycle(func: &Function, block: BlockId) -> bool {
    let mut stack = vec![block];
    let mut seen = DenseBitSet::new_empty(func.blocks.len());
    while let Some(current) = stack.pop() {
        let Some(term) = func.blocks[current].terminator.as_ref() else { continue };
        for succ in term.successors() {
            if succ == block {
                return true;
            }
            if seen.insert(succ) {
                stack.push(succ);
            }
        }
    }
    false
}

fn is_fmp_addr(func: &Function, value: ValueId) -> bool {
    matches!(func.value(value), Value::Immediate(imm) if imm.as_u256() == Some(U256::from(0x40)))
}

/// Collects load/bump pairs in `block` with no operation between them that
/// can read or move the free-memory pointer.
fn collect_block_candidates(
    func: &Function,
    block_id: BlockId,
    inst_results: &FxHashMap<InstId, ValueId>,
    out: &mut Vec<Candidate>,
) {
    let mut open: Option<(InstId, ValueId)> = None;
    for &inst_id in &func.blocks[block_id].instructions {
        match &func.instructions[inst_id].kind {
            InstKind::MLoad(addr) if is_fmp_addr(func, *addr) => {
                // A new fmp read closes any open pair unmatched.
                open = inst_results.get(&inst_id).map(|&ptr| (inst_id, ptr));
            }
            InstKind::MStore(addr, value) if is_fmp_addr(func, *addr) => {
                if let Some((load, ptr)) = open.take()
                    && let Some(size) = bump_size(func, *value, ptr)
                    && size > 0
                    && size <= 0x1000
                    && size.is_multiple_of(32)
                {
                    out.push(Candidate {
                        block: block_id,
                        load,
                        ptr,
                        bump: inst_id,
                        bump_value: *value,
                        size,
                    });
                }
            }
            // Anything that can call out or allocate can observe or move the
            // free-memory pointer: close the window.
            InstKind::InternalCall { .. }
            | InstKind::Call { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::Create(_, _, _)
            | InstKind::Create2(_, _, _, _) => open = None,
            _ => {}
        }
    }
}

/// Matches `value == add(ptr, K)` (either operand order) and returns `K`.
fn bump_size(func: &Function, value: ValueId, ptr: ValueId) -> Option<u64> {
    let Value::Inst(inst_id) = func.value(value) else { return None };
    let InstKind::Add(a, b) = func.instructions[*inst_id].kind else { return None };
    if a == ptr {
        func.value_u64(b)
    } else if b == ptr {
        func.value_u64(a)
    } else {
        None
    }
}

/// Verifies every use of the pointer stays in bounds and never escapes, then
/// rewrites the pair: pointer uses take the static address, the load and the
/// bump are deleted, and the frame grows by the allocation size.
fn apply_candidate(func: &mut Function, cand: &Candidate, shadow: u64) -> bool {
    let inst_results = func.inst_results();

    // In-bounds address derivations from the pointer, to a fixpoint so
    // definition order does not matter. The bump's own sum is exempt.
    let mut derived: FxHashMap<ValueId, u64> = FxHashMap::default();
    derived.insert(cand.ptr, 0);
    for _ in 0..4 {
        let mut grew = false;
        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if let InstKind::Add(a, b) = func.instructions[inst_id].kind
                    && let Some(&result) = inst_results.get(&inst_id)
                    && result != cand.bump_value
                    && !derived.contains_key(&result)
                {
                    let (base, offset) = if derived.contains_key(&a) {
                        (a, b)
                    } else if derived.contains_key(&b) {
                        (b, a)
                    } else {
                        continue;
                    };
                    let (Some(base_off), Some(off)) =
                        (derived.get(&base).copied(), func.value_u64(offset))
                    else {
                        return false;
                    };
                    let Some(total) = base_off.checked_add(off) else { return false };
                    if total >= cand.size {
                        return false;
                    }
                    derived.insert(result, total);
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    // The bump's sum may feed nothing but the bump store itself.
    let mut bump_value_uses = 0usize;
    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            for &operand in func.instructions[inst_id].kind.operands().iter() {
                if operand == cand.bump_value {
                    bump_value_uses += 1;
                }
            }
        }
        if let Some(term) = &block.terminator {
            for &operand in term.operands().iter() {
                if operand == cand.bump_value {
                    return false;
                }
            }
        }
    }
    if bump_value_uses != 1 {
        return false;
    }

    // Every use of every derived address must be a bounded memory access.
    let in_range = |off: u64, len: u64| off.checked_add(len).is_some_and(|end| end <= cand.size);
    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            if inst_id == cand.bump {
                continue;
            }
            let kind = &func.instructions[inst_id].kind;
            if let Some(&result) = inst_results.get(&inst_id)
                && result == cand.bump_value
            {
                // The bump sum itself; consumed only by the bump store.
                continue;
            }
            for &operand in kind.operands().iter() {
                let Some(&off) = derived.get(&operand) else { continue };
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
                        operand == addr
                            && func.value_u64(size).is_some_and(|len| in_range(off, len))
                    }
                    InstKind::Log1(addr, size, _)
                    | InstKind::Log2(addr, size, _, _)
                    | InstKind::Log3(addr, size, _, _, _)
                    | InstKind::Log4(addr, size, _, _, _, _) => {
                        operand == addr
                            && func.value_u64(size).is_some_and(|len| in_range(off, len))
                    }
                    InstKind::MCopy(dest, src, size) => {
                        (operand == dest || operand == src)
                            && func.value_u64(size).is_some_and(|len| in_range(off, len))
                    }
                    // In-bounds derivations were collected above; anything
                    // else consuming an address is an escape.
                    InstKind::Add(_, _) => {
                        inst_results.get(&inst_id).is_some_and(|r| derived.contains_key(r))
                    }
                    _ => false,
                };
                if !ok {
                    return false;
                }
            }
        }
        if let Some(term) = &block.terminator {
            for &operand in term.operands().iter() {
                let Some(&off) = derived.get(&operand) else { continue };
                let ok = match term {
                    Terminator::Revert { offset, size }
                    | Terminator::ReturnData { offset, size } => {
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
    }

    // Rewrite: the region lives past the locals and the static return
    // buffer. It must stay inside the tallest entry's shadow — growing past
    // it pushes the shared static-frame region and can widen every helper
    // and spill push behind it — and must not drag this entry's own spill
    // base across the one-byte address boundary.
    let base = 0x80 + func.internal_frame_size.max(func.external_static_return_size);
    if base + cand.size > shadow || (base < 0x100 && base + cand.size > 0x100) {
        return false;
    }
    func.internal_frame_size = (base - 0x80) + cand.size;
    let replacement = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(base))));
    let mut replacements = FxHashMap::default();
    replacements.insert(cand.ptr, replacement);
    func.replace_uses_canonicalized(&replacements);
    let block = &mut func.blocks[cand.block];
    block.instructions.retain(|&inst| inst != cand.load && inst != cand.bump);
    true
}
