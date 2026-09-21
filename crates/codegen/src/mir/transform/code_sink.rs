//! Sink pure expression tails into the only branch that consumes them.
//!
//! For an acyclic conditional block, inspect its final uninterrupted run of
//! pure instructions backwards. Move a definition to a direct successor only
//! when that successor has no other predecessor and contains every use. Moving
//! consumers first lets their producers follow without cloning expressions.
//! Phi operands count as uses on incoming edges, so a value needed by an edge
//! cannot move past that edge. Instruction identities and metadata stay intact.
//!
//! The destination starts with the moved instructions, after its phis and
//! before any effects. Reads, writes, gas and memory-size observations, calls,
//! and control effects stop the source scan. Neither endpoint may belong to a
//! cycle: sinking must not introduce repeated work. This bounded late pass
//! avoids work on the unused branch and shortens live ranges without changing
//! CFG edges or adding instructions. Branch sinking is disabled in functions
//! that may write persistent storage, including through calls or creation:
//! skipped work must not leave extra gas at an SSTORE sentry on another path.
//! At most one distinct nonconstant operand
//! may replace the result across the branch, limiting added stack pressure.
//! It does not attempt shared-code placement,
//! cross-join sinking, cloning, or memory-read motion. The default pipeline
//! enables it only for gas: changed live ranges can enlarge size-oriented code.
//!
//! In blocks of at most 128 instructions, single-use pure expressions may also
//! sink across stores to their next use. Reads stay in their original order;
//! only ordinary memory and transient storage writes may be crossed. SSTORE
//! stays ordered because its gas sentry can reject a low-gas call. Gas, memory-size,
//! calls, control effects, logs, and overridden effects remain barriers. The
//! same one-input pressure bound applies, and no instruction is duplicated.

use crate::mir::{
    BlockId, EffectKind, Function, InstId, InstKind, Module, Terminator, Value, ValueId,
    analysis::CfgInfo,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::index::{IndexVec, index_vec};
use solar_sema::Gcx;

pub(crate) struct CodeSink;

impl MirPass for CodeSink {
    fn name(&self) -> &'static str {
        "code-sink"
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| run(func))
    }
}

#[derive(Clone, Copy)]
enum Use {
    Instruction(InstId),
    Edge(BlockId),
}

fn run(func: &mut Function) -> bool {
    if !func.blocks.iter().any(|block| {
        !block.instructions.is_empty()
            && (matches!(block.terminator, Some(Terminator::Branch { .. }))
                || block.instructions.iter().any(|&inst| movable_store(func.inst(inst))))
    }) {
        return false;
    }
    let may_write_storage = |effect| {
        matches!(
            effect,
            EffectKind::StorageWrite
                | EffectKind::ICall
                | EffectKind::ExternalCall
                | EffectKind::Create
        )
    };
    let can_sink_branches = !func.instructions().any(|inst| {
        let instruction = func.inst(inst);
        may_write_storage(instruction.kind.effect_kind())
            || instruction.metadata.effect().is_some_and(may_write_storage)
    }) && !func
        .blocks
        .iter()
        .any(|block| matches!(block.terminator, Some(Terminator::TailCall { .. })));
    let cfg = CfgInfo::new(func);
    let mut owners = index_vec![BlockId::ENTRY; func.num_insts()];
    let mut users =
        IndexVec::<ValueId, SmallVec<[Use; 2]>>::from_vec(vec![SmallVec::new(); func.num_values()]);
    for (block, body) in func.blocks.iter_enumerated() {
        for &inst in &body.instructions {
            owners[inst] = block;
            let kind = &func.inst(inst).kind;
            if let InstKind::Phi(incoming) = kind {
                for &(from, value) in incoming {
                    users[value].push(Use::Edge(from));
                }
            } else {
                for value in kind.operands() {
                    users[value].push(Use::Instruction(inst));
                }
            }
        }
        if let Some(term) = &body.terminator {
            for value in term.operands() {
                users[value].push(Use::Edge(block));
            }
        }
    }
    let mut changed = false;
    for &block in cfg.rpo().iter().rev() {
        let Some(Terminator::Branch { then_block, else_block, .. }) = func.blocks[block].terminator
        else {
            continue;
        };
        if !can_sink_branches || cfg.cyclic_blocks().contains(block) {
            continue;
        }
        let original = func.blocks[block].instructions.clone();
        let mut sunk = [Vec::new(), Vec::new()];
        for inst in original.into_iter().rev() {
            let instruction = func.inst(inst);
            if matches!(instruction.kind, InstKind::Phi(_) | InstKind::Gas | InstKind::MSize)
                || instruction.kind.effect_kind() != EffectKind::Pure
                || !instruction.kind.effects().can_speculate()
                || instruction.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
            {
                break;
            }
            if !at_most_one_nonconstant_operand(func, &instruction.kind) {
                continue;
            }
            let Some(value) = func.inst_result_value(inst) else { continue };
            let use_block = |usage| match usage {
                Use::Instruction(inst) => owners[inst],
                Use::Edge(block) => block,
            };
            let Some(&first) = users[value].first() else { continue };
            let target = use_block(first);
            if (target != then_block && target != else_block)
                || cfg.cyclic_blocks().contains(target)
                || func.blocks[target].predecessors.as_slice() != [block]
                || users[value].iter().any(|&usage| use_block(usage) != target)
            {
                continue;
            }
            sunk[usize::from(target != then_block)].push(inst);
            owners[inst] = target;
            changed = true;
        }
        func.blocks[block].instructions.retain(|&inst| owners[inst] == block);
        for (target, mut moved) in [then_block, else_block].into_iter().zip(sunk) {
            if moved.is_empty() {
                continue;
            }
            moved.reverse();
            let insert = func.blocks[target]
                .instructions
                .iter()
                .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
                .count();
            func.blocks[target].instructions.splice(insert..insert, moved);
        }
    }
    for block in func.blocks.indices() {
        let instructions = &func.blocks[block].instructions;
        if instructions.len() > 128
            || !instructions.iter().any(|&inst| movable_store(func.inst(inst)))
        {
            continue;
        }
        let original = instructions.clone();
        for inst in original.into_iter().rev() {
            let instruction = func.inst(inst);
            if !instruction.kind.effects().can_speculate()
                || instruction.kind.effect_kind() != EffectKind::Pure
                || matches!(instruction.kind, InstKind::Phi(_))
                || instruction.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
            {
                continue;
            }
            if !at_most_one_nonconstant_operand(func, &instruction.kind) {
                continue;
            }
            let Some(result) = func.inst_result_value(inst) else { continue };
            let [Use::Instruction(consumer)] = users[result].as_slice() else { continue };
            if owners[*consumer] != block {
                continue;
            }
            let instructions = &func.blocks[block].instructions;
            let from = instructions.iter().position(|&id| id == inst).unwrap();
            let to = instructions.iter().position(|id| id == consumer).unwrap();
            if to <= from + 1 {
                continue;
            }
            let crossed = &instructions[from + 1..to];
            if !crossed.iter().any(|&id| movable_store(func.inst(id)))
                || crossed.iter().any(|&id| {
                    let instruction = func.inst(id);
                    !movable_store(instruction)
                        && (!instruction.kind.effects().can_speculate()
                            || instruction.kind.effect_kind() != EffectKind::Pure
                            || instruction
                                .metadata
                                .effect()
                                .is_some_and(|effect| effect != EffectKind::Pure))
                })
            {
                continue;
            }
            let instructions = &mut func.blocks[block].instructions;
            instructions.remove(from);
            instructions.insert(to - 1, inst);
            changed = true;
        }
    }
    changed
}

fn movable_store(instruction: &crate::mir::Instruction) -> bool {
    matches!(instruction.kind, InstKind::MStore(..) | InstKind::MStore8(..) | InstKind::TStore(..))
        && instruction
            .metadata
            .effect()
            .is_none_or(|effect| effect == instruction.kind.effect_kind())
}

fn at_most_one_nonconstant_operand(func: &Function, kind: &InstKind) -> bool {
    let mut operands = kind
        .operands()
        .into_iter()
        .filter(|&value| !matches!(func.value(value), Value::Immediate(_)));
    let Some(first) = operands.next() else { return true };
    operands.all(|value| value == first)
}
