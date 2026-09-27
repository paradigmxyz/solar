//! The modeled cost of a function, priced by the target cost model.
//!
//! A function's bytes are the code of its reachable blocks and its gas the operations a call runs,
//! callees included, plus the memory the call grows into. Both come from one pricing of each
//! operation, so that a rewrite cannot win by moving work between them:
//!
//! - An operation costs what [`Target::op`] charges for it. The gas meter sizes dynamic work, such
//!   as hashed words or exponent bytes, from the values the run computes; bytes come from
//!   immediates alone.
//! - Each operand costs a push for an immediate or one stack copy otherwise. This stands in for
//!   stack scheduling, which a function-level model cannot see; phis cost nothing.
//! - `select` costs its emitted sequence, internal calls and returns the call protocol, and each
//!   jump, branch, and switch case a pushed label, the jump, and the landing.
//!
//! The model ranks a candidate against its original, which the same approximations price, and
//! [`max_live_values`] bounds the stack pressure a candidate may add.

use crate::{
    backend::evm::op,
    mir::{
        BlockId, Callee, Function, InstId, InstKind, Module, Terminator, Value, ValueId,
        analysis::{CfgInfo, Liveness},
        memory::EvmMemoryLayout,
        utils::interp::Meter,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Returns the estimated bytes of the reachable code of `function`.
pub(super) fn code_bytes(target: Target, module: &Module, function: &Function) -> u64 {
    let cfg = CfgInfo::new(function);
    let immediate = |value| immediate(function, value);
    let mut bytes = 0;
    for &block in cfg.rpo() {
        for &inst in &function.blocks[block].instructions {
            bytes += u64::from(instruction(target, module, function, inst, &immediate).bytes);
        }
        bytes += u64::from(terminator(target, function, block, None).bytes);
    }
    bytes
}

/// Adds up the gas of the operations runs report.
///
/// A meter serves the runs of one set of function bodies, whose addresses key its prices: an
/// operation's static price is computed once, and each run only adds the dynamic work its operand
/// values size.
pub(super) struct GasMeter<'a> {
    target: Target,
    module: &'a Module,
    prices: FxHashMap<(usize, Site), u64>,
    gas: u64,
}

/// An instruction or a block's terminator.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Site {
    Instruction(InstId),
    Terminator(BlockId),
}

impl<'a> GasMeter<'a> {
    pub(super) fn new(target: Target, module: &'a Module) -> Self {
        Self { target, module, prices: FxHashMap::default(), gas: 0 }
    }

    /// Returns the gas since the last call.
    pub(super) fn take(&mut self) -> u64 {
        std::mem::take(&mut self.gas)
    }

    fn price(&mut self, function: &Function, site: Site, price: impl FnOnce() -> Cost) -> u64 {
        let key = (std::ptr::from_ref(function).addr(), site);
        *self.prices.entry(key).or_insert_with(|| u64::from(price().gas))
    }
}

impl Meter for GasMeter<'_> {
    fn instruction(
        &mut self,
        function: &Function,
        inst: InstId,
        operand: &dyn Fn(ValueId) -> Option<U256>,
    ) {
        let (target, module) = (self.target, self.module);
        let static_value = |value| immediate(function, value);
        let price = self.price(function, Site::Instruction(inst), || {
            instruction(target, module, function, inst, &static_value)
        });
        let kind = &function.inst(inst).kind;
        // Replace the operation's statically sized work with the work this run's values size.
        let gas =
            if matches!(kind, InstKind::Phi(_) | InstKind::Select(..) | InstKind::ICall { .. }) {
                price
            } else {
                let op = kind.op();
                price - u64::from(target.op(&op, static_value).gas)
                    + u64::from(target.op(&op, operand).gas)
            };
        self.gas = self.gas.saturating_add(gas);
    }

    fn terminator(
        &mut self,
        function: &Function,
        block: BlockId,
        operand: &dyn Fn(ValueId) -> Option<U256>,
    ) {
        let gas = if matches!(function.blocks[block].terminator, Some(Terminator::Switch { .. })) {
            u64::from(terminator(self.target, function, block, Some(operand)).gas)
        } else {
            let target = self.target;
            self.price(function, Site::Terminator(block), || {
                terminator(target, function, block, None)
            })
        };
        self.gas = self.gas.saturating_add(gas);
    }
}

/// Returns the most instruction results and arguments live at once in `function`.
pub(super) fn max_live_values(function: &Function) -> usize {
    let liveness = Liveness::compute(function);
    let cfg = CfgInfo::new(function);
    let counted = |value: ValueId| matches!(function.value(value), Value::Inst(_) | Value::Arg(_));
    let mut most = 0;
    let mut operands = SmallVec::<[ValueId; 8]>::new();
    for &block in cfg.rpo() {
        let body = &function.blocks[block];
        let mut live = DenseBitSet::from(liveness.live_out(block));
        if let Some(terminator) = &body.terminator {
            for operand in terminator.operands() {
                live.insert(operand);
            }
        }
        most = most.max(live.iter().filter(|&value| counted(value)).count());
        for &inst in body.instructions.iter().rev() {
            let instruction = function.inst(inst);
            if let Some(result) = instruction.result() {
                live.remove(result);
            }
            if !matches!(instruction.kind, InstKind::Phi(_)) {
                operands.clear();
                instruction.kind.collect_operands(&mut operands);
                for &operand in &operands {
                    live.insert(operand);
                }
            }
            most = most.max(live.iter().filter(|&value| counted(value)).count());
        }
    }
    most
}

/// Prices instruction `inst` of `function`, sizing dynamic work from the values `value` knows.
fn instruction(
    target: Target,
    module: &Module,
    function: &Function,
    inst: InstId,
    value: &dyn Fn(ValueId) -> Option<U256>,
) -> Cost {
    let kind = &function.inst(inst).kind;
    let operation = match kind {
        InstKind::Phi(_) => return Cost::ZERO,
        // Conditions are `i1`, so the emitted sequence needs no normalization.
        InstKind::Select(..) => target.select(false),
        InstKind::ICall { function: Callee::Function(callee), args } => {
            let callee = module.function(*callee);
            let frame_words = callee.internal_frame_size / EvmMemoryLayout::WORD_SIZE;
            target.icall(args.len(), callee.return_components().len(), frame_words)
        }
        kind => target.op(&kind.op(), value),
    };
    kind.operands()
        .into_iter()
        .fold(operation, |cost, operand| cost + operand_cost(target, function, operand))
}

/// Prices the terminator of `block` in `function`. `run` reads the values of a run, which
/// decide how many cases a switch compares; without them, every case counts.
fn terminator(
    target: Target,
    function: &Function,
    block: BlockId,
    run: Option<&dyn Fn(ValueId) -> Option<U256>>,
) -> Cost {
    let Some(terminator) = &function.blocks[block].terminator else { return Cost::ZERO };
    let control = match terminator {
        Terminator::Jump(_) | Terminator::TailCall { .. } => target.jump(),
        Terminator::Branch { .. } => target.branch(),
        Terminator::Switch { value: scrutinee, cases, .. } => {
            // dup1; push case; eq; push label; jumpi; jumpdest
            let case = |case| {
                target.dup()
                    + operand_cost(target, function, case)
                    + target.opcode(op::EQ)
                    + target.branch()
            };
            let scrutinee = run.and_then(|run| run(*scrutinee));
            let mut cost = Cost::ZERO;
            let mut matched = false;
            for &(case_value, _) in cases {
                cost += case(case_value);
                if let (Some(scrutinee), Some(run)) = (scrutinee, run)
                    && run(case_value) == Some(scrutinee)
                {
                    matched = true;
                    break;
                }
            }
            if matched { cost } else { cost + target.jump() }
        }
        Terminator::Return { values } => {
            target.internal_return(function.params.len(), values.len())
        }
        Terminator::Revert { .. } => target.opcode(op::REVERT),
        Terminator::ReturnData { .. } => target.opcode(op::RETURN),
        Terminator::Stop => target.opcode(op::STOP),
        Terminator::Invalid => target.opcode(op::INVALID),
        Terminator::SelfDestruct { .. } => target.opcode(op::SELFDESTRUCT),
        Terminator::RevertReturndata => target.opcode(op::REVERT),
    };
    let operands = match terminator {
        // Case values are priced with their comparisons.
        Terminator::Switch { value, .. } => SmallVec::from_slice(&[*value]),
        terminator => terminator.operands(),
    };
    operands
        .into_iter()
        .fold(control, |cost, operand| cost + operand_cost(target, function, operand))
}

/// Prices materializing `value` as an operand: a push for an immediate, a stack copy otherwise.
fn operand_cost(target: Target, function: &Function, value: ValueId) -> Cost {
    immediate(function, value).map_or_else(|| target.dup(), |value| target.push(value))
}

/// Returns the word of an immediate value.
fn immediate(function: &Function, value: ValueId) -> Option<U256> {
    match function.value(value) {
        Value::Immediate(immediate) => immediate.as_u256(),
        _ => None,
    }
}
