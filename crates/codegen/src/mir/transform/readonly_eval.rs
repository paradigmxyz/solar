//! Evaluate internal calls over concrete scalars and explicitly initialized fresh memory.
//!
//! A forward scan records constant full-word stores in each block. Alias ModRef
//! invalidates overlapping writes and unknown memory effects. Only unique,
//! non-loop allocation bases are tracked; scratch, frames, recycled pointers,
//! and implicit zero initialization never supply facts. Thus every removed load
//! reads bytes already written on that path and cannot expand memory.
//!
//! A bounded interpreter follows concrete control flow through direct calls,
//! scalar EVM operations, phis, and reads of those words. It rejects every
//! executed write, unknown read, environment observation, or unsupported operation.
//! Pointer values remain symbolic and cannot escape as constant return values.
//! Memory, storage, and calldata reference returns are excluded even for concrete
//! addresses: exposing immediates can disrupt pointer induction and loop scheduling.
//! A shared per-caller fuel budget and nesting limit bound recursive and looping
//! evaluation. The memory table holds at most 64 words to bound alias filtering.
//!
//! Successful single-result calls become typed constants; successful void calls
//! disappear. Constant scalar arguments also permit evaluation without a memory
//! allocation. Only the executed path needs to be read-only, so a checked helper
//! can fold when its failure branch is unreachable for these concrete arguments.
//! `Stop` in an internal void callee is an internal return, matching lowering.
//! No allocation or initialization is removed here, and no code is moved or cloned.
//! Run after semantic memory lowering and allocation coalescing, before raw
//! allocations lose their provenance. Facts do not cross caller block boundaries.

use crate::mir::{
    ArgIdx, BlockId, Function, FunctionId, Immediate, InstId, InstKind, MirType, Module,
    Terminator, Value, ValueId,
    analysis::{
        AddressSpace, AliasAnalysis, Location, LocationSize, MemoryAddress, MemoryBase,
        MemoryLocation,
    },
    pass::{MirPass, ModuleAnalyses},
    utils::{self, eval},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};

/// Folds internal calls whose executed path uses concrete scalars and known allocation words.
pub(crate) struct ReadonlyEval;

const MAX_FUEL: usize = 1024;
const MAX_WORDS: usize = 64;
const MAX_DEPTH: usize = 4;
type Memory = FxHashMap<MemoryAddress, U256>;
type Environment = FxHashMap<ValueId, Datum>;
type Arguments = IndexVec<ArgIdx, Datum>;

#[derive(Clone, Copy)]
enum EvaluatedReturn {
    Void,
    Word(U256),
}

#[derive(Clone, Copy)]
enum Datum {
    Word(U256),
    Pointer(MemoryAddress),
}

impl Datum {
    fn word(self) -> Option<U256> {
        match self {
            Self::Word(word) => Some(word),
            Self::Pointer(_) => None,
        }
    }
}

impl MirPass for ReadonlyEval {
    fn name(&self) -> &'static str {
        "readonly-eval"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for id in module.functions.indices() {
            let replacements = find_calls(module, id);
            if replacements.is_empty() {
                continue;
            }
            let func = module.function_mut(id);
            let mut values = FxHashMap::default();
            let mut dead = DenseBitSet::<InstId>::new_empty(func.num_insts());
            // result = icall readonly(args); use result => use constant
            // icall checked_void(known_successful_args) => empty
            for (inst, result) in replacements {
                if let EvaluatedReturn::Word(word) = result {
                    let result = func.inst_result_value(inst).unwrap();
                    let immediate = Immediate::for_type(func.inst(inst).result_ty, word);
                    let value = func.alloc_value(Value::Immediate(immediate));
                    values.insert(result, value);
                }
                dead.insert(inst);
            }
            // NOTE: Removed calls intentionally lose their debug checkpoints;
            // no executable instruction is retained to represent the old call.
            func.for_each_instruction_mut(|_, inst| {
                inst.rewrite_operands(|value| *value = utils::resolve_replacement(*value, &values));
            });
            for block in &mut func.blocks {
                block.instructions.retain(|&inst| !dead.contains(inst));
                if let Some(term) = &mut block.terminator {
                    utils::replace_terminator_uses_canonicalized(term, &values);
                }
            }
            changed = true;
        }
        changed
    }
}

fn find_calls(module: &Module, id: FunctionId) -> Vec<(InstId, EvaluatedReturn)> {
    let func = module.function(id);
    let has_allocation =
        func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Alloc { .. }));
    if !func.instructions().any(|inst| {
        matches!(&func.inst(inst).kind, InstKind::ICall { args, returns: 0..=1, .. }
            if has_allocation || args.iter().all(|&arg| func.value(arg).as_immediate().is_some()))
    }) {
        return Vec::new();
    }
    // Scalar-only evaluation cannot acquire allocation facts. Avoid provenance
    // queries for every unknown operand in these functions.
    let alias = has_allocation.then(|| AliasAnalysis::new(func));
    let mut folded = Vec::new();
    let mut fuel = MAX_FUEL;
    for block in &func.blocks {
        let mut memory = Memory::default();
        let mut env = Environment::default();
        for &inst in &block.instructions {
            if fuel == 0 {
                return folded;
            }
            let instruction = func.inst(inst);
            let get = |value| {
                operand(func, &env, None, value).or_else(|| {
                    let address = alias.as_ref()?.memory_address(func, value)?;
                    matches!(address.base, MemoryBase::Allocation(_))
                        .then_some(Datum::Pointer(address))
                })
            };
            if let InstKind::ICall { function, args, returns: 0..=1 } = &instruction.kind
                && let Some(args) = args.iter().map(|&arg| get(arg)).collect::<Option<Arguments>>()
                && let Some(result) = evaluate(module, *function, &args, &memory, &mut fuel, 0)
            {
                if let EvaluatedReturn::Word(word) = result {
                    env.insert(func.inst_result_value(inst).unwrap(), Datum::Word(word));
                }
                folded.push((inst, result));
                continue;
            }
            let result = scalar(&instruction.kind, get, &memory);
            if let Some(alias) = &alias {
                let effects = alias.instruction_mod_ref(func, inst);
                if effects.writes_space(AddressSpace::Memory) {
                    memory.retain(|address, _| {
                        !effects.may_write(
                            alias,
                            Location::Memory(MemoryLocation::new(
                                *address,
                                LocationSize::Const(32),
                            )),
                        )
                    });
                }
                if let InstKind::MStore(address, value) = instruction.kind
                    && let Some(word) = operand(func, &env, None, value).and_then(Datum::word)
                    && let Some(address) = alias.memory_address(func, address)
                    && matches!(address.base, MemoryBase::Allocation(_))
                {
                    if memory.len() == MAX_WORDS {
                        memory.clear();
                    }
                    memory.insert(address, word);
                }
            }
            if let Some(result) = result
                && let Some(value) = func.inst_result_value(inst)
            {
                env.insert(value, result);
            }
        }
    }
    folded
}

fn operand(
    func: &Function,
    env: &Environment,
    args: Option<&Arguments>,
    value: ValueId,
) -> Option<Datum> {
    if let Some(value) = env.get(&value) {
        return Some(*value);
    }
    match func.value(value) {
        Value::Arg(index) => args?.get(*index).copied(),
        Value::Immediate(value) => value.as_u256().map(Datum::Word),
        _ => None,
    }
}

fn scalar(
    kind: &InstKind,
    get: impl Fn(ValueId) -> Option<Datum>,
    memory: &Memory,
) -> Option<Datum> {
    match *kind {
        InstKind::MLoad(address) => {
            let Datum::Pointer(address) = get(address)? else { return None };
            return memory.get(&address).copied().map(Datum::Word);
        }
        InstKind::Add(a, b) => {
            if let (Some(Datum::Pointer(address)), Some(Datum::Word(offset)))
            | (Some(Datum::Word(offset)), Some(Datum::Pointer(address))) = (get(a), get(b))
            {
                return address.checked_add(offset.try_into().ok()?).map(Datum::Pointer);
            }
        }
        InstKind::Select(condition, yes, no) => {
            return get(if get(condition)?.word()?.is_zero() { no } else { yes });
        }
        _ => {}
    }
    eval::eval_inst(kind, |value| get(value).and_then(Datum::word).ok_or(()))
        .ok()
        .flatten()
        .map(Datum::Word)
}

fn evaluate(
    module: &Module,
    id: FunctionId,
    args: &Arguments,
    memory: &Memory,
    fuel: &mut usize,
    depth: usize,
) -> Option<EvaluatedReturn> {
    let func = module.function(id);
    if depth >= MAX_DEPTH
        || args.len() != func.params.len()
        || func.returns.len() > 1
        || func.returns.first().is_some_and(|ty| {
            !matches!(
                ty,
                MirType::UInt(_)
                    | MirType::Int(_)
                    | MirType::Bool
                    | MirType::Address
                    | MirType::FixedBytes(_)
                    | MirType::Function
            )
        })
    {
        return None;
    }
    let mut env = Environment::default();
    let mut current = BlockId::ENTRY;
    let mut predecessor = None;
    loop {
        *fuel = fuel.checked_sub(1)?;
        let block = &func.blocks[current];
        let mut phis = Vec::new();
        for &inst in &block.instructions {
            if let InstKind::Phi(incoming) = &func.inst(inst).kind {
                let pred = predecessor?;
                let (_, value) = incoming.iter().find(|(block, _)| *block == pred)?;
                phis.push((
                    func.inst_result_value(inst)?,
                    operand(func, &env, Some(args), *value)?,
                ));
            }
        }
        env.extend(phis);
        for &inst in &block.instructions {
            *fuel = fuel.checked_sub(1)?;
            let kind = &func.inst(inst).kind;
            if matches!(kind, InstKind::Phi(_)) {
                continue;
            }
            let get = |value| operand(func, &env, Some(args), value);
            let result = if let InstKind::ICall { function, args, returns: 0..=1 } = kind {
                let args = args.iter().map(|&arg| get(arg)).collect::<Option<Arguments>>()?;
                match evaluate(module, *function, &args, memory, fuel, depth + 1)? {
                    EvaluatedReturn::Void => continue,
                    EvaluatedReturn::Word(word) => Datum::Word(word),
                }
            } else {
                scalar(kind, get, memory)?
            };
            env.insert(func.inst_result_value(inst)?, result);
        }
        let next = match block.terminator.as_ref()? {
            Terminator::Jump(target) => *target,
            Terminator::Branch { condition, then_block, else_block } => {
                if operand(func, &env, Some(args), *condition)?.word()?.is_zero() {
                    *else_block
                } else {
                    *then_block
                }
            }
            Terminator::Return { values } if values.len() == func.returns.len() => {
                return match values.as_slice() {
                    [] => Some(EvaluatedReturn::Void),
                    [value] => {
                        operand(func, &env, Some(args), *value)?.word().map(EvaluatedReturn::Word)
                    }
                    _ => None,
                };
            }
            Terminator::Stop if func.returns.is_empty() => return Some(EvaluatedReturn::Void),
            _ => return None,
        };
        predecessor = Some(current);
        current = next;
    }
}
