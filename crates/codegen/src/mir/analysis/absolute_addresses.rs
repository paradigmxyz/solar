//! Classification of words as absolute memory addresses by what they are computed from.
//!
//! Each word gets the set of origins it may have: computed from constants alone, from constants
//! and calldata, or from anything else, such as the heap or a loaded word. The sets follow the
//! values through the whole module, so a function's parameters hold what its callers pass and a
//! call's result holds what the callee returns. A set keeps every origin some execution can give
//! the word, so a parameter that one caller feeds calldata and another feeds a heap pointer
//! still reports calldata.
//!
//! The analysis only sees values that move through registers. A word the module stores to memory
//! and loads again is `Other`, and so is a function's second and later results, which travel in a
//! buffer.

use crate::mir::{
    ArgIdx, Callee, Function, FunctionId, InstKind, Module, PtrLocation, Terminator, Value,
    ValueId,
    analysis::{direct_callees, is_floor_clamp},
};
use solar_data_structures::index::{IndexVec, index_vec};

/// The origins a word may have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AddressInputs(u8);

impl AddressInputs {
    /// A word no execution has produced so far.
    const NONE: Self = Self(0);
    /// Constants alone.
    const CONSTANT: Self = Self(1);
    /// Constants and calldata.
    const CALLDATA: Self = Self(2);
    /// The heap, a loaded word, or anything else.
    const OTHER: Self = Self(4);

    /// Returns whether some execution computes the word from constants and calldata, which a
    /// write through it can lay out as calldata sizes it.
    pub(crate) const fn may_be_calldata(self) -> bool {
        self.0 & Self::CALLDATA.0 != 0
    }

    /// Returns whether some execution computes the word from constants and calldata alone, so it
    /// can point below the heap.
    pub(crate) const fn may_be_absolute(self) -> bool {
        self.0 & (Self::CONSTANT.0 | Self::CALLDATA.0) != 0
    }

    /// Returns the origins of a word that is either of the two.
    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns the origins of a word computed from both, which take the least known of each pair
    /// of origins.
    fn combine(self, other: Self) -> Self {
        let mut combined = 0;
        for lhs in (0..3).map(|bit| self.0 & (1 << bit)).filter(|&bit| bit != 0) {
            for rhs in (0..3).map(|bit| other.0 & (1 << bit)).filter(|&bit| bit != 0) {
                combined |= lhs.max(rhs);
            }
        }
        Self(combined)
    }
}

/// The origins of the values of every function of a module.
pub(crate) type ModuleAddressInputs = IndexVec<FunctionId, IndexVec<ValueId, AddressInputs>>;

/// Classifies every value of every function in `module` as an absolute address by what it is
/// computed from.
///
/// Sets only grow, so repeating the passes until none changes a set reaches the fixed point.
/// Shared operands are classified once, however many expressions read them.
pub(crate) fn absolute_address_inputs(module: &Module) -> ModuleAddressInputs {
    let mut called = index_vec![false; module.functions.len()];
    for func in &module.functions {
        for callee in direct_callees(func) {
            called[callee] = true;
        }
    }
    // Nothing in the module passes the parameters of a function it does not call, or of one that
    // the outside calls too.
    let outside = module.root_functions();
    let mut state = State {
        inputs: IndexVec::new(),
        args: module
            .functions
            .iter_enumerated()
            .map(|(func_id, func)| {
                let input = if outside.contains(func_id) || !called[func_id] {
                    AddressInputs::OTHER
                } else {
                    AddressInputs::NONE
                };
                index_vec![input; func.arg_indices().count()]
            })
            .collect(),
        returns: index_vec![AddressInputs::NONE; module.functions.len()],
    };
    let arg_values = module.functions.iter().map(Function::arg_uses).collect::<IndexVec<_, _>>();
    for func in &module.functions {
        let mut inputs = index_vec![AddressInputs::NONE; func.num_values()];
        for (value, input) in inputs.iter_mut_enumerated() {
            *input = match func.value(value) {
                Value::Immediate(_) => AddressInputs::CONSTANT,
                Value::Inst(_) | Value::Arg(_) => AddressInputs::NONE,
                Value::Undef(_) | Value::Error(_) => AddressInputs::OTHER,
            };
        }
        state.inputs.push(inputs);
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (func_id, func) in module.functions.iter_enumerated() {
            changed |= state.propagate(func_id, func, &arg_values[func_id]);
        }
    }
    state.inputs
}

/// The sets the analysis has reached so far.
struct State {
    inputs: ModuleAddressInputs,
    /// What the calls to each function pass to each parameter.
    args: IndexVec<FunctionId, IndexVec<ArgIdx, AddressInputs>>,
    /// What each function returns.
    returns: IndexVec<FunctionId, AddressInputs>,
}

impl State {
    /// Runs one pass over `func`, which takes its parameters from `self.args`, adds what its calls
    /// pass to `self.args` and what it returns to `self.returns`, and returns whether any set
    /// grew.
    fn propagate(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        arg_values: &IndexVec<ArgIdx, Vec<ValueId>>,
    ) -> bool {
        let mut changed = false;
        for (index, values) in arg_values.iter_enumerated() {
            for &value in values {
                changed |= raise(&mut self.inputs[func_id][value], self.args[func_id][index]);
            }
        }
        for block in &func.blocks {
            match &block.terminator {
                Some(Terminator::Return { values }) => {
                    for &value in values {
                        changed |= raise(&mut self.returns[func_id], self.inputs[func_id][value]);
                    }
                }
                Some(Terminator::TailCall { function, args }) => {
                    changed |= self.pass_args(func_id, *function, args);
                }
                _ => {}
            }
        }
        for inst_id in func.instructions() {
            let kind = &func.inst(inst_id).kind;
            if let InstKind::ICall { function: Callee::Function(callee), args } = kind {
                changed |= self.pass_args(func_id, *callee, args);
            }
            let Some(value) = func.inst_result_value(inst_id) else { continue };
            let inputs = &self.inputs[func_id];
            let input = match kind {
                InstKind::Load(.., PtrLocation::Calldata) | InstKind::CalldataSize => {
                    AddressInputs::CALLDATA
                }
                InstKind::Add(lhs, rhs)
                | InstKind::Sub(lhs, rhs)
                | InstKind::Mul(lhs, rhs)
                | InstKind::Div(lhs, rhs)
                | InstKind::Exp(lhs, rhs)
                | InstKind::Shl(lhs, rhs)
                | InstKind::Shr(lhs, rhs)
                | InstKind::And(lhs, rhs)
                | InstKind::Or(lhs, rhs) => inputs[*lhs].combine(inputs[*rhs]),
                // A floor clamp lies at or above the heap floor whatever its word.
                InstKind::Select(cond, lhs, rhs) if is_floor_clamp(func, *cond, *lhs, *rhs) => {
                    AddressInputs::OTHER
                }
                InstKind::Select(_, lhs, rhs) => inputs[*lhs].union(inputs[*rhs]),
                InstKind::Zext(inner) | InstKind::IntToPtr(inner) => inputs[*inner],
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .fold(AddressInputs::NONE, |origins, &(_, value)| origins.union(inputs[value])),
                InstKind::ICall { function: Callee::Function(callee), .. } => self.returns[*callee],
                _ => AddressInputs::OTHER,
            };
            changed |= raise(&mut self.inputs[func_id][value], input);
        }
        changed
    }

    /// Adds what `caller` passes in `args` to the parameters of `callee`, and returns whether any
    /// set grew.
    fn pass_args(&mut self, caller: FunctionId, callee: FunctionId, args: &[ValueId]) -> bool {
        let mut changed = false;
        for (&value, param) in args.iter().zip(self.args[callee].iter_mut()) {
            changed |= raise(param, self.inputs[caller][value]);
        }
        changed
    }
}

/// Adds `input` to `current`, and returns whether it grew.
fn raise(current: &mut AddressInputs, input: AddressInputs) -> bool {
    let raised = current.union(input);
    let grew = raised != *current;
    *current = raised;
    grew
}
