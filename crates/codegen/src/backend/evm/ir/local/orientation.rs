//! Final literal/binary operand orientation on physical block instructions.
//!
//! A literal followed by SWAP1/SWAP2 and a reversible binary operation can
//! instead swap the incoming pair, push the literal, and reverse the operation's
//! operands. This removes one byte and three gas with identical required inputs,
//! output order and peak height. Running after structural transforms and layout
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
    literal_observers_allow, swapped, verify,
};
use solar_sema::Gcx;

pub(crate) struct LiteralOrientation;

impl EvmPass for LiteralOrientation {
    fn name(&self) -> &'static str {
        "literal-orientation"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !module.block_ids().any(|id| {
            module.blocks[id].insts.windows(4).any(|insts| matched_opcode(insts).is_some())
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

fn matched_opcode(insts: &[Instruction]) -> Option<u8> {
    if let [literal, first, second, binary, ..] = insts
        && matches!(literal.kind, InstKind::Push(_))
        && matches!(first.kind, InstKind::Swap(1))
        && matches!(second.kind, InstKind::Swap(2))
        && let InstKind::Op(code) = binary.kind
        && insts[..4].iter().all(canonical)
    {
        swapped(code)
    } else {
        None
    }
}

pub(super) fn orient(insts: &mut Vec<Instruction>) -> bool {
    let mut read = 0;
    let mut write = 0;
    while read < insts.len() {
        if let Some(code) = matched_opcode(&insts[read..]) {
            // push literal; swap1; swap2; binary
            // -> swap1; push literal; swapped binary
            insts.swap(read, read + 1);
            insts[read + 3].kind = InstKind::Op(code);
            for offset in [0, 1, 3] {
                insts.swap(write, read + offset);
                write += 1;
            }
            read += 4;
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
