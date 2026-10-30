//! Block-local common-subexpression regeneration over scheduled EVM IR.
//!
//! The pass symbolically executes each basic block and interns expressions by opcode, operands,
//! and the relevant memory or storage epoch. When an expression already exists on the physical
//! stack, it removes the repeated computation and regenerates the value with a reachable `DUP`.
//! It also tracks known constant-address memory words, removing stores of an unchanged value and
//! replacing loads with the expression last stored at that address.
//!
//! Expression spans are reusable only when they are closed: the interval must consume no value
//! from before the interval and produce only the tracked result. Unknown stack effects and
//! observable writes invalidate the affected symbolic state. Memory and storage reads carry
//! separate epochs, so a write prevents reuse across a possible state change without blocking
//! unrelated pure expressions. Rewrites stay within one block and respect the target's reachable
//! stack depth; the pass does not perform alias analysis or move instructions across control flow.
//!
//! After a `GAS` read, memory, hashing, and storage reads are no longer reused or forwarded, as
//! MIR's common-subexpression elimination also leaves them alone once gas is observed: a warm
//! `SLOAD` costs 100 gas, so dropping one changes what the next `GAS` read measures.
//!
//! This runs after physical stack scheduling, where repeated expressions and redundant constant
//! memory traffic are visible. Peephole cleanup follows it because removing a computation can
//! expose adjacent stack and arithmetic simplifications.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Metadata, Module, PushValue},
    op::{self, StackOp},
};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

pub(super) struct BlockCse;

impl EvmPass for BlockCse {
    fn name(&self) -> &'static str {
        "block-cse"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut cse = Cse { evm_version: gcx.sess.opts.evm_version, ..Default::default() };
        let mut changed = false;
        for block in &mut module.blocks {
            changed |= cse.regenerate(&mut block.instructions);
        }
        changed
    }
}

#[derive(PartialEq, Eq, Hash)]
enum Expr {
    Push(u8, u8, PushValue),
    /// An opcode, the memory or storage epoch of a read, and its operands.
    Op(u8, Option<u64>, SmallVec<[usize; 3]>),
}

#[derive(Clone, Copy)]
struct StackValue {
    expr: usize,
    /// A closed instruction interval that produces only this value.
    span: Option<(usize, usize)>,
    /// The rebuilt instruction that physically pushed this entry.
    origin: Option<usize>,
}

/// The symbolic walk over one block, writing the rebuilt instructions to `out`.
#[derive(Default)]
struct Cse {
    evm_version: EvmVersion,
    out: Vec<Instruction>,
    stack: Vec<StackValue>,
    expressions: FxHashMap<Expr, usize>,
    next_expr: usize,
    memory_epoch: u64,
    storage_epoch: u64,
    /// Constant-address memory words with a known symbolic content: a store whose value is
    /// already present is a no-op, and a load of a known word yields the stored expression.
    known_stores: FxHashMap<u64, usize>,
    const_exprs: FxHashMap<usize, u64>,
    gas_observed: bool,
    changed: bool,
}

impl Cse {
    /// Rebuilds `instructions` and returns whether the rebuild removed or regenerated any.
    fn regenerate(&mut self, instructions: &mut Vec<Instruction>) -> bool {
        self.stack.clear();
        self.expressions.clear();
        self.known_stores.clear();
        self.const_exprs.clear();
        self.gas_observed = false;
        self.changed = false;
        for inst in instructions.drain(..) {
            self.step(inst);
        }
        instructions.append(&mut self.out);
        self.changed
    }

    fn step(&mut self, inst: Instruction) {
        let opcode = inst.opcode;
        if inst.is_encoded_push() {
            return self.push(inst);
        }
        if let Some(stack_op) = inst.as_stack_op() {
            return self.stack_op(inst, stack_op);
        }
        if opcode == op::MSTORE || opcode == op::MSTORE8 {
            return self.store(inst);
        }

        // Once gas is observed, every read of memory or storage is a new expression.
        if self.gas_observed {
            match opcode {
                op::MLOAD | op::KECCAK256 => self.memory_epoch += 1,
                op::SLOAD | op::TLOAD => self.storage_epoch += 1,
                _ => {}
            }
        }
        self.gas_observed |= opcode == op::GAS;

        if opcode == op::MLOAD && !self.gas_observed {
            self.ensure_depth(1);
            let addr = self.stack[self.stack.len() - 1];
            if let Some(&known) =
                self.const_exprs.get(&addr.expr).and_then(|address| self.known_stores.get(address))
            {
                self.stack.pop();
                // Forward the stored word: reuse a live stack copy when the address was just
                // pushed, otherwise keep the load but give its result the stored expression for
                // downstream reuse.
                if self.is_removable_copy_at(addr, 1)
                    && let Some(depth) = self.reachable_depth(known)
                {
                    self.out.pop();
                    self.emit_dup(depth, known, inst.metadata);
                } else {
                    self.emit(inst, known, None);
                }
                return;
            }
        }

        if let Some((inputs, epoch)) =
            expression_inputs(opcode, self.memory_epoch, self.storage_epoch)
        {
            self.ensure_depth(inputs);
            let operands =
                self.stack.drain(self.stack.len() - inputs..).collect::<SmallVec<[_; 3]>>();
            let mut operand_exprs =
                operands.iter().map(|value| value.expr).collect::<SmallVec<_>>();
            if op::is_commutative(opcode) {
                operand_exprs.sort_unstable();
            }
            let expr = self.intern(Expr::Op(opcode, epoch, operand_exprs));
            let start = closed_span_start(operands.iter().map(|value| value.span), self.out.len());
            if let Some(start) = start
                && let Some(depth) = self.reachable_depth(expr)
            {
                // The removed interval holds at least two instructions, so it costs at least
                // 4 gas and 2 bytes, and a `DUPn` costs at most 3 gas and 2 bytes.
                self.out.truncate(start);
                self.emit_dup(depth, expr, inst.metadata);
            } else {
                self.emit(inst, expr, start);
            }
            return;
        }

        let stack_effect = inst.stack_effect();
        let origin = self.out.len();
        self.out.push(inst);
        if op::writes_memory(opcode) {
            self.memory_epoch += 1;
            self.known_stores.clear();
        }
        if op::writes_storage(opcode) {
            self.storage_epoch += 1;
        }
        let inputs = usize::from(stack_effect.inputs);
        self.ensure_depth(inputs);
        self.stack.truncate(self.stack.len() - inputs);
        for _ in 0..stack_effect.outputs {
            let expr = self.fresh();
            self.stack.push(StackValue { expr, span: None, origin: Some(origin) });
        }
    }

    fn push(&mut self, inst: Instruction) {
        let Some(value) = inst.value else {
            let expr = self.fresh();
            return self.emit(inst, expr, None);
        };
        let expr = self.intern(Expr::Push(inst.opcode, inst.encoding, value));
        if let Some(immediate) = inst.concrete_immediate()
            && let Ok(address) = u64::try_from(immediate)
        {
            self.const_exprs.insert(expr, address);
        }
        if inst.is_duplicable_push(self.evm_version)
            && let Some(depth) = self.reachable_depth(expr)
        {
            self.emit_dup(depth, expr, inst.metadata);
        } else {
            self.emit(inst, expr, Some(self.out.len()));
        }
    }

    fn stack_op(&mut self, inst: Instruction, stack_op: StackOp) {
        match stack_op {
            StackOp::Dup(depth) => {
                let depth = usize::from(depth);
                self.ensure_depth(depth);
                let expr = self.stack[self.stack.len() - depth].expr;
                self.emit(inst, expr, None);
            }
            StackOp::Swap(depth) => self.swap(inst, 0, depth),
            StackOp::Exchange(n, m) => self.swap(inst, n, m),
            StackOp::Pop => {
                self.ensure_depth(1);
                self.stack.pop();
                self.out.push(inst);
            }
        }
    }

    fn swap(&mut self, inst: Instruction, n: u8, m: u8) {
        self.ensure_depth(usize::from(m) + 1);
        let top = self.stack.len() - 1;
        self.stack.swap(top - usize::from(n), top - usize::from(m));
        self.out.push(inst);
    }

    fn store(&mut self, inst: Instruction) {
        let opcode = inst.opcode;
        self.ensure_depth(2);
        let addr = self.stack[self.stack.len() - 1];
        let value = self.stack[self.stack.len() - 2];
        let const_addr = self.const_exprs.get(&addr.expr).copied();
        self.stack.truncate(self.stack.len() - 2);
        // Re-storing the word already known at this address is a no-op. Remove it only when the
        // two operands were pushed by the two immediately preceding copies, so truncation drops
        // exactly the operand materialization and nothing else.
        if opcode == op::MSTORE
            && let Some(address) = const_addr
            && self.known_stores.get(&address) == Some(&value.expr)
            && self.is_removable_copy_at(addr, 1)
            && self.is_removable_copy_at(value, 2)
        {
            self.out.truncate(self.out.len() - 2);
            self.changed = true;
            return;
        }
        match const_addr {
            Some(address) => {
                self.known_stores
                    .retain(|&slot, _| slot.abs_diff(address) >= op::WORD_BYTES as u64);
                if opcode == op::MSTORE {
                    self.known_stores.insert(address, value.expr);
                }
            }
            None => self.known_stores.clear(),
        }
        self.memory_epoch += 1;
        self.out.push(inst);
    }

    /// Appends `inst` and pushes the value it produces.
    fn emit(&mut self, inst: Instruction, expr: usize, span_start: Option<usize>) {
        let origin = self.out.len();
        self.out.push(inst);
        let span = span_start.map(|start| (start, self.out.len()));
        self.stack.push(StackValue { expr, span, origin: Some(origin) });
    }

    /// Regenerates `expr` with the `DUP` of the entry `depth` below the top.
    fn emit_dup(&mut self, depth: usize, expr: usize, metadata: Metadata) {
        let dup = Instruction::stack_op(StackOp::Dup((depth + 1) as u8)).with_metadata(metadata);
        self.emit(dup, expr, None);
        self.changed = true;
    }

    /// Returns the depth of the nearest stack entry holding `expr`, when a `DUP` reaches it.
    fn reachable_depth(&self, expr: usize) -> Option<usize> {
        let depth = self.stack.iter().rev().position(|value| value.expr == expr)?;
        (depth < self.evm_version.reachable_stack_depth()).then_some(depth)
    }

    /// Returns whether `value` was pushed by the pure copy that sits `back` instructions before
    /// the end of the output.
    fn is_removable_copy_at(&self, value: StackValue, back: usize) -> bool {
        let Some(at) = self.out.len().checked_sub(back) else { return false };
        let copy = &self.out[at];
        value.origin == Some(at)
            && (copy.is_encoded_push() || matches!(copy.as_stack_op(), Some(StackOp::Dup(_))))
    }

    /// Prepends unknown entries until the stack holds `depth` values.
    fn ensure_depth(&mut self, depth: usize) {
        let missing = depth.saturating_sub(self.stack.len());
        let first = self.next_expr;
        self.next_expr += missing;
        self.stack.splice(
            0..0,
            (first..first + missing).map(|expr| StackValue { expr, span: None, origin: None }),
        );
    }

    fn intern(&mut self, expr: Expr) -> usize {
        let next = &mut self.next_expr;
        *self.expressions.entry(expr).or_insert_with(|| {
            *next += 1;
            *next - 1
        })
    }

    fn fresh(&mut self) -> usize {
        self.next_expr += 1;
        self.next_expr - 1
    }
}

/// Returns the start of the instruction interval that produced exactly the `spans`, when they are
/// adjacent and end at `instruction_end`.
fn closed_span_start(
    spans: impl IntoIterator<Item = Option<(usize, usize)>>,
    instruction_end: usize,
) -> Option<usize> {
    let mut spans: SmallVec<[(usize, usize); 3]> = spans.into_iter().collect::<Option<_>>()?;
    spans.sort_unstable();
    let start = spans.first()?.0;
    let mut cursor = start;
    for (span_start, span_end) in spans {
        if span_start != cursor {
            return None;
        }
        cursor = span_end;
    }
    (cursor == instruction_end).then_some(start)
}

/// Returns the input count and cache epoch of an opcode this pass value-numbers, or `None` when
/// the opcode is not a tracked pure expression. The input count comes from [`op::stack_io`]; the
/// epoch keys a memory or storage read to the current clobber generation so it is only reused
/// until the next write. Pure arithmetic has no epoch, and calldata is immutable.
fn expression_inputs(
    opcode: u8,
    memory_epoch: u64,
    storage_epoch: u64,
) -> Option<(usize, Option<u64>)> {
    let epoch = if op::is_pure(opcode) {
        None
    } else {
        Some(match opcode {
            op::MLOAD | op::KECCAK256 => memory_epoch << 1,
            op::SLOAD | op::TLOAD => (storage_epoch << 1) | 1,
            op::CALLDATALOAD => 0,
            _ => return None,
        })
    };
    let (inputs, _) = op::stack_io(opcode)?;
    Some((usize::from(inputs), epoch))
}
