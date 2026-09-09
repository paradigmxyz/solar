//! Evaluate read-only internal calls over explicitly initialized fresh memory.
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
//! A shared per-caller fuel budget and nesting limit bound recursive and looping
//! evaluation. The memory table holds at most 64 words to bound alias filtering.
//!
//! Successful single-result calls become typed constants. No allocation or
//! initialization is removed here, and no code is moved or cloned. Run after
//! semantic memory lowering and allocation coalescing, before raw allocations
//! lose their provenance. Facts do not cross caller block boundaries.

use crate::mir::{
    ArgIdx, BlockId, Function, FunctionId, Immediate, InstId, InstKind, Module, Terminator, Value,
    ValueId,
    analysis::{
        AddressSpace, AliasAnalysis, Location, LocationSize, MemoryAddress, MemoryBase,
        MemoryLocation,
    },
    pass::{MirPass, ModuleAnalyses},
    utils::{self, eval},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};

/// Folds internal calls whose executed path only reads known allocation words.
pub(crate) struct ReadonlyEval;

const MAX_FUEL: usize = 1024;
const MAX_WORDS: usize = 64;
const MAX_DEPTH: usize = 4;
type Memory = FxHashMap<MemoryAddress, U256>;
type Environment = FxHashMap<ValueId, Datum>;
type Arguments = IndexVec<ArgIdx, Datum>;

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
            // result = icall readonly(args); use result -> use constant
            for (inst, word) in replacements {
                let result = func.inst_result_value(inst).unwrap();
                let immediate = Immediate::for_type(func.inst(inst).result_ty, word);
                let value = func.alloc_value(Value::Immediate(immediate));
                values.insert(result, value);
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

fn find_calls(module: &Module, id: FunctionId) -> Vec<(InstId, U256)> {
    let func = module.function(id);
    if !func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Alloc { .. }))
        || !func
            .instructions()
            .any(|inst| matches!(func.inst(inst).kind, InstKind::ICall { returns: 1, .. }))
    {
        return Vec::new();
    }
    let alias = AliasAnalysis::new(func);
    let mut folded = Vec::new();
    let mut fuel = MAX_FUEL;
    for block in &func.blocks {
        let mut memory = Memory::default();
        let mut env = Environment::default();
        for &inst in &block.instructions {
            let instruction = func.inst(inst);
            let get = |value| {
                operand(func, &env, None, value).or_else(|| {
                    let address = alias.memory_address(func, value)?;
                    matches!(address.base, MemoryBase::Allocation(_))
                        .then_some(Datum::Pointer(address))
                })
            };
            if let InstKind::ICall { function, args, returns: 1 } = &instruction.kind
                && !memory.is_empty()
                && let Some(args) = args.iter().map(|&arg| get(arg)).collect::<Option<Arguments>>()
                && let Some(word) = evaluate(module, *function, &args, &memory, &mut fuel, 0)
            {
                env.insert(func.inst_result_value(inst).unwrap(), Datum::Word(word));
                folded.push((inst, word));
                continue;
            }
            let result = scalar(&instruction.kind, get, &memory);
            let effects = alias.instruction_mod_ref(func, inst);
            if effects.writes_space(AddressSpace::Memory) {
                memory.retain(|address, _| {
                    !effects.may_write(
                        &alias,
                        Location::Memory(MemoryLocation::new(*address, LocationSize::Const(32))),
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
) -> Option<U256> {
    let func = module.function(id);
    if depth >= MAX_DEPTH || args.len() != func.params.len() || func.returns.len() != 1 {
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
            let result = if let InstKind::ICall { function, args, returns: 1 } = kind {
                let args = args.iter().map(|&arg| get(arg)).collect::<Option<Arguments>>()?;
                Datum::Word(evaluate(module, *function, &args, memory, fuel, depth + 1)?)
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
            Terminator::Return { values } if values.len() == 1 => {
                return operand(func, &env, Some(args), values[0])?.word();
            }
            _ => return None,
        };
        predecessor = Some(current);
        current = next;
    }
}
