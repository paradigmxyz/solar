//! Classification of words as absolute memory addresses by what they are computed from.
//!
//! Each word gets the set of origins it may have: computed from constants alone, from constants
//! and calldata, or from anything else, such as the heap or a loaded word. The sets follow the
//! values through the whole module, so a function's parameters hold what its callers pass and a
//! call's result holds what the callee returns. The integer parameters of an external entry hold
//! calldata words. A set keeps every origin some execution can give the word, so a parameter that
//! one caller feeds calldata and another feeds a heap pointer still reports calldata.
//!
//! The analysis only sees values that move through registers. A word the module stores to memory
//! and loads again is `Other`, and so is a function's second and later results once lowering moves
//! them into a buffer. Before that, a struct holds the union of its fields.

use crate::mir::{
    ArgIdx, Callee, Function, FunctionId, InstKind, MirType, Module, PtrLocation, Terminator,
    Value, ValueId,
    analysis::{direct_callees, is_floor_clamp},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
};

/// The origins a word may have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        for lhs in self.origins() {
            for rhs in other.origins() {
                combined |= lhs.max(rhs);
            }
        }
        Self(combined)
    }

    /// Returns the origins one at a time, from the most to the least known.
    fn origins(self) -> impl Iterator<Item = u8> {
        (0..3).map(move |bit| self.0 & (1 << bit)).filter(|&origin| origin != 0)
    }

    /// Adds `other` to the origins, and returns whether they grew.
    fn raise(&mut self, other: Self) -> bool {
        let raised = self.union(other);
        let grew = raised != *self;
        *self = raised;
        grew
    }
}

/// The origins of the values of every function of a module.
pub(crate) type ModuleAddressInputs = IndexVec<FunctionId, IndexVec<ValueId, AddressInputs>>;

/// Classifies every value of every function in `module` as an absolute address by what it is
/// computed from.
///
/// Sets only grow, so revisiting a function whenever a set it reads grows reaches the fixed
/// point. A change wakes the callee whose parameters it feeds, or the callers that receive the
/// results it returns, so a chain of calls costs one visit per link. Shared operands are
/// classified once, however many expressions read them.
pub(crate) fn absolute_address_inputs(module: &Module) -> ModuleAddressInputs {
    let mut called = index_vec![false; module.functions.len()];
    let mut callers = index_vec![Vec::new(); module.functions.len()];
    for (caller, func) in module.functions.iter_enumerated() {
        for callee in direct_callees(func) {
            called[callee] = true;
            if callers[callee].last() != Some(&caller) {
                callers[callee].push(caller);
            }
        }
    }
    // The parameters of a function nothing in the module calls, or that the outside calls too, hold
    // what the outside passes. For an external entry that is the decoded calldata, so its integer
    // parameters are calldata words and anything else is unknown.
    let outside = module.root_functions();
    let entries = module.external_entries();
    let mut state = State {
        inputs: module.functions.iter().map(initial_inputs).collect(),
        args: module
            .functions
            .iter_enumerated()
            .map(|(func_id, func)| {
                func.arg_indices()
                    .map(|index| {
                        if !outside.contains(func_id) && called[func_id] {
                            AddressInputs::NONE
                        } else if entries.contains(func_id)
                            && matches!(func.arg_ty(index), MirType::Int(_))
                        {
                            AddressInputs::CALLDATA
                        } else {
                            AddressInputs::OTHER
                        }
                    })
                    .collect()
            })
            .collect(),
        returns: index_vec![AddressInputs::NONE; module.functions.len()],
        callers,
        awake: DenseBitSet::new_filled(module.functions.len()),
        queue: module.functions.indices().rev().collect(),
    };
    let arg_values = module.functions.iter().map(Function::arg_uses).collect::<IndexVec<_, _>>();
    while let Some(func_id) = state.queue.pop() {
        state.awake.remove(func_id);
        while state.propagate(func_id, &module.functions[func_id], &arg_values[func_id]) {}
    }
    state.inputs
}

/// Returns the origins of `func`'s values before any pass: constants for immediates, nothing yet
/// for results and parameters, and other for the values lowering left undefined.
fn initial_inputs(func: &Function) -> IndexVec<ValueId, AddressInputs> {
    let mut inputs = index_vec![AddressInputs::NONE; func.num_values()];
    for (value, input) in inputs.iter_mut_enumerated() {
        *input = match func.value(value) {
            Value::Immediate(_) => AddressInputs::CONSTANT,
            Value::Inst(_) | Value::Arg(_) => AddressInputs::NONE,
            Value::Undef(_) | Value::Error(_) => AddressInputs::OTHER,
        };
    }
    inputs
}

/// The sets the analysis has reached so far.
struct State {
    inputs: ModuleAddressInputs,
    /// What the calls to each function pass to each parameter.
    args: IndexVec<FunctionId, IndexVec<ArgIdx, AddressInputs>>,
    /// What each function returns.
    returns: IndexVec<FunctionId, AddressInputs>,
    /// The functions that call or tail call each function.
    callers: IndexVec<FunctionId, Vec<FunctionId>>,
    /// The functions in `queue`.
    awake: DenseBitSet<FunctionId>,
    /// The functions to visit again.
    queue: Vec<FunctionId>,
}

impl State {
    /// Runs one pass over `func`, which takes its parameters from `self.args`, adds what its calls
    /// pass to `self.args` and what it returns to `self.returns`, wakes the functions that read
    /// what grew, and returns whether any set of `func` grew.
    fn propagate(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        arg_values: &IndexVec<ArgIdx, Vec<ValueId>>,
    ) -> bool {
        let mut changed = false;
        for (index, values) in arg_values.iter_enumerated() {
            for &value in values {
                changed |= self.inputs[func_id][value].raise(self.args[func_id][index]);
            }
        }
        for block in &func.blocks {
            match &block.terminator {
                // A call's result is the first returned component.
                Some(Terminator::Return { values }) => {
                    if let Some(&value) = values.first() {
                        let returned = self.inputs[func_id][value];
                        changed |= self.raise_returns(func_id, returned);
                    }
                }
                // A tail call delivers the callee's results as its own.
                Some(Terminator::TailCall { function, args }) => {
                    changed |= self.pass_args(func_id, *function, args);
                    let returned = self.returns[*function];
                    changed |= self.raise_returns(func_id, returned);
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
                // A struct holds the union of its fields, so a multi-result call's result keeps
                // what its callee inserts. An undefined base adds no field.
                InstKind::InsertValue { aggregate, value, .. } => match func.value(*aggregate) {
                    Value::Undef(_) => inputs[*value],
                    _ => inputs[*aggregate].union(inputs[*value]),
                },
                InstKind::ExtractValue { aggregate, .. } => inputs[*aggregate],
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .fold(AddressInputs::NONE, |origins, &(_, value)| origins.union(inputs[value])),
                InstKind::ICall { function: Callee::Function(callee), .. } => self.returns[*callee],
                _ => AddressInputs::OTHER,
            };
            changed |= self.inputs[func_id][value].raise(input);
        }
        changed
    }

    /// Adds what `caller` passes in `args` to the parameters of `callee`, and returns whether any
    /// set grew.
    fn pass_args(&mut self, caller: FunctionId, callee: FunctionId, args: &[ValueId]) -> bool {
        let mut changed = false;
        for (&value, param) in args.iter().zip(self.args[callee].iter_mut()) {
            changed |= param.raise(self.inputs[caller][value]);
        }
        if changed {
            self.wake(callee);
        }
        changed
    }

    /// Adds `returned` to what `func_id` returns, wakes its callers if that grew it, and returns
    /// whether it grew.
    fn raise_returns(&mut self, func_id: FunctionId, returned: AddressInputs) -> bool {
        let changed = self.returns[func_id].raise(returned);
        if changed {
            for index in 0..self.callers[func_id].len() {
                self.wake(self.callers[func_id][index]);
            }
        }
        changed
    }

    /// Queues `func_id` for another visit.
    fn wake(&mut self, func_id: FunctionId) {
        if self.awake.insert(func_id) {
            self.queue.push(func_id);
        }
    }
}
