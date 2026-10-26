//! Shared utilities for EVM IR transforms.
//!
//! Physical block reordering must preserve block identity from the perspective
//! of the rest of the IR. The helpers here rebuild block storage and remap every
//! entry, push, and terminator reference together.

use crate::{
    backend::evm::{
        ir::{BlockId, Instruction, Module, PushValue, TerminatorKind},
        op,
    },
    target::Target,
};
use solar_data_structures::{index::IndexVec, map::FxHashSet};
use solar_sema::Gcx;
use std::hash::{Hash, Hasher};

pub(super) use crate::backend::evm::ir::{remap_block_order, retain_blocks};

/// The machine-level identity shared by transforms that compare instructions.
///
/// `keep_with_next` is part of the identity: sharing one copy of two otherwise equal instructions
/// must not drop one copy's constraint on the boundary that follows it. The small fields share
/// one word so the suffix and outlining tables hash them together.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct MachineInstKey(u64, Option<PushValue>);

impl MachineInstKey {
    pub(super) fn new(inst: &Instruction) -> Self {
        let (stack_kind, first, second) = match inst.as_stack_op() {
            None => (0, 0, 0),
            Some(op::StackOp::Dup(depth)) => (1, depth, 0),
            Some(op::StackOp::Swap(depth)) => (2, depth, 0),
            Some(op::StackOp::Exchange(first, second)) => (3, first, second),
            Some(op::StackOp::Pop) => (4, 0, 0),
        };
        let operation = u64::from_le_bytes([
            inst.opcode,
            inst.encoding,
            u8::from(inst.keeps_with_next()),
            stack_kind,
            first,
            second,
            0,
            0,
        ]);
        Self(operation, inst.value)
    }

    /// Feeds this key to `hasher` as whole words. Equal keys feed equal words, and the value's
    /// kind in the free top bytes of the first word keeps different keys' words distinct.
    fn hash_words(self, hasher: &mut impl Hasher) {
        match self.1 {
            None => hasher.write_u64(self.0),
            Some(PushValue::Immediate(value)) => {
                hasher.write_u64(self.0 | 1 << 48);
                for &limb in value.as_limbs() {
                    hasher.write_u64(limb);
                }
            }
            Some(PushValue::Library(library)) => {
                hasher.write_u64(self.0 | 2 << 48);
                hasher.write_u64(library.index() as u64);
            }
            Some(PushValue::Block(block)) => {
                hasher.write_u64(self.0 | 3 << 48);
                hasher.write_u64(block.index() as u64);
            }
            Some(PushValue::Data(data)) => {
                hasher.write_u64(self.0 | 4 << 48);
                hasher.write_u64(data.id.index() as u64 | u64::from(data.offset) << 32);
            }
            Some(PushValue::DataSize(size)) => {
                hasher.write_u64(self.0 | 5 << 48);
                hasher.write_u64(size.data.index() as u64);
                hasher.write_u64(size.addend);
                hasher.write_u64(u64::from(size.aligned));
            }
        }
    }
}

impl Hash for MachineInstKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash_words(state);
    }
}

/// A stack operation as a symbolic stack simulation sees it. A push is a stack operation for this
/// purpose: deleting it deletes the word it pushes.
#[derive(Clone, Copy)]
pub(super) enum SymbolicStackOp {
    Push,
    Physical(op::StackOp),
}

/// Returns the symbolic stack operation of a push or physical stack operation.
pub(super) fn symbolic_stack_op(inst: &Instruction) -> Option<SymbolicStackOp> {
    if inst.is_encoded_push() {
        return Some(SymbolicStackOp::Push);
    }
    inst.as_stack_op().map(SymbolicStackOp::Physical)
}

/// Returns whether the boundary at `index` in `instructions` may be disturbed.
///
/// The boundary right after a `keep_with_next` instruction is not a legal split point, so no
/// transform may start a block, outline a run, share a tail, or insert an instruction there.
pub(super) fn is_split_point(instructions: &[Instruction], index: usize) -> bool {
    debug_assert!(index <= instructions.len());
    index == 0 || !instructions[index - 1].keeps_with_next()
}

/// Allocates unused textual block labels without assuming labels are dense.
pub(super) struct FreshLabels {
    occupied: FxHashSet<u32>,
    next: u32,
}

impl FreshLabels {
    pub(super) fn new(module: &Module) -> Self {
        let occupied = module.blocks.iter().map(|block| block.label).collect::<FxHashSet<_>>();
        let next =
            occupied.iter().copied().max().and_then(|label| label.checked_add(1)).unwrap_or(0);
        Self { occupied, next }
    }

    /// Reserves `count` labels before a transform mutates the module.
    pub(super) fn take(&mut self, count: usize) -> Option<Vec<u32>> {
        (0..count).map(|_| self.next()).collect()
    }

    fn next(&mut self) -> Option<u32> {
        let start = self.next;
        loop {
            let label = self.next;
            self.next = self.next.wrapping_add(1);
            if self.occupied.insert(label) {
                return Some(label);
            }
            if self.next == start {
                return None;
            }
        }
    }
}

/// Returns a conservative lower bound for one instruction's assembled byte length.
pub(super) fn instruction_size_lower_bound(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    if !inst.is_encoded_push() {
        return inst.as_stack_op().map_or(1, |stack_op| {
            stack_op
                .assembled_len(gcx.sess.opts.evm_version)
                .expect("EVM IR passes only run on target-compatible stack operations")
        });
    }
    if inst.pushed_library().is_some() {
        return Target::new(gcx).opcode(op::PUSH20).bytes as usize;
    }
    if let Some(type_size) = inst.immutable_type_size() {
        return Target::new(gcx).opcode(op::push(type_size.bytes())).bytes as usize;
    }
    if let Some(value) = inst.concrete_immediate() {
        return Target::new(gcx).push(value).bytes as usize;
    }
    // Labels and data offsets are address-sensitive. They may resolve to
    // zero, so one byte is the only safe lower bound before assembly.
    1
}

/// Returns whether a terminator ends the current physical fallthrough trace.
pub(super) fn is_terminal_boundary(kind: &TerminatorKind) -> bool {
    matches!(kind, TerminatorKind::IndexedJump(_))
        || matches!(kind, TerminatorKind::Op(opcode) if op::is_terminal(*opcode))
}

/// Counts the pushed labels and terminator edges targeting each block, plus the program entry.
pub(super) fn count_block_references(module: &Module, references: &mut IndexVec<BlockId, usize>) {
    references.clear();
    references.resize(module.blocks.len(), 0);
    if let Some(entry) = references.first_mut() {
        *entry = 1;
    }
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(target) = inst.pushed_block() {
                references[target] += 1;
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| references[target] += 1);
        }
    }
}
