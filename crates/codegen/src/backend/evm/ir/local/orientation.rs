//! Final literal/binary operand orientation on physical block instructions.
//!
//! A literal followed by SWAP1, deeper swaps and a reversible binary operation can
//! instead perform those deeper swaps one slot shallower, push the literal, and
//! reverse the operation's operands. The literal stays immediately below the top
//! during the original deeper swaps. Moving its push to the consumer removes
//! SWAP1 with identical required inputs, output order and peak height. Remaining
//! swaps cannot become wider. Running after structural transforms and layout
//! preserves the expressions previously selected for CSE and outlining.
//!
//! The exact pattern requires canonical metadata and the shared module-wide
//! literal observer policy, including proved computed transfers. Indexed jumps
//! are deliberately excluded: their later address-width selection can change
//! physical splitting, outside this fixed-layout size proof. A candidate scan
//! precedes control-flow analysis; rewriting compacts each block in place once.
//! No height analysis, scheduler, CFG rewrite or compact-stream transform runs.

use super::{
    super::TerminatorKind, EvmPass, InstKind, Instruction, Module, canonical,
    literal_observers_allow, op, swapped, verify,
};
use solar_sema::Gcx;

pub(crate) struct LiteralOrientation;

impl EvmPass for LiteralOrientation {
    fn name(&self) -> &'static str {
        "literal-orientation"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !module.block_ids().any(|id| {
            let insts = &module.blocks[id].insts;
            (0..insts.len()).any(|start| matched_opcode(&insts[start..]).is_some())
        }) || module
            .block_ids()
            .any(|id| matches!(module.blocks[id].terminator.kind, TerminatorKind::IndexedJump(_)))
            || !literal_observers_allow(module)
            || verify::has_unknown_jump(module)
        {
            return false;
        }
        let mut changed = false;
        for id in module.block_ids().collect::<Vec<_>>() {
            changed |= orient(&mut module.blocks[id].insts);
        }
        changed
    }
}

fn matched_opcode(insts: &[Instruction]) -> Option<(u8, usize)> {
    if let [literal, first, rest @ ..] = insts
        && matches!(literal.kind, InstKind::Push(_))
        && matches!(first.kind, InstKind::Swap(1))
    {
        let count = rest
            .iter()
            .take_while(|inst| match inst.kind {
                InstKind::Swap(depth) => {
                    (2..=16).contains(&depth) || op::encode_depth(depth).is_some()
                }
                _ => false,
            })
            .count();
        let end = count + 2;
        if count != 0
            && let InstKind::Op(code) = insts.get(end)?.kind
            && insts[..=end].iter().all(canonical)
        {
            return swapped(code).map(|opcode| (opcode, end));
        }
    }
    None
}

pub(super) fn orient(insts: &mut Vec<Instruction>) -> bool {
    let mut read = 0;
    let mut write = 0;
    while read < insts.len() {
        if let Some((code, end)) = matched_opcode(&insts[read..]) {
            // push literal; swap1; swap d1; ...; swap dn; binary
            // -> swap (d1-1); ...; swap (dn-1); push literal; swapped binary
            insts[read..read + end].rotate_left(2);
            for inst in &mut insts[read..read + end - 2] {
                let InstKind::Swap(depth) = &mut inst.kind else { unreachable!() };
                *depth -= 1;
            }
            insts[read + end].kind = InstKind::Op(code);
            for offset in (0..end - 1).chain(std::iter::once(end)) {
                insts.swap(write, read + offset);
                write += 1;
            }
            read += end + 1;
        } else {
            // instruction -> instruction
            if write != read {
                insts.swap(write, read);
            }
            write += 1;
            read += 1;
        }
    }
    let changed = write != insts.len();
    insts.truncate(write);
    changed
}
