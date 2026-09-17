//! Specialize small pure loops on an invariant zero-divisor guard.
//!
//! Recognize a loop-invariant `x == 0` used in a boolean disjunction inside
//! a loop. Branch on that existing predicate in the preheader, clone the loop
//! for the zero case, and replace the predicate with its known truth value
//! in each copy. The zero copy also substitutes zero for `x`, exposing checked
//! multiplication guards and arithmetic to the following scalar cleanup.
//!
//! Only nonnested loops of at most 32 pure instructions qualify. The header
//! has one terminal normal exit with no other predecessors and at most eight
//! instructions; that exit is copied too, avoiding a new merge of loop-carried
//! values. Other exits must terminate immediately, and no loop definition may
//! escape except into the copied normal exit. Other escaping SSA shapes,
//! memory operations, calls, observations, and instruction effect overrides
//! prevent specialization. Only the pure equality may move to the preheader;
//! a zero-trip loop cannot acquire a panic or other observable body behavior.
//!
//! Gas mode accepts bounded duplication to remove repeated guard work. Each
//! function is specialized at most once per invocation, before late scalar
//! simplification and CFG cleanup. This is not general loop unswitching or
//! an argument that duplicating arbitrary loops is profitable.

use crate::mir::{
    BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, Module,
    Terminator, Value, ValueId,
    analysis::{Loop, LoopAnalyzer, LoopInfo},
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_sema::Gcx;

pub(crate) struct LoopUnswitch;

impl MirPass for LoopUnswitch {
    fn name(&self) -> &'static str {
        "loop-unswitch"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _: &Module) -> bool {
        gcx.sess.opts.optimization.is_gas()
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| {
            if !func.instructions().any(|inst| {
                func.inst(inst).result_ty == Some(MirType::I1)
                    && matches!(func.inst(inst).kind, InstKind::Or(..))
            }) {
                return false;
            }
            let mut analyzer = LoopAnalyzer::new();
            let loops = analyzer.analyze(func);
            let candidate =
                loops.all_loops().find_map(|loop_info| plan(func, &analyzer, &loops, loop_info));
            let Some(candidate) = candidate else { return false };
            apply(func, candidate);
            true
        })
    }
}

struct Candidate {
    header: BlockId,
    preheader: BlockId,
    blocks: DenseBitSet<BlockId>,
    predicate: ValueId,
    zero_value: ValueId,
    hoist: Option<(BlockId, InstId)>,
}

fn plan(
    func: &Function,
    analyzer: &LoopAnalyzer,
    loops: &LoopInfo,
    loop_info: &Loop,
) -> Option<Candidate> {
    let preheader = loop_info.preheader?;
    let header = loop_info.header;
    if func.blocks[preheader].terminator != Some(Terminator::Jump(header))
        || loops
            .all_loops()
            .any(|other| other.header != header && loop_info.blocks.contains(other.header))
    {
        return None;
    }
    let Some(Terminator::Branch { then_block, else_block: exit, .. }) =
        func.blocks[header].terminator
    else {
        return None;
    };
    if !loop_info.blocks.contains(then_block)
        || loop_info.blocks.contains(exit)
        || func.blocks[exit].predecessors.as_slice() != [header]
    {
        return None;
    }
    if func.blocks[exit].instructions.len() > 8
        || !func.blocks[exit].terminator.as_ref()?.successors().is_empty()
        || func.blocks[exit].instructions.iter().any(|&inst| {
            func.inst(inst)
                .metadata
                .effect()
                .is_some_and(|effect| effect != func.inst(inst).kind.effect_kind())
        })
    {
        return None;
    }
    for &other in &loop_info.exit_blocks {
        if func.blocks[other]
            .instructions
            .iter()
            .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
        {
            return None;
        }
        if other != exit
            && !matches!(
                func.blocks[other].terminator,
                Some(Terminator::Revert { .. } | Terminator::Invalid)
            )
        {
            return None;
        }
    }
    let mut instructions = DenseBitSet::<InstId>::new_empty(func.num_insts());
    for block in &loop_info.blocks {
        for &inst in &func.blocks[block].instructions {
            let instruction = func.inst(inst);
            if instruction.kind.effect_kind() != EffectKind::Pure
                || instruction.kind.effects().control.any()
                || instruction.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
                || !matches!(instruction.result_ty, Some(MirType::I1 | MirType::I256))
            {
                return None;
            }
            instructions.insert(inst);
        }
    }
    if instructions.count() > 32 {
        return None;
    }
    let mut guard = None;
    for inst in &instructions {
        if let InstKind::Or(left, right) = func.inst(inst).kind {
            for predicate in [left, right] {
                if let Value::Inst(def) = func.value(predicate)
                    && func.value_ty(predicate) == Some(MirType::I1)
                    && matches!(func.inst(*def).kind, InstKind::Eq(..))
                {
                    let Some(zero_value) = func.inst(*def).kind.zero_test_operand(func) else {
                        continue;
                    };
                    if func.value_ty(zero_value) != Some(MirType::I256)
                        || func.value_u256(zero_value).is_some()
                    {
                        continue;
                    }
                    let owner = func
                        .blocks
                        .iter_enumerated()
                        .find(|(_, block)| block.instructions.contains(def))?
                        .0;
                    let available = match func.value(zero_value) {
                        Value::Arg(_) => true,
                        Value::Inst(value_def) if !instructions.contains(*value_def) => {
                            func.blocks.iter_enumerated().any(|(block, body)| {
                                body.instructions.contains(value_def)
                                    && analyzer.dominates(block, preheader)
                            })
                        }
                        _ => false,
                    };
                    if available
                        && (loop_info.blocks.contains(owner)
                            || analyzer.dominates(owner, preheader))
                    {
                        let hoist = loop_info.blocks.contains(owner).then_some((owner, *def));
                        guard = Some((predicate, zero_value, hoist));
                        break;
                    }
                }
            }
        }
        if guard.is_some() {
            break;
        }
    }
    let (predicate, zero_value, hoist) = guard?;
    for (block, body) in func.blocks.iter_enumerated() {
        if loop_info.blocks.contains(block) || block == exit {
            continue;
        }
        let escapes =
            |value| matches!(func.value(value), Value::Inst(def) if instructions.contains(*def));
        if body.instructions.iter().any(|&inst| func.inst(inst).operands().into_iter().any(escapes))
            || body.terminator.as_ref()?.operands().into_iter().any(escapes)
        {
            return None;
        }
    }
    let mut blocks = loop_info.blocks.clone();
    blocks.insert(exit);
    Some(Candidate { header, preheader, blocks, predicate, zero_value, hoist })
}

fn apply(func: &mut Function, candidate: Candidate) {
    if let Some((block, inst)) = candidate.hoist {
        func.blocks[block].instructions.retain(|&id| id != inst);
        func.blocks[candidate.preheader].instructions.push(inst);
    }
    let mut blocks = FxHashMap::default();
    for block in &candidate.blocks {
        blocks.insert(block, func.alloc_block());
    }
    let mut values = FxHashMap::default();
    for block in &candidate.blocks {
        let mut cloned = Vec::new();
        for inst in func.blocks[block].instructions.clone() {
            let original = func.inst(inst);
            let mut instruction = Instruction::new(original.kind.clone(), original.result_ty);
            instruction.metadata.copy_debug_context(&original.metadata);
            let cloned_inst = if let Some(result) = func.inst_result_value(inst) {
                let (cloned_inst, value) = func.alloc_value_inst(instruction);
                values.insert(result, value);
                cloned_inst
            } else {
                func.alloc_inst(instruction)
            };
            cloned.push(cloned_inst);
        }
        func.blocks[blocks[&block]].instructions = cloned;
    }
    let yes = func.alloc_value(Value::Immediate(Immediate::I1(true)));
    let no = func.alloc_value(Value::Immediate(Immediate::I1(false)));
    let zero = func.alloc_value(Value::Immediate(Immediate::I256(alloy_primitives::U256::ZERO)));
    values.insert(candidate.predicate, yes);
    values.insert(candidate.zero_value, zero);
    let mapped = |value| values.get(&value).copied().unwrap_or(value);
    for block in &candidate.blocks {
        let clone = blocks[&block];
        for inst in func.blocks[clone].instructions.clone() {
            let kind = &mut func.inst_mut(inst).kind;
            kind.visit_operands_mut(|value| *value = mapped(*value));
            if let InstKind::Phi(incoming) = kind {
                for (from, _) in incoming {
                    *from = blocks.get(from).copied().unwrap_or(*from);
                }
            }
        }
        let mut term = func.blocks[block].terminator.clone().unwrap();
        term.visit_operands_mut(|value| *value = mapped(*value));
        super::loop_split::retarget(&mut term, |target| {
            blocks.get(&target).copied().unwrap_or(target)
        });
        let metadata = func.blocks[block].terminator_metadata.clone();
        func.blocks[clone].set_terminator(term, metadata);
        for inst in func.blocks[block].instructions.clone() {
            func.inst_mut(inst).kind.visit_operands_mut(|value| {
                if *value == candidate.predicate {
                    *value = no;
                }
            });
        }
        func.blocks[block].terminator.as_mut().unwrap().visit_operands_mut(|value| {
            if *value == candidate.predicate {
                *value = no;
            }
        });
    }
    // NOTE: The new dispatch has no source branch, so its debug origin is intentionally absent.
    func.blocks[candidate.preheader].set_generated_terminator(Terminator::Branch {
        condition: candidate.predicate,
        then_block: blocks[&candidate.header],
        else_block: candidate.header,
    });
    super::loop_split::rebuild_predecessors(func);
}
