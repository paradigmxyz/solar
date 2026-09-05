//! Local physical-stack and literal optimizations.
//!
//! Peepholes use bounded adjacent identities in EVM pop order, respecting opaque
//! stack metadata and unresolved deferred values. Constant evaluation delegates
//! to the retained 256-bit evaluator. Compact literals use the shared bounded
//! materializer. Terminal cleanup removes only an unobserved pure suffix, stopping
//! at effects or unknown stack contracts. Stack-only normalization symbolically
//! executes permutations and asks the private scheduler for a cheaper equivalent.
//! All changes happen on explicit block instructions before primitive assembly;
//! no operation moves across a control-flow edge or mutable observation.

use super::{EvmPass, InstKind, Instruction, Module, immediate, verify};
use crate::backend::evm::op;
use solar_config::OptimizationMode;
use solar_sema::Gcx;

mod cse;
mod dead_copies;
mod peephole;
mod stack;

use cse::common_expressions;
use peephole::{dead_tail, peephole, terminal_pops};
use stack::{dedup_stack, normalize, reorder};

pub(super) struct LocalPass(pub &'static str);

impl EvmPass for LocalPass {
    fn name(&self) -> &'static str {
        self.0
    }
    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let version = gcx.sess.opts.evm_version;
        let mut changed = false;
        let heights = (matches!(
            self.0,
            "compact-pushes" | "reorder-pushes" | "dce" | "peephole" | "stack-normalize"
        ))
        .then(|| verify::stack_heights(module).ok())
        .flatten();
        let reachable = matches!(
            self.0,
            "compact-pushes" | "reorder-pushes" | "dce" | "peephole" | "stack-normalize"
        )
        .then(|| verify::physical_reachability(module));
        for id in module.block_ids().collect::<Vec<_>>() {
            let entry_max = heights
                .as_ref()
                .and_then(|heights| heights[id].map(|(_, max)| max))
                .or_else(|| {
                    reachable.as_ref().is_some_and(|reachable| !reachable.contains(id)).then_some(0)
                });
            let block = &mut module.blocks[id];
            match self.0 {
                "compact-pushes" => {
                    let old = std::mem::take(&mut block.insts);
                    let peak = stack_usage(&old).map(|(_, _, peak)| peak);
                    let mut relative_height = Some(0i64);
                    let mut height = entry_max;
                    // push value -> <compact literal construction>
                    for inst in old {
                        let next_relative = relative_height.and_then(|height| {
                            let (inputs, outputs) =
                                verify::effect(&inst.kind).or(inst.stack_effect)?;
                            height.checked_sub(i64::from(inputs))?.checked_add(i64::from(outputs))
                        });
                        let next_height = height.and_then(|height| {
                            let (inputs, outputs) =
                                verify::effect(&inst.kind).or(inst.stack_effect)?;
                            height
                                .checked_sub(usize::from(inputs))?
                                .checked_add(usize::from(outputs))
                        });
                        if let InstKind::Push(value) = inst.kind
                            && canonical(&inst)
                        {
                            let budget = height
                                .map(|height| 1024usize.saturating_sub(height))
                                .or_else(|| usize::try_from(peak? - relative_height?).ok())
                                .unwrap_or(1);
                            let replacement =
                                immediate::materialize_bounded(version, value, budget);
                            changed |= replacement.as_slice() != [inst];
                            block.insts.extend(replacement);
                        } else {
                            block.insts.push(inst);
                        }
                        height = next_height;
                        relative_height = next_relative;
                    }
                }
                "dce" => {
                    changed |= peephole(&mut block.insts, version, entry_max);
                    changed |= dead_copies::eliminate(&mut block.insts, version);
                    changed |= dedup_stack(&mut block.insts, version);
                    changed |= peephole(&mut block.insts, version, entry_max);
                    changed |= dead_tail(&mut block.insts, &block.terminator.kind, entry_max);
                    changed |= terminal_pops(&mut block.insts, &block.terminator.kind, entry_max);
                }
                "stack-normalize" => changed |= normalize(&mut block.insts, version, entry_max),
                "stack-dedup" => changed |= dedup_stack(&mut block.insts, version),
                "reorder-pushes" => {
                    changed |= reorder(
                        &mut block.insts,
                        version,
                        entry_max,
                        gcx.sess.opts.optimization,
                    )
                }
                "block-cse" => changed |= common_expressions(&mut block.insts, version),
                "peephole" => {
                    changed |= peephole(&mut block.insts, version, entry_max);
                    changed |= dedup_stack(&mut block.insts, version);
                }
                _ => unreachable!(),
            }
        }
        if self.0 == "dce"
            && let Some(heights) = &heights
        {
            for id in module.block_ids().collect::<Vec<_>>() {
                if let super::TerminatorKind::Jump(target) = module.blocks[id].terminator.kind
                    && target != id
                    && !module.blocks[id].insts.is_empty()
                    && module.blocks[id]
                        .insts
                        .iter()
                        .all(|inst| canonical(inst) && matches!(inst.kind, InstKind::Op(op::POP)))
                    && let Some((_, incoming)) = heights[id]
                    && let Some(peak) =
                        peephole::self_contained_terminal_peak(&module.blocks[target])
                    && incoming as i64 + peak.max(1) <= 1024
                {
                    // pop dead_prefix...; jump <self-contained terminal body>
                    // -> jump <same terminal body>
                    module.blocks[id].insts.clear();
                    changed = true;
                }
            }
        }
        changed
    }
}

fn canonical(inst: &Instruction) -> bool {
    inst.stack_effect.is_none_or(|actual| verify::effect(&inst.kind) == Some(actual))
}

fn swapped(opcode: u8) -> Option<u8> {
    Some(match opcode {
        op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ => opcode,
        op::LT => op::GT,
        op::GT => op::LT,
        op::SLT => op::SGT,
        op::SGT => op::SLT,
        _ => return None,
    })
}

fn pure(opcode: u8) -> bool {
    matches!(opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::CLZ)
}

fn discardable_push(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::Push(_)
            | InstKind::PushLabel(_)
            | InstKind::PushData { .. }
            | InstKind::PushImmutable { .. }
            | InstKind::Op(op::PUSH0)
    )
}

fn rewrite(
    insts: &mut Vec<Instruction>,
    start: usize,
    len: usize,
    replacement: Vec<Instruction>,
) -> bool {
    if insts[start..start + len] == replacement {
        return false;
    }
    // <matched physical instructions> -> <equivalent replacement>
    insts.splice(start..start + len, replacement);
    true
}

/// Returns required incoming words, net height change and relative peak.
fn stack_usage(insts: &[Instruction]) -> Option<(i64, i64, i64)> {
    let mut height = 0i64;
    let mut peak = 0i64;
    let mut required = 0i64;
    for inst in insts {
        let (inputs, outputs) = verify::effect(&inst.kind).or(inst.stack_effect)?;
        let read = match inst.kind {
            InstKind::Dup(depth) => i64::from(depth),
            InstKind::Swap(depth) => i64::from(depth) + 1,
            InstKind::Exchange(a, b) => i64::from(a.max(b)) + 1,
            _ => i64::from(inputs),
        };
        required = required.max(read - height);
        height = height.checked_sub(i64::from(inputs))?.checked_add(i64::from(outputs))?;
        peak = peak.max(height);
    }
    Some((required, height, peak))
}

/// Estimates a physical scheduling trial using the shared local rewrite rules.
pub(super) fn scheduling_cost(
    version: solar_config::EvmVersion,
    input: &[Instruction],
) -> (usize, usize) {
    let mut trial = input.to_vec();
    peephole(&mut trial, version, None);
    dead_copies::eliminate(&mut trial, version);
    dedup_stack(&mut trial, version);
    peephole(&mut trial, version, None);
    normalize(&mut trial, version, None);
    let (bytes, gas) = immediate::cost(version, &trial);
    (gas, bytes)
}
