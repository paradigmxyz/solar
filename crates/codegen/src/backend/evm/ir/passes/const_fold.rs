//! Constant folding by symbolic execution of each block.
//!
//! The pass walks a block once, keeping a model of its stack. A word is a known constant or
//! unknown, and may carry a closed producer: the contiguous range of emitted instructions that
//! pushes only that word and has no other effect. A producer is a push, a `DUP`, or a pure
//! operation whose operands' producers are adjacent and end at it. The model also keeps each
//! producer's static cost and stack peak, priced with the target's cost model. Words below the
//! block's entry are unknown, and any other instruction applies its stack effect with unknown
//! outputs. Label, immutable, and data pushes are unknown. The model has no value identities and
//! no affine forms.
//!
//! The pass takes five rewrites. Each deletes or replaces the tail of the output, and replaces
//! only when the new code dominates the old under [`Target::dominates`] and its transient stack
//! use does not exceed the old code's.
//!
//! - A constant word with a closed producer becomes its materialization. This folds a pure
//!   operation over constant operands, including a `DUP` of a constant, as in `push 128; push 32;
//!   dup 2; add` => `push 128; push 160`, and turns a `DUP` of zero into `PUSH0`. A producer that
//!   already is the cheapest materialization stays, so the compact recipes `PUSH0 NOT` and `PUSH0
//!   NOT PUSH s SHR` survive.
//! - A constant top operand with a closed producer right before an operation that returns the other
//!   operand for every value of it ([`Fact::Identity`]) disappears with the operation.
//! - The same operand before an operation whose result is constant for every value of the other
//!   operand ([`Fact::Absorb`]) becomes `POP` and the materialized result, or only the result when
//!   the other operand's producer is closed too.
//! - A closed producer right before `POP` is deleted with it.
//! - A constant zero condition with a closed producer right before `PUSH label; JUMPI` is deleted
//!   with the branch.
//!
//! A range is replaced only when it starts at a split point and none of its instructions keeps
//! with the next one. Nothing crosses a block. The peephole pass runs this one on each block
//! before its rules, so it shares their clean-block cache; the pass itself is available as
//! `const-fold`. Instructions are copied only from the first rewrite on.
//!
//! The materialization takes the source context of the folded operation and the agreed function
//! events of the range. Removed operands' locations drop, and their function events move to the
//! instruction before the range. A rewrite never invents a location or decides anything by debug
//! information.
//!
//! The pass does not move instructions, track a word through a `SWAP`, or fold a taken
//! `JUMPI`. Algebra over unknown words stays with the peephole rules.

use super::{
    EvmPass,
    compact_pushes::materialize_immediate,
    utils::{is_unprotected, replacement_debug_info},
};
use crate::{
    backend::evm::{
        ir::{ImmediateMaterialization, Instruction, Module, ModuleTarget},
        op::{self, StackOp},
    },
    mir::utils::eval::eval_opcode,
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_sema::Gcx;

pub(super) struct ConstFold;

impl EvmPass for ConstFold {
    fn name(&self) -> &'static str {
        "const-fold"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut folder = Folder::new(module.runtime_target(gcx));
        let mut scratch = Vec::new();
        let mut changed = false;
        for block in &mut module.blocks {
            changed |= folder.run(&mut block.instructions, &mut scratch) != 0;
        }
        changed
    }
}

/// What an operation does for every value of its second operand, given its top operand.
#[derive(Clone, Copy)]
enum Fact {
    /// The result is the second operand.
    Identity,
    /// The result is this word.
    Absorb(U256),
}

/// Returns what `opcode` does for every second operand when its top operand is `top`.
fn top_operand_fact(opcode: u8, top: U256) -> Option<Fact> {
    match opcode {
        op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR if top.is_zero() => {
            Some(Fact::Identity)
        }
        op::MUL if top == U256::ONE => Some(Fact::Identity),
        op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT if top.is_zero() => {
            Some(Fact::Absorb(U256::ZERO))
        }
        op::EXP if top == U256::ONE => Some(Fact::Absorb(U256::ONE)),
        _ => None,
    }
}

/// The instructions `start..end` of the output, which push one word and have no other effect.
#[derive(Clone, Copy)]
struct Producer {
    start: usize,
    end: usize,
    /// The most words the range holds above its base, the stack height before it.
    peak: usize,
    cost: Cost,
}

#[derive(Clone, Copy)]
struct Word {
    value: Option<U256>,
    producer: Option<Producer>,
}

impl Word {
    const UNKNOWN: Self = Self { value: None, producer: None };
}

/// A rewrite of the output from `start` on. It consumes the top `consumed` words of the model.
#[derive(Clone, Copy)]
struct Rewrite {
    start: usize,
    consumed: usize,
    edit: Edit,
}

#[derive(Clone, Copy)]
enum Edit {
    Delete,
    /// Replace the range with `value`'s materialization, after a `POP` if `pop`.
    Fold {
        value: U256,
        pop: bool,
    },
}

pub(super) struct Folder {
    target: Target,
    stack: Vec<Word>,
}

impl Folder {
    pub(super) fn new(target: Target) -> Self {
        Self { target, stack: Vec::new() }
    }

    /// Folds one block and returns the number of rewrites.
    pub(super) fn run(
        &mut self,
        instructions: &mut Vec<Instruction>,
        scratch: &mut Vec<Instruction>,
    ) -> usize {
        self.stack.clear();
        // Inspect the original prefix without copying instructions until the first rewrite.
        for end in 1..=instructions.len() {
            let Some(first) = self.step(&instructions[..end]) else { continue };
            scratch.clear();
            scratch.extend(instructions.drain(end..));
            self.apply(instructions, first);
            let mut rewrites = 1;
            for inst in scratch.drain(..) {
                instructions.push(inst);
                if let Some(rewrite) = self.step(instructions) {
                    self.apply(instructions, rewrite);
                    rewrites += 1;
                }
            }
            return rewrites;
        }
        0
    }

    /// Advances the model over the last instruction of `out`, or returns the rewrite of `out`
    /// that replaces it, leaving the model before the instruction.
    fn step(&mut self, out: &[Instruction]) -> Option<Rewrite> {
        let end = out.len();
        let inst = &out[end - 1];
        if inst.is_encoded_push() {
            let cost = match inst.concrete_immediate() {
                Some(value) => self.target.push(value),
                None => self.target.opcode(inst.opcode),
            };
            let producer = Producer { start: end - 1, end, peak: 1, cost };
            self.stack.push(Word { value: inst.concrete_immediate(), producer: Some(producer) });
            return None;
        }
        if let Some(stack_op) = inst.as_stack_op() {
            return self.step_stack_op(out, stack_op);
        }

        let opcode = inst.opcode;
        let effect = inst.stack_effect();
        let inputs = usize::from(effect.inputs);
        self.ensure_depth(inputs);
        if opcode == op::JUMPI {
            if let Some(rewrite) = self.untaken_jump(out) {
                return Some(rewrite);
            }
        } else if op::is_pure(opcode) && effect.outputs == 1 {
            return self.step_pure(out, opcode, inputs);
        }
        self.stack.truncate(self.stack.len() - inputs);
        self.stack.extend((0..effect.outputs).map(|_| Word::UNKNOWN));
        None
    }

    /// The rewrite of a `JUMPI` whose condition is a constant zero with a closed producer right
    /// before the pushed label.
    fn untaken_jump(&self, out: &[Instruction]) -> Option<Rewrite> {
        let end = out.len();
        let words = &self.stack[self.stack.len() - 2..];
        // cond; push label; jumpi
        if words[0].value != Some(U256::ZERO) || out[end - 2].pushed_block().is_none() {
            return None;
        }
        let operands = closed_operands(words, end - 1)?;
        is_unprotected(out, operands.start).then_some(Rewrite {
            start: operands.start,
            consumed: 2,
            edit: Edit::Delete,
        })
    }

    /// Advances the model over a pure operation at the end of `out`, or returns its rewrite.
    fn step_pure(&mut self, out: &[Instruction], opcode: u8, inputs: usize) -> Option<Rewrite> {
        let end = out.len();
        let words = &self.stack[self.stack.len() - inputs..];
        let args = words.iter().rev().map(|word| word.value).collect::<SmallVec<[_; 3]>>();
        let cost = self.target.opcode_with_immediates(opcode, &args);
        let total = closed_operands(words, end - 1).map(|operands| Producer {
            end,
            peak: operands.peak.max(1),
            cost: operands.cost + cost,
            ..operands
        });
        if let Some(rewrite) = self.fold_top_operand(out, words, opcode, cost, total) {
            return Some(rewrite);
        }
        let value = args
            .iter()
            .copied()
            .collect::<Option<SmallVec<[U256; 3]>>>()
            .and_then(|args| eval_opcode(opcode, &args));
        if let Some(value) = value
            && let Some(total) = total
            && self.folds(out, value, &total)
        {
            return Some(Rewrite {
                start: total.start,
                consumed: inputs,
                edit: Edit::Fold { value, pop: false },
            });
        }
        self.stack.truncate(self.stack.len() - inputs);
        self.stack.push(Word { value, producer: total });
        None
    }

    fn step_stack_op(&mut self, out: &[Instruction], stack_op: StackOp) -> Option<Rewrite> {
        let end = out.len();
        self.ensure_depth(stack_op.required_depth());
        let top = self.stack.len() - 1;
        match stack_op {
            StackOp::Dup(depth) => {
                let source = self.stack[top + 1 - usize::from(depth)];
                let producer = Producer { start: end - 1, end, peak: 1, cost: self.target.dup() };
                if let Some(value) = source.value
                    && self.folds(out, value, &producer)
                {
                    return Some(Rewrite {
                        start: end - 1,
                        consumed: 0,
                        edit: Edit::Fold { value, pop: false },
                    });
                }
                self.stack.push(Word { value: source.value, producer: Some(producer) });
            }
            StackOp::Swap(depth) => self.stack.swap(top, top - usize::from(depth)),
            StackOp::Exchange(first, second) => {
                self.stack.swap(top - usize::from(first), top - usize::from(second));
            }
            StackOp::Pop => {
                // closed producer; pop
                if let Some(producer) = self.stack[top].producer
                    && producer.end == end - 1
                    && is_unprotected(out, producer.start)
                {
                    return Some(Rewrite {
                        start: producer.start,
                        consumed: 1,
                        edit: Edit::Delete,
                    });
                }
                self.stack.pop();
            }
        }
        None
    }

    /// The rewrite for a constant top operand that makes the operation, whose operands are
    /// `words`, an identity or constant, when the code it leaves is cheaper.
    fn fold_top_operand(
        &self,
        out: &[Instruction],
        words: &[Word],
        opcode: u8,
        cost: Cost,
        total: Option<Producer>,
    ) -> Option<Rewrite> {
        let &[_, top] = words else { return None };
        let (value, producer) = (top.value?, top.producer?);
        let fact = top_operand_fact(opcode, value)?;
        if producer.end != out.len() - 1 || !is_unprotected(out, producer.start) {
            return None;
        }
        // other; push c; op
        match (fact, total) {
            (Fact::Identity, _) => {
                Some(Rewrite { start: producer.start, consumed: 1, edit: Edit::Delete })
            }
            // The other operand's producer is closed too: delete it with the rest.
            (Fact::Absorb(result), Some(total)) => {
                self.folds(out, result, &total).then_some(Rewrite {
                    start: total.start,
                    consumed: 2,
                    edit: Edit::Fold { value: result, pop: false },
                })
            }
            (Fact::Absorb(result), None) => {
                let target = self.target;
                let before = producer.cost + cost;
                let after = target.opcode(op::POP) + target.push(result);
                // The `POP` frees the other operand's word before the result is built.
                (target.dominates(after, before) && self.peak(result) <= producer.peak + 1)
                    .then_some(Rewrite {
                        start: producer.start,
                        consumed: 2,
                        edit: Edit::Fold { value: result, pop: true },
                    })
            }
        }
    }

    /// Whether materializing `value` replaces `producer` with cheaper code that holds no more
    /// words at once.
    fn folds(&self, out: &[Instruction], value: U256, producer: &Producer) -> bool {
        self.target.dominates(self.target.push(value), producer.cost)
            && self.peak(value) <= producer.peak
            && is_unprotected(out, producer.start)
    }

    fn peak(&self, value: U256) -> usize {
        ImmediateMaterialization::new(self.target.evm_version(), value).stack_peak()
    }

    /// Adds unknown words below the model until it holds `depth`.
    fn ensure_depth(&mut self, depth: usize) {
        if let Some(missing) = depth.checked_sub(self.stack.len()) {
            self.stack.splice(0..0, (0..missing).map(|_| Word::UNKNOWN));
        }
    }

    /// Applies `rewrite` to the output and the model.
    fn apply(&mut self, out: &mut Vec<Instruction>, rewrite: Rewrite) {
        let Rewrite { start, consumed, edit } = rewrite;
        self.stack.truncate(self.stack.len() - consumed);
        match edit {
            Edit::Delete => {
                // The removed instructions take their origins with them; only their function
                // events move.
                let (kept, removed) = out.split_at_mut(start);
                if let Some(survivor) = kept.last_mut() {
                    for inst in removed.iter() {
                        survivor.metadata.move_function_events(&inst.metadata);
                    }
                }
                out.truncate(start);
            }
            Edit::Fold { value, pop } => {
                replace_with_constant(self.target, out, start, value, pop);
                // The materialization is closed even when a `POP` precedes it.
                let producer = Producer {
                    start: start + usize::from(pop),
                    end: out.len(),
                    peak: self.peak(value),
                    cost: self.target.push(value),
                };
                self.stack.push(Word { value: Some(value), producer: Some(producer) });
            }
        }
    }
}

/// Returns the range the operands `words` of an instruction at `end` take, when each has a closed
/// producer and the producers are adjacent and end at `end`.
fn closed_operands(words: &[Word], end: usize) -> Option<Producer> {
    let (mut start, mut peak, mut cost) = (end, 0, Cost::ZERO);
    for (depth, word) in words.iter().enumerate().rev() {
        let producer = word.producer.filter(|producer| producer.end == start)?;
        start = producer.start;
        peak = peak.max(depth + producer.peak);
        cost += producer.cost;
    }
    Some(Producer { start, end, peak, cost })
}

/// Replaces `instructions[start..]`, whose last instruction computes `value`, with a `POP` if
/// `pop` and the materialization of `value`.
pub(super) fn replace_with_constant(
    target: Target,
    instructions: &mut Vec<Instruction>,
    start: usize,
    value: U256,
    pop: bool,
) {
    // NOTE: The materialization computes the old root's value on the same path, so every
    // instruction of it takes the root's source context. Operand locations disappear.
    let context = replacement_debug_info(&instructions[start..], instructions.len() - start - 1);
    instructions.truncate(start);
    if pop {
        instructions.push(Instruction::opcode(op::POP).with_debug_loc(&context));
    }
    let first = instructions.len();
    materialize_immediate(instructions, target.evm_version(), value, &context);
    let last = instructions.last_mut().unwrap();
    if let Some(exit) = context.function_exit() {
        last.metadata.set_function_exit(exit);
    }
    if let Some(function) = context.function_invoke() {
        instructions[first].metadata.set_function_invoke(function);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_operand_facts_hold_for_every_word() {
        let edges = [
            U256::ZERO,
            U256::ONE,
            U256::from(2),
            U256::from(31),
            U256::from(255),
            U256::from(256),
            U256::ONE << 255,
            U256::MAX - U256::ONE,
            U256::MAX,
        ];
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut random = || {
            let mut limbs = [0; 4];
            for limb in &mut limbs {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *limb = state;
            }
            U256::from_limbs(limbs)
        };
        let words = edges.into_iter().chain((0..64).map(|_| random())).collect::<Vec<_>>();
        for opcode in 0..=u8::MAX {
            for &top in &edges {
                let Some(fact) = top_operand_fact(opcode, top) else { continue };
                for &other in &words {
                    let expected = match fact {
                        Fact::Identity => other,
                        Fact::Absorb(result) => result,
                    };
                    assert_eq!(eval_opcode(opcode, &[top, other]), Some(expected), "{opcode:#x}");
                }
            }
        }
    }
}
