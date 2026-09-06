//! Structural and stack validation for physical EVM programs.
//!
//! Every instruction is checked independently of reachability. A worklist then
//! propagates physical stack states and label provenance across explicit edges.
//! Known computed destinations contribute edges. Recursive transfers carrying a
//! fresh continuation are checked one activation at a time, retaining suspended
//! prefixes on return labels; their global height remains unknown. Entry bounds
//! permit safe local temporary expansion only where an absolute bound is proved.
//! Unknown computed destinations invalidate absolute incoming bounds throughout
//! the module: a known edge cannot exclude an additional dynamic entry prefix. At most sixteen
//! distinct label states are retained per block and exact height; further states widen to unknown
//! labels. This bounds call-path combinations while preserving structural height growth
//! and prevents optimization from using heights behind unproved return edges.

use super::{BlockId, InstKind, Instruction, Module, TerminatorKind};
use crate::backend::evm::op;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;
use std::collections::VecDeque;

pub(crate) fn validate(gcx: Gcx<'_>, module: &Module) -> Option<StackHeights> {
    let fail = |block: BlockId, message: String| {
        gcx.dcx()
            .err(format!("EVM IR verification failed: block {}: {message}", block.index()))
            .emit();
    };
    let mut invalid_keep = false;
    for id in module.block_ids() {
        let block = &module.blocks[id];
        for (index, inst) in block.insts.iter().enumerate() {
            if !inst.keep_with_next {
                continue;
            }
            let next = block.insts.get(index + 1);
            let name = inst_name(&inst.kind);
            let message = match inst.kind {
                InstKind::Op(op::GAS | op::SUB) if next.is_none() => Some(format!(
                    "`{name}` must be followed by the instruction it is kept with"
                )),
                InstKind::Op(op::GAS) => next.and_then(|next| {
                    (next.kind != InstKind::Op(op::SUB) || !next.keep_with_next).then(|| format!(
                        "`gas` is kept with the next instruction, which must be a `sub` kept with the call, not `{}`",
                        inst_name(&next.kind)
                    ))
                }),
                InstKind::Op(op::SUB) => next.and_then(|next| {
                    (!matches!(next.kind, InstKind::Op(op::CALL | op::CALLCODE | op::DELEGATECALL | op::STATICCALL))).then(|| format!(
                        "`sub` is kept with the next instruction, which must be a call, not `{}`",
                        inst_name(&next.kind)
                    ))
                }),
                _ => Some(format!("`{name}` cannot be kept with a next instruction")),
            };
            if let Some(message) = message {
                fail(id, message);
                invalid_keep = true;
            }
        }
        if block.terminator.keep_with_next {
            fail(
                id,
                format!(
                    "terminator `{}` cannot be kept with a next instruction",
                    term_name(&block.terminator.kind)
                ),
            );
            invalid_keep = true;
        }
    }
    if invalid_keep {
        return None;
    }
    for id in module.block_ids() {
        let block = &module.blocks[id];
        for inst in &block.insts {
            let expected = effect(&inst.kind);
            if let Some((inputs, outputs)) = expected {
                if let Some(actual) = inst.stack_effect
                    && actual != (inputs, outputs)
                {
                    let name = inst_name(&inst.kind);
                    fail(
                        id,
                        format!(
                            "`{name}` has stack effect {}->{}, expected {inputs}->{outputs}",
                            actual.0, actual.1
                        ),
                    );
                    return None;
                }
            } else if inst.stack_effect.is_none() {
                let name = inst_name(&inst.kind);
                fail(id, format!("instruction `{name}` must declare an explicit stack effect"));
                return None;
            }
            match inst.kind {
                InstKind::Op(opcode) if (op::PUSH1..=op::PUSH32).contains(&opcode) => {
                    let name = inst_name(&inst.kind);
                    fail(id, format!("`{name}` must carry an encoded push value"));
                    return None;
                }
                InstKind::PushData { id: data_id, offset } => {
                    let Some(data) = module.data.get(data_id) else {
                        fail(id, "unknown program data identifier".into());
                        return None;
                    };
                    if offset as usize > data.bytes.len() {
                        fail(
                            id,
                            format!(
                                "program data offset `{offset}` exceeds data size `{}`",
                                data.bytes.len()
                            ),
                        );
                        return None;
                    }
                }
                InstKind::PushLabel(target) if module.blocks.get(target).is_none() => {
                    fail(id, "unknown block reference".into());
                    return None;
                }
                InstKind::PushImmutable { width, .. } if !(1..=32).contains(&width) => {
                    fail(id, "immutable width must be between 1 and 32 bytes".into());
                    return None;
                }
                InstKind::Dup(0) | InstKind::Swap(0) => {
                    let name = inst_name(&inst.kind);
                    fail(id, format!("`{name}` depth must be positive"));
                    return None;
                }
                _ => {}
            }
        }
        let expected = term_effect(&block.terminator.kind);
        if let Some(actual) = block.terminator.stack_effect
            && actual != expected
        {
            fail(
                id,
                format!(
                    "`{}` has stack effect {}->{}, expected {}->{}",
                    term_name(&block.terminator.kind),
                    actual.0,
                    actual.1,
                    expected.0,
                    expected.1
                ),
            );
            return None;
        }
        for target in successors(&block.terminator.kind) {
            if module.blocks.get(target).is_none() {
                fail(id, "unknown block reference".into());
                return None;
            }
        }
    }

    if !gcx.sess.opts.evm_version.has_extended_stack_ops() {
        let mut unavailable = false;
        for id in module.block_ids() {
            let block = &module.blocks[id];
            for inst in &block.insts {
                let name = match inst.kind {
                    InstKind::Dup(depth) if depth > 16 => Some("dup"),
                    InstKind::Swap(depth) if depth > 16 => Some("swap"),
                    InstKind::Exchange(_, depth) if depth > 16 => Some("exchange"),
                    _ => None,
                };
                if let Some(name) = name {
                    gcx.dcx().err(format!("EVM IR verification failed: block {}: `{name}` requires Amsterdam-compatible EVM", module.block_label(id))).emit();
                    unavailable = true;
                }
            }
        }
        if unavailable {
            return None;
        }
    }

    match stack_analysis(module) {
        Ok((heights, _)) => Some(heights),
        Err((id, message)) => {
            fail(id, message);
            None
        }
    }
}

/// Minimum and maximum proved entry heights; `None` means no entry proof.
pub(crate) type StackHeights = IndexVec<BlockId, Option<(usize, usize)>>;

/// Computes entry-height bounds without emitting diagnostics or changing IR.
///
/// Unknown computed edges end the proof. Callers must conservatively avoid
/// increasing local stack peaks for blocks with no entry proof.
pub(crate) fn stack_heights(module: &Module) -> Result<StackHeights, (BlockId, String)> {
    stack_facts(module).map(|(heights, _)| heights)
}

/// Computes optimization bounds and unknown-transfer status in one analysis.
/// Encoding validation separately retains the raw bounds of concrete paths.
pub(super) fn stack_facts(module: &Module) -> Result<(StackHeights, bool), (BlockId, String)> {
    stack_analysis(module).map(|(mut heights, unknown)| {
        if unknown {
            heights.raw.fill(None);
        }
        (heights, unknown)
    })
}

/// Whether an executed computed transfer has no proved label destination.
/// Failed proofs also prevent deleting blocks based on incomplete control flow.
pub(super) fn has_unknown_jump(module: &Module) -> bool {
    stack_analysis(module).map_or(true, |(_, unknown)| unknown)
}

fn stack_analysis(module: &Module) -> Result<(StackHeights, bool), (BlockId, String)> {
    let mut bounds =
        IndexVec::<BlockId, Option<(usize, usize)>>::from_vec(vec![None; module.blocks.len()]);
    let mut pending = VecDeque::new();
    let mut unknown_jump = false;
    let mut states =
        FxHashMap::<(BlockId, usize), Option<FxHashSet<Vec<Option<(BlockId, usize)>>>>>::default();
    let mut prototypes = FxHashMap::<BlockId, Vec<Option<(BlockId, usize)>>>::default();
    // The physical graph is immutable throughout this analysis.
    let mut recursive = FxHashMap::default();
    let mut unproved = DenseBitSet::new_empty(module.blocks.len());
    if let Some(entry) = module.block_ids().next() {
        pending.push_back((entry, Vec::new()));
    }
    while let Some((id, mut stack)) = pending.pop_front() {
        // A fixed height admits a bounded number of precise label contexts. Beyond
        // that, unknown labels subsume every context without changing the height.
        let contexts =
            states.entry((id, stack.len())).or_insert_with(|| Some(FxHashSet::default()));
        let Some(precise) = contexts else {
            forget_destinations(module, &stack, &mut unproved);
            continue;
        };
        if !precise.insert(stack.clone()) {
            continue;
        }
        if precise.len() > 16 {
            mark_reachable(module, id, &mut unproved);
            for state in precise.iter() {
                forget_destinations(module, state, &mut unproved);
            }
            *contexts = None;
            stack.fill(None);
        }
        prototypes.entry(id).or_insert_with(|| stack.clone());
        let entry = stack.len();
        match &mut bounds[id] {
            Some((min, max)) => {
                *min = (*min).min(entry);
                *max = (*max).max(entry);
            }
            bounds @ None => *bounds = Some((entry, entry)),
        }
        let block = &module.blocks[id];
        for inst in &block.insts {
            let height = stack.len();
            match inst.kind {
                InstKind::Dup(depth) => {
                    if depth as usize > height {
                        return Err((
                            id,
                            format!(
                                "`dup {depth}` reaches depth {depth} but the stack has {height}"
                            ),
                        ));
                    }
                    stack.push(stack[height - depth as usize]);
                }
                InstKind::Swap(depth) => {
                    if depth as usize >= height {
                        return Err((
                            id,
                            format!(
                                "`swap {depth}` reaches depth {depth} but the stack has {height}"
                            ),
                        ));
                    }
                    stack.swap(height - 1, height - 1 - depth as usize);
                }
                InstKind::Exchange(a, b) => {
                    if b as usize >= height || a == 0 || a >= b {
                        return Err((
                            id,
                            format!(
                                "`exchange {a}, {b}` reaches outside the stack of {height} words"
                            ),
                        ));
                    }
                    stack.swap(height - 1 - a as usize, height - 1 - b as usize);
                }
                _ => {
                    let (inputs, outputs) =
                        effect(&inst.kind).or(inst.stack_effect).ok_or_else(|| {
                            let name = inst_name(&inst.kind);
                            (
                                id,
                                format!(
                                    "instruction `{name}` must declare an explicit stack effect"
                                ),
                            )
                        })?;
                    if inputs as usize > height {
                        return Err((id, underflow(&inst_name(&inst.kind), height, inputs)));
                    }
                    let jump_target = if inst.kind == InstKind::Op(op::JUMPI) {
                        stack.last().copied().flatten()
                    } else {
                        None
                    };
                    if inst.kind == InstKind::Op(op::JUMPI) && jump_target.is_none() {
                        unknown_jump = true;
                    }
                    stack.truncate(height - inputs as usize);
                    stack.resize(stack.len() + outputs as usize, None);
                    if let InstKind::PushLabel(target) = inst.kind {
                        *stack.last_mut().unwrap() = Some((target, 0));
                    }
                    if let Some((target, prefix)) = jump_target {
                        let mut outgoing = vec![None; prefix];
                        outgoing.extend_from_slice(&stack);
                        pending.push_back((target, outgoing));
                    }
                }
            }
            if stack.len() > 1024 {
                let name = inst_name(&inst.kind);
                return Err((
                    id,
                    format!(
                        "`{name}` grows the stack to {} words, exceeding the limit of 1024",
                        stack.len()
                    ),
                ));
            }
        }
        let (inputs, _) = term_effect(&block.terminator.kind);
        if inputs as usize > stack.len() {
            return Err((id, underflow(term_name(&block.terminator.kind), stack.len(), inputs)));
        }
        let dynamic = if matches!(block.terminator.kind, TerminatorKind::DynamicJump) {
            stack.last().copied().flatten()
        } else {
            None
        };
        if block.terminator.kind == TerminatorKind::DynamicJump && dynamic.is_none() {
            unknown_jump = true;
        }
        stack.truncate(stack.len() - inputs as usize);
        for target in successors(&block.terminator.kind) {
            let mut outgoing = stack.clone();
            if let Some(prototype) = prototypes.get(&target)
                && outgoing.len() > prototype.len()
                && outgoing
                    .iter()
                    .flatten()
                    .any(|(label, _)| !prototype.iter().flatten().any(|(old, _)| label == old))
                && *recursive
                    .entry((target, id))
                    .or_insert_with(|| recursive_transfer(module, target, id))
            {
                // A recursive physical transfer adds a fresh continuation above a
                // suspended prefix. Prove one activation and remember that prefix
                // on its continuation; runtime recursion depth remains unbounded.
                let prefix = outgoing.len() - prototype.len();
                if let Some(position) = outgoing.iter().rposition(Option::is_some)
                    && position >= prefix
                {
                    outgoing[position].as_mut().unwrap().1 += prefix;
                    outgoing.drain(..prefix);
                    mark_reachable(module, target, &mut unproved);
                }
            }
            pending.push_back((target, outgoing));
        }
        if let Some((target, prefix)) = dynamic {
            let mut outgoing = vec![None; prefix];
            outgoing.extend(stack);
            if prefix > 0 {
                mark_reachable(module, target, &mut unproved);
            }
            pending.push_back((target, outgoing));
        }
    }
    for id in unproved.iter() {
        bounds[id] = None;
    }
    Ok((bounds, unknown_jump))
}

fn physical_successors(module: &Module, id: BlockId) -> impl Iterator<Item = BlockId> + '_ {
    let block = &module.blocks[id];
    successors(&block.terminator.kind).chain(block.insts.iter().filter_map(|inst| {
        if let InstKind::PushLabel(target) = inst.kind { Some(target) } else { None }
    }))
}

fn recursive_transfer(module: &Module, target: BlockId, source: BlockId) -> bool {
    let mut seen = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = vec![target];
    let mut returning = false;
    while let Some(id) = pending.pop() {
        if seen.insert(id) {
            returning |= module.blocks[id].terminator.kind == TerminatorKind::DynamicJump;
            pending.extend(physical_successors(module, id));
        }
    }
    seen.contains(source) && returning
}

fn mark_reachable(module: &Module, entry: BlockId, seen: &mut DenseBitSet<BlockId>) {
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if seen.insert(id) {
            pending.extend(physical_successors(module, id));
        }
    }
}

// Caller continuations may have been pushed before the widened block. Retain
// their unknown-height status too, even if no local reference exposes the edge.
fn forget_destinations(
    module: &Module,
    stack: &[Option<(BlockId, usize)>],
    unproved: &mut DenseBitSet<BlockId>,
) {
    for &(target, _) in stack.iter().flatten() {
        mark_reachable(module, target, unproved);
    }
}

fn underflow(name: &str, height: usize, inputs: u8) -> String {
    if name == "pop" && height == 0 {
        "`pop` on an empty stack".into()
    } else {
        format!("`{name}` requires {inputs} stack words but the stack has {height}")
    }
}

pub(crate) fn effect(kind: &InstKind) -> Option<(u8, u8)> {
    match kind {
        InstKind::Op(opcode) => op::stack_io(*opcode),
        InstKind::Push(_)
        | InstKind::PushLabel(_)
        | InstKind::PushData { .. }
        | InstKind::PushDeferred(_)
        | InstKind::PushImmutable { .. } => Some((0, 1)),
        InstKind::Dup(_) => Some((0, 1)),
        InstKind::Swap(_) | InstKind::Exchange(..) => Some((0, 0)),
    }
}

pub(crate) fn term_effect(kind: &TerminatorKind) -> (u8, u8) {
    match kind {
        TerminatorKind::JumpI(..)
        | TerminatorKind::DynamicJump
        | TerminatorKind::IndexedJump(_)
        | TerminatorKind::SelfDestruct => (1, 0),
        TerminatorKind::Return | TerminatorKind::Revert => (2, 0),
        _ => (0, 0),
    }
}

pub(crate) fn successors(kind: &TerminatorKind) -> impl Iterator<Item = BlockId> + '_ {
    let (targets, other) = match kind {
        TerminatorKind::Jump(target) => (std::slice::from_ref(target), None),
        TerminatorKind::JumpI(yes, no) => (std::slice::from_ref(yes), Some(*no)),
        TerminatorKind::IndexedJump(targets) => (targets.as_slice(), None),
        _ => (&[][..], None),
    };
    targets.iter().copied().chain(other)
}

fn inst_name(kind: &InstKind) -> String {
    match kind {
        InstKind::Op(opcode) => op::name(*opcode)
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| format!("op_{opcode:02x}")),
        InstKind::Push(_) | InstKind::PushLabel(_) => "push".into(),
        InstKind::PushData { .. } => "push_data".into(),
        InstKind::PushDeferred(_) => "push_deferred".into(),
        InstKind::PushImmutable { .. } => "push_immutable".into(),
        InstKind::Dup(_) => "dup".into(),
        InstKind::Swap(_) => "swap".into(),
        InstKind::Exchange(..) => "exchange".into(),
    }
}

fn term_name(kind: &TerminatorKind) -> &'static str {
    match kind {
        TerminatorKind::Jump(_) | TerminatorKind::DynamicJump => "jump",
        TerminatorKind::JumpI(..) => "jumpi",
        TerminatorKind::IndexedJump(_) => "indexed_jump",
        TerminatorKind::Stop => "stop",
        TerminatorKind::Return => "return",
        TerminatorKind::Revert => "revert",
        TerminatorKind::Invalid => "invalid",
        TerminatorKind::SelfDestruct => "selfdestruct",
        TerminatorKind::Unreachable => "unreachable",
    }
}

/// Checks physical opcode availability after required legalization.
pub(super) fn validate_target(gcx: Gcx<'_>, module: &Module) {
    for id in module.block_ids() {
        let block = &module.blocks[id];
        for inst in &block.insts {
            if let InstKind::Op(opcode) = inst.kind
                && op::name(opcode).is_some()
                && !op::available(opcode, gcx.sess.opts.evm_version)
            {
                gcx.dcx().err(format!("EVM IR verification failed: block {}: opcode `{}` is unavailable for `{}` EVM", id.index(), inst_name(&inst.kind), gcx.sess.opts.evm_version)).emit();
                return;
            }
        }
    }
}

/// Checks that a local rewrite neither reaches below its original inputs nor
/// grows beyond the proved physical stack limit. Unknown incoming prefixes can
/// only accept rewrites that do not increase the original local peak. Proven
/// unreachable blocks may expand: no execution enters them. Reachability includes
/// every physical label reference, including continuations used by computed jumps.
/// Callers may reuse the analyses across rewrites that preserve the original
/// blocks' incoming stack heights and reachability.
pub(crate) fn rewrite_fits(
    module: &Module,
    heights: &StackHeights,
    reachable: &DenseBitSet<BlockId>,
    block: BlockId,
    start: usize,
    end: usize,
    replacement: &[Instruction],
) -> bool {
    let insts = &module.blocks[block].insts;
    if !super::split_allowed(insts, start) || !super::split_allowed(insts, end) {
        return false;
    }
    let Some((old_need, old_peak, _)) = local_profile(&module.blocks[block].insts[start..end])
    else {
        return false;
    };
    let Some((new_need, new_peak, _)) = local_profile(replacement) else { return false };
    if new_need > old_need {
        return false;
    }
    if new_peak <= old_peak {
        return true;
    }
    let Some((_, _, delta)) = local_profile(&module.blocks[block].insts[..start]) else {
        return false;
    };
    if delta + new_peak > 1024 {
        return false;
    }
    let Some((_, incoming)) = heights[block] else {
        return !reachable.contains(block);
    };
    incoming as isize + delta + new_peak <= 1024
}

fn local_profile(insts: &[Instruction]) -> Option<(isize, isize, isize)> {
    let mut height = 0isize;
    let mut need = 0isize;
    let mut peak = 0isize;
    for inst in insts {
        let (inputs, outputs) = effect(&inst.kind).or(inst.stack_effect)?;
        let access = match inst.kind {
            InstKind::Dup(depth) => depth as isize,
            InstKind::Swap(depth) | InstKind::Exchange(_, depth) => depth as isize + 1,
            _ => inputs as isize,
        };
        need = need.max(access - height);
        height += outputs as isize - inputs as isize;
        peak = peak.max(height);
    }
    Some((need, peak, height))
}

/// Computes the closure of structural edges and physical label references.
/// A computed transfer may also enter globally exposed labels. Conservatively
/// include every globally referenced target and block containing a JUMPDEST.
/// Unreferenced blocks without a JUMPDEST still cannot be entered. This query
/// does not repeat the more expensive label-stack analysis.
pub(crate) fn physical_reachability(module: &Module) -> DenseBitSet<BlockId> {
    let mut reachable = DenseBitSet::new_empty(module.blocks.len());
    if let Some(entry) = module.block_ids().next() {
        mark_reachable(module, entry, &mut reachable);
    }
    if reachable.iter().any(|id| {
        module.blocks[id].terminator.kind == TerminatorKind::DynamicJump
            || module.blocks[id].insts.iter().any(|inst| inst.kind == InstKind::Op(op::JUMPI))
    }) {
        for id in module.block_ids() {
            for target in physical_successors(module, id) {
                mark_reachable(module, target, &mut reachable);
            }
            if module.blocks[id].insts.iter().any(|inst| inst.kind == InstKind::Op(op::JUMPDEST)) {
                mark_reachable(module, id, &mut reachable);
            }
        }
    }
    reachable
}

/// Validates transient words introduced while encoding control transfers.
///
/// Layout is final here, including wide-table splitting. Direct fallthroughs
/// need no address push. Conditional and packed-index jumps need one temporary
/// word before consuming their condition/index. Unknown recursive prefixes have
/// no absolute bound and must not be assigned an invented height. Supplied heights
/// must be raw validation facts for this exact immutable module, not optimization
/// bounds that discard concrete paths when other control transfers are unknown.
pub(super) fn validate_encoding(
    gcx: Gcx<'_>,
    module: &Module,
    heights: Option<&StackHeights>,
) -> solar_interface::Result<()> {
    let fail = |id: BlockId, message: String| {
        gcx.dcx()
            .err(format!("EVM IR verification failed: block {}: {message}", module.block_label(id)))
            .emit()
    };
    // A concrete overflowing path remains invalid even when another dynamic
    // entry prevents treating its observed height as an optimization bound.
    let computed;
    let heights = if let Some(heights) = heights {
        heights
    } else {
        computed = stack_analysis(module).map_err(|(id, message)| fail(id, message))?.0;
        &computed
    };
    let order = module.block_ids().collect::<Vec<_>>();
    for (position, &id) in order.iter().enumerate() {
        let block = &module.blocks[id];
        let extra = match block.terminator.kind {
            TerminatorKind::Jump(target) => usize::from(order.get(position + 1) != Some(&target)),
            TerminatorKind::JumpI(..) | TerminatorKind::IndexedJump(_) => 1,
            _ => 0,
        };
        if extra > 0
            && let Some((_, incoming)) = heights[id]
            && let Some((_, _, delta)) = local_profile(&block.insts)
        {
            let encoded_peak = incoming as isize + delta + extra as isize;
            if encoded_peak > 1024 {
                return Err(fail(
                    id,
                    format!(
                        "`{}` encoding grows the stack to {encoded_peak} words, exceeding the limit of 1024",
                        term_name(&block.terminator.kind)
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BlockId, TerminatorKind, successors};

    #[test]
    fn successor_order_and_duplicates() {
        let a = BlockId::new(7);
        let b = BlockId::new(2);
        let cases = [
            (TerminatorKind::Jump(a), vec![a]),
            (TerminatorKind::JumpI(a, b), vec![a, b]),
            (TerminatorKind::JumpI(a, a), vec![a, a]),
            (TerminatorKind::IndexedJump(vec![]), vec![]),
            (TerminatorKind::IndexedJump(vec![b]), vec![b]),
            (TerminatorKind::IndexedJump(vec![a, b, a]), vec![a, b, a]),
            (TerminatorKind::DynamicJump, vec![]),
            (TerminatorKind::Stop, vec![]),
            (TerminatorKind::Return, vec![]),
            (TerminatorKind::Revert, vec![]),
            (TerminatorKind::Invalid, vec![]),
            (TerminatorKind::SelfDestruct, vec![]),
            (TerminatorKind::Unreachable, vec![]),
        ];
        for (kind, expected) in cases {
            let mut actual = successors(&kind);
            for target in expected {
                assert_eq!(actual.next(), Some(target), "{kind:?}");
            }
            assert_eq!(actual.next(), None, "{kind:?}");
            assert_eq!(actual.next(), None, "{kind:?}");
        }
    }
}
