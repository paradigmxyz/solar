//! Partially inline small returning guards at internal calls in loops.
//!
//! A void helper whose entry contains only word comparisons and branches to an
//! empty return can be bypassed when that guard succeeds. Clone the entry guard
//! at the call site and retain the original call on the other edge. The helper's
//! stateful or expensive body remains shared, and its failure behavior is unchanged.
//! Reads, gas observations, frame accesses, phis, and custom effects in the guard
//! are excluded. Nominal boolean types do not imply clean words: explicit
//! normalization instructions are copied with the comparisons.
//!
//! This is a gas-mode loop optimization, bounded to four sites per caller, four
//! scalar arguments, and six comparison instructions per guard. It trades a small
//! caller-side guard for avoiding the internal call protocol on the returning
//! path. The other path pays an extra guard, so this is deliberately not a general
//! inliner. Run once after memory lowering and local CSE; subsequent CFG and word
//! cleanup can simplify the copied comparisons.

use crate::mir::{
    BlockId, EffectKind, Function, InstKind, Instruction, MirType, Module, Terminator, Value,
    ValueId,
    analysis::CfgInfo,
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_sema::Gcx;

pub(crate) struct InlineGuards;

struct Guard {
    function: Function,
    condition: ValueId,
    returns_if: bool,
}

impl MirPass for InlineGuards {
    fn name(&self) -> &'static str {
        "inline-guards"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        gcx.sess.opts.optimization.is_gas()
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, _: &mut ModuleAnalyses) -> bool {
        let guards = module
            .functions
            .iter_enumerated()
            .filter_map(|(id, func)| returning_guard(func).map(|guard| (id, guard)))
            .collect::<FxHashMap<_, _>>();
        if guards.is_empty() {
            return false;
        }
        let mut changed = false;
        for (caller, func) in module.functions.iter_mut_enumerated() {
            let mut sites = Vec::new();
            for (block, body) in func.blocks.iter_enumerated() {
                for (index, &inst) in body.instructions.iter().enumerate() {
                    if let InstKind::ICall { function, args, returns: 0 } = &func.inst(inst).kind
                        && *function != caller
                        && let Some(guard) = guards.get(function)
                        && args.len() == guard.function.params.len()
                        && func
                            .inst(inst)
                            .metadata
                            .effect()
                            .is_none_or(|effect| effect == func.inst(inst).kind.effect_kind())
                    {
                        sites.push((block, index, *function));
                    }
                }
            }
            if sites.is_empty() {
                continue;
            }
            let cfg = CfgInfo::new(func);
            sites.retain(|&(block, _, _)| cfg.cyclic_blocks().contains(block));
            sites.truncate(4);
            // Splitting later calls first preserves the earlier instruction indices.
            for (block, index, callee) in sites.into_iter().rev() {
                split_guard(func, block, index, &guards[&callee]);
                changed = true;
            }
        }
        changed
    }
}

fn returning_guard(func: &Function) -> Option<Guard> {
    if func.blocks.is_empty()
        || !func.returns.is_empty()
        || func.params.len() > 4
        || func.params.iter().any(|ty| matches!(ty, MirType::Slice(_) | MirType::Void))
        || func.internal_frame_size != 0
        || func.attributes.no_inline
        || func.is_public()
        || func.selector.is_some()
        || func.attributes.is_constructor
        || func.attributes.is_receive
        || func.attributes.is_fallback
        || func.instructions().take(65).count() > 64
    {
        return None;
    }
    let entry = &func.blocks[BlockId::ENTRY];
    let Terminator::Branch { condition, then_block, else_block } = entry.terminator.clone()? else {
        return None;
    };
    if then_block == else_block || entry.instructions.len() > 6 {
        return None;
    }
    let returns = |block: BlockId| {
        let block = &func.blocks[block];
        block.instructions.is_empty()
            && match &block.terminator {
                Some(Terminator::Stop) => true,
                Some(Terminator::Return { values }) => values.is_empty(),
                _ => false,
            }
    };
    let returns_if = match (returns(then_block), returns(else_block)) {
        (true, false) => true,
        (false, true) => false,
        _ => return None,
    };
    let mut defined = DenseBitSet::new_empty(func.num_values());
    for &inst in &entry.instructions {
        let instruction = func.inst(inst);
        if !matches!(
            instruction.kind,
            InstKind::Eq(..) | InstKind::Lt(..) | InstKind::Gt(..) | InstKind::IsZero(..)
        ) || instruction.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
            || !instruction.operands().into_iter().all(|value| guard_operand(func, value, &defined))
        {
            return None;
        }
        defined.insert(func.inst_result_value(inst)?);
    }
    guard_operand(func, condition, &defined).then(|| Guard {
        function: func.clone(),
        condition,
        returns_if,
    })
}

fn guard_operand(func: &Function, value: ValueId, defined: &DenseBitSet<ValueId>) -> bool {
    match func.value(value) {
        Value::Arg(index) => index.index() < func.params.len(),
        Value::Immediate(_) => true,
        Value::Inst(_) => defined.contains(value),
        _ => false,
    }
}

fn clone_value(
    caller: &mut Function,
    callee: &Function,
    args: &[ValueId],
    values: &mut FxHashMap<ValueId, ValueId>,
    value: ValueId,
) -> ValueId {
    *values.entry(value).or_insert_with(|| match callee.value(value) {
        Value::Arg(index) => args[index.index()],
        // callee.const c => caller.const c
        Value::Immediate(imm) => caller.alloc_value(Value::Immediate(imm.clone())),
        _ => unreachable!("guard operands were validated in definition order"),
    })
}

fn split_guard(caller: &mut Function, block: BlockId, index: usize, guard: &Guard) {
    let inst = caller.blocks[block].instructions[index];
    let InstKind::ICall { args, .. } = caller.inst(inst).kind.clone() else { unreachable!() };
    let callee = &guard.function;
    let mut values = FxHashMap::default();
    let mut instructions = Vec::new();
    // icall helper(args)
    // => condition = helper.entry(args)
    //    branch condition, continuation, call
    // call: icall helper(args); jump continuation
    for &source in &callee.blocks[BlockId::ENTRY].instructions {
        let source_inst = callee.inst(source);
        let mut kind = source_inst.kind.clone();
        kind.visit_operands_mut(|value| {
            *value = clone_value(caller, callee, &args, &mut values, *value);
        });
        let mut instruction = Instruction::new(kind, source_inst.result_ty);
        instruction.metadata.copy_debug_context(&source_inst.metadata);
        let (new_inst, new_value) = caller.alloc_value_inst(instruction);
        values.insert(callee.inst_result_value(source).unwrap(), new_value);
        instructions.push(new_inst);
    }
    let condition = clone_value(caller, callee, &args, &mut values, guard.condition);
    let continuation = caller.alloc_block();
    let call = caller.alloc_block();
    let (terminator, metadata) = caller.blocks[block].take_terminator();
    let successors = terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    // continuation: suffix; original_terminator
    caller.blocks[continuation].instructions =
        caller.blocks[block].instructions.split_off(index + 1);
    caller.blocks[block].instructions.pop();
    caller.blocks[block].instructions.extend(instructions);
    if let Some(terminator) = terminator {
        caller.blocks[continuation].set_terminator(terminator, metadata);
    }
    // call: icall helper(args); jump continuation
    caller.blocks[call].instructions.push(inst);
    caller.blocks[call].set_generated_terminator(Terminator::Jump(continuation));
    // branch condition, continuation, call (or reversed for a false-edge return)
    let (then_block, else_block) =
        if guard.returns_if { (continuation, call) } else { (call, continuation) };
    let metadata = caller.inst(inst).metadata.debug_context();
    caller.blocks[block]
        .set_terminator(Terminator::Branch { condition, then_block, else_block }, metadata);
    caller.blocks[call].predecessors.push(block);
    caller.blocks[continuation].predecessors.extend([block, call]);
    // successor: phi [..., block: value] => phi [..., continuation: value]
    for successor in successors {
        for pred in &mut caller.blocks[successor].predecessors {
            if *pred == block {
                *pred = continuation;
            }
        }
        for source in caller.blocks[successor].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut caller.inst_mut(source).kind {
                for (pred, _) in incoming {
                    if *pred == block {
                        *pred = continuation;
                    }
                }
            }
        }
    }
}
