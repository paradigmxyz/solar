//! Remove redundant low-bit masks using physical-stack data dependencies.
//!
//! A forward walk records immutable value nodes and known result widths, following
//! DUP, SWAP, and EXCHANGE without treating them as computations. A reverse walk
//! unions the low bits each use observes. Opcode contracts supply producer widths
//! and implicit operand truncation; bitwise and modular arithmetic propagate demand.
//! Thus a shared value keeps its cleanup if even one use observes the high bits.
//! AND demand uses only literal bounds; removed masks forward demand unchanged so
//! a producer-width proof cannot disappear when another mask is also removed.
//!
//! Only adjacent literal PUSH/AND pairs are removed. Their input either already
//! fits the mask or all uses ignore the cleared bits. No instruction is moved,
//! no effect is removed, and the physical stack layout stays identical. Unknown
//! effects, protected bundles, and gas/PC observations end the tracked region.
//! Block outputs are fully observed, so this does not assume inter-block layouts.
//! Run before constant materialization obscures literal masks. Unchanged blocks
//! without a candidate require no graph allocation.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, TerminatorKind},
    op::{self, StackOp},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_sema::Gcx;

pub(super) struct WordCleanup;

impl EvmPass for WordCleanup {
    fn name(&self) -> &'static str {
        "word-cleanup"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let mut graph = Graph::default();
        for block in &mut module.blocks {
            let terminal = block.terminator.as_ref().and_then(|term| match term.kind {
                TerminatorKind::Op(opcode)
                    if term.metadata.stack.is_none() && !term.metadata.keep_with_next =>
                {
                    Some(opcode)
                }
                _ => None,
            });
            changed |= graph.run(&mut block.instructions, terminal);
        }
        changed
    }
}

struct Node {
    opcode: u8,
    inputs: SmallVec<[usize; 2]>,
    bits: u16,
    demanded: u16,
}

struct Mask {
    node: usize,
    instruction: usize,
    bits: u16,
    remove: bool,
}

#[derive(Default)]
struct Graph {
    nodes: Vec<Node>,
    stack: Vec<usize>,
    masks: Vec<Mask>,
}

impl Graph {
    fn node(&mut self, opcode: u8, inputs: SmallVec<[usize; 2]>, bits: u16) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node { opcode, inputs, bits, demanded: 0 });
        id
    }

    fn ensure_depth(&mut self, depth: usize) {
        while self.stack.len() < depth {
            let id = self.node(op::INVALID, SmallVec::new(), 256);
            self.stack.insert(0, id);
        }
    }

    fn barrier(&mut self) {
        for id in self.stack.drain(..) {
            self.nodes[id].demanded = 256;
        }
    }

    fn operation(&mut self, opcode: u8) -> Option<usize> {
        let definition = op::definition(opcode)?;
        let (pops, pushes) = definition.stack_io?;
        if pushes > 1 {
            return None;
        }
        self.ensure_depth(usize::from(pops));
        let inputs = (0..pops).map(|_| self.stack.pop().unwrap()).collect::<SmallVec<[usize; 2]>>();
        let bits = match opcode {
            op::AND => self.nodes[inputs[0]].bits.min(self.nodes[inputs[1]].bits),
            op::OR | op::XOR => self.nodes[inputs[0]].bits.max(self.nodes[inputs[1]].bits),
            _ => definition.result_bits,
        };
        let id = self.node(opcode, inputs, bits);
        if pushes != 0 {
            self.stack.push(id);
        }
        Some(id)
    }

    fn run(&mut self, instructions: &mut Vec<Instruction>, terminal: Option<u8>) -> bool {
        if !instructions.windows(2).any(|pair| {
            pair[1].as_evm_opcode() == Some(op::AND)
                && pair[0].concrete_immediate().is_some_and(|mask| mask_bits(mask).is_some())
        }) {
            return false;
        }
        self.nodes.clear();
        self.stack.clear();
        self.masks.clear();
        for (index, inst) in instructions.iter().enumerate() {
            if !inst.has_canonical_stack_effect()
                || inst.keeps_with_next()
                || index > 0 && instructions[index - 1].keeps_with_next()
                || matches!(inst.as_evm_opcode(), Some(op::GAS | op::PC | op::JUMPDEST))
            {
                self.barrier();
                continue;
            }
            if inst.is_encoded_push() {
                let bits = inst.concrete_immediate().map_or(256, |value| value.bit_len() as u16);
                let id = self.node(op::INVALID, SmallVec::new(), bits);
                self.stack.push(id);
                continue;
            }
            if let Some(stack_op) = inst.as_stack_op() {
                match stack_op {
                    StackOp::Dup(depth) => {
                        self.ensure_depth(usize::from(depth));
                        self.stack.push(self.stack[self.stack.len() - usize::from(depth)]);
                    }
                    StackOp::Swap(depth) => {
                        self.ensure_depth(usize::from(depth) + 1);
                        let top = self.stack.len() - 1;
                        self.stack.swap(top, top - usize::from(depth));
                    }
                    StackOp::Exchange(n, m) => {
                        self.ensure_depth(usize::from(m) + 1);
                        let top = self.stack.len() - 1;
                        self.stack.swap(top - usize::from(n), top - usize::from(m));
                    }
                    StackOp::Pop => {
                        self.ensure_depth(1);
                        self.stack.pop();
                    }
                }
                continue;
            }
            if let Some(id) = self.operation(inst.opcode) {
                if inst.opcode == op::AND
                    && index > 0
                    && !instructions[index - 1].keeps_with_next()
                    && instructions[index - 1].has_canonical_stack_effect()
                    && (index < 2 || !instructions[index - 2].keeps_with_next())
                    && let Some(mask) = instructions[index - 1].concrete_immediate()
                    && let Some(bits) = mask_bits(mask)
                {
                    self.masks.push(Mask { node: id, instruction: index, bits, remove: false });
                }
            } else {
                self.barrier();
            }
        }
        if let Some(opcode) = terminal {
            self.operation(opcode);
        }
        self.barrier();
        let mut masks = self.masks.iter_mut().rev().peekable();
        for id in (0..self.nodes.len()).rev() {
            let node = &self.nodes[id];
            let demanded = node.demanded;
            let opcode = node.opcode;
            let mut removed = false;
            if let Some(mask) = masks.peek_mut()
                && mask.node == id
            {
                removed = demanded <= mask.bits || self.nodes[node.inputs[1]].bits <= mask.bits;
                mask.remove = removed;
                masks.next();
            }
            for index in 0..self.nodes[id].inputs.len() {
                let input = self.nodes[id].inputs[index];
                let bits = match opcode {
                    op::AND => {
                        let other = &self.nodes[self.nodes[id].inputs[1 - index]];
                        // Only literal bounds survive other mask deletions. A removed mask
                        // must forward full demand to preserve its producer's width proof.
                        if !removed && other.opcode == op::INVALID {
                            demanded.min(other.bits)
                        } else {
                            demanded
                        }
                    }
                    op::OR | op::XOR | op::NOT | op::ADD | op::SUB | op::MUL => demanded,
                    _ => op::definition(opcode)
                        .and_then(|def| def.input_bits.get(index))
                        .copied()
                        .unwrap_or(256),
                };
                self.nodes[input].demanded = self.nodes[input].demanded.max(bits);
            }
        }
        self.masks.retain(|mask| mask.remove);
        if self.masks.is_empty() {
            return false;
        }
        // NOTE: Removed pairs drop their debug metadata; surviving instructions keep their origins.
        let mut remove =
            self.masks.iter().flat_map(|mask| [mask.instruction - 1, mask.instruction]).peekable();
        let mut index = 0;
        instructions.retain(|_| {
            let keep = remove.peek().copied() != Some(index);
            if !keep {
                remove.next();
            }
            index += 1;
            keep
        });
        true
    }
}

fn mask_bits(mask: U256) -> Option<u16> {
    (mask & mask.wrapping_add(U256::ONE)).is_zero().then(|| mask.bit_len() as u16)
}
