//! Local physical-stack and literal optimizations.
//!
//! Peepholes use bounded adjacent identities in EVM pop order, respecting opaque
//! stack metadata and unresolved deferred values. Constant evaluation delegates
//! to the retained 256-bit evaluator. Compact literals use the shared bounded
//! materializer; construction-base reuse considers only the preceding literal
//! and checks the complete adjacent pair's stack and byte/gas costs. Gas mode can
//! then cache one repeated PUSH/NOT result within the selected physical body,
//! paying its stack transport and preserving the exact entry/exit boundary under
//! full-block cost and capacity checks. Terminal cleanup removes only
//! an unobserved pure suffix, stopping at effects or unknown stack contracts. Stack-only
//! normalization symbolically executes permutations and asks the private scheduler for a cheaper
//! equivalent. All changes happen on explicit block instructions before primitive assembly;
//! no operation moves across a control-flow edge or mutable observation. The final
//! `late-dce` configuration uses the same DCE traversal but permits calldata unit-carry
//! specialization in Size mode after tail sharing; Gas mode permits it throughout.
//! Earlier Size cleanup retains the common arithmetic shape for sharing. Late-DCE alone
//! also permits disjoint store-pair reordering, using its existing traversal. Earlier
//! cleanup and all scheduling queries disable that rule independently of literal permissions.
//! Raw JUMPDESTs are alternate entries: stack identities and height proofs stop
//! there even when the textual block continues.
//!
//! Scheduling estimates normally disable literal-copy permission. The final resident-operand
//! trial compares conservative, literal-aware and finally oriented query copies, retaining strict
//! conservative improvement and nonincreasing later estimates. Each state also prices literal
//! constructions within its relative peak using the existing allocation-free planner. Absolute
//! entry facts, adjacent complement reuse and literal caching are not modeled. Orientation and
//! construction queries do not simulate their complete executable pass order. These estimates
//! never grant a module permission or change the established scheduling and outlining costs;
//! complete generated-code measurements remain necessary.

use super::{EvmPass, InstKind, Instruction, Module, immediate, verify};
use crate::backend::evm::op;
use solar_config::OptimizationMode;
use solar_sema::Gcx;

mod cse;
mod dead_copies;
mod environment;
mod literal_cache;
mod memory_roundtrip;
mod orientation;
mod peephole;
mod stack;
mod terminal;

pub(super) use environment::EnvironmentCopies;
pub(super) use orientation::LiteralOrientation;
pub(super) use terminal::TerminalPrefixes;

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
        let pass = if self.0 == "late-dce" { "dce" } else { self.0 };
        let calldata_carry = gcx.sess.opts.optimization.is_gas() || self.0 == "late-dce";
        let store_pairs = self.0 == "late-dce";
        let version = gcx.sess.opts.evm_version;
        let mut changed = false;
        // The literal/copy rules shorten code. Private labels are control-only,
        // but parsed labels, numeric jumps and code/gas observations can expose it.
        // Conservatively include gas forwarded to external calls and creations.
        // This permission concerns these literal rules, not gas invariance of the pipeline.
        let literal_copy_order =
            matches!(pass, "dce" | "peephole" | "compact-pushes" | "stack-normalize")
                && literal_observers_allow(module);
        let facts = (matches!(
            pass,
            "compact-pushes" | "reorder-pushes" | "dce" | "peephole" | "stack-normalize"
        ))
        .then(|| verify::stack_facts(module).ok())
        .flatten();
        let literal_copy_order =
            literal_copy_order && facts.as_ref().is_some_and(|(_, unknown)| !*unknown);
        let heights = facts.map(|(heights, _)| heights);
        let reachable = matches!(
            pass,
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
            match pass {
                "compact-pushes" => {
                    let old = std::mem::take(&mut block.insts);
                    let peak = stack_usage(&old).map(|(_, _, peak)| peak);
                    let mut relative_height = Some(0i64);
                    let mut height = entry_max;
                    let mut previous_literal = None;
                    // push value -> <compact literal construction>
                    for inst in old {
                        if matches!(inst.kind, InstKind::Op(op::JUMPDEST)) {
                            height = None;
                            relative_height = None;
                        }
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
                            let budget = literal_budget(height, peak, relative_height);
                            let mut replacement =
                                immediate::materialize_bounded(version, value, budget);
                            if literal_copy_order
                                && let Some((previous, start)) = previous_literal
                                && matches!(replacement.as_slice(), [
                                    Instruction { kind: InstKind::Push(base), .. },
                                    Instruction { kind: InstKind::Op(op::NOT), .. }, ..
                                ] if !*base == previous)
                            {
                                let mut pair = block.insts[start..].to_vec();
                                let split = pair.len();
                                pair.extend_from_slice(&replacement);
                                let old_usage = stack_usage(&pair);
                                let old_cost = immediate::cost(version, &pair);
                                // <previous literal>; push ~previous; not; <construction tail>
                                // <previous literal>; dup1; <construction tail>
                                pair.splice(split..split + 2, [InstKind::Dup(1).into()]);
                                let cost = immediate::cost(version, &pair);
                                if let Some((need, net, peak)) = stack_usage(&pair)
                                    && let Some((old_need, old_net, old_peak)) = old_usage
                                    && need <= old_need
                                    && net == old_net
                                    && peak <= old_peak
                                    && cost.0 < old_cost.0
                                    && cost.1 <= old_cost.1
                                {
                                    // dup1; <construction tail>
                                    replacement.splice(..2, [InstKind::Dup(1).into()]);
                                }
                            }
                            previous_literal = Some((value, block.insts.len()));
                            inherit_debug(std::slice::from_ref(&inst), &mut replacement);
                            changed |= replacement.as_slice() != [inst];
                            block.insts.extend(replacement);
                        } else {
                            previous_literal = None;
                            block.insts.push(inst);
                        }
                        height = next_height;
                        relative_height = next_relative;
                    }
                    if literal_copy_order && gcx.sess.opts.optimization.is_gas() {
                        // <original entry>; <cached literal transport>; <exact original exit>
                        changed |= literal_cache::reuse(
                            &mut block.insts,
                            version,
                            entry_max,
                            module.debug_info_tracked,
                        );
                    }
                }
                "dce" => {
                    changed |= peephole(
                        &mut block.insts,
                        version,
                        entry_max,
                        literal_copy_order,
                        calldata_carry,
                        store_pairs,
                    );
                    changed |= dead_copies::eliminate(&mut block.insts, version);
                    changed |= dedup_stack(&mut block.insts, version);
                    changed |= peephole(
                        &mut block.insts,
                        version,
                        entry_max,
                        literal_copy_order,
                        calldata_carry,
                        store_pairs,
                    );
                    changed |= dead_tail(&mut block.insts, &block.terminator.kind, entry_max);
                    changed |=
                        terminal_pops(&mut block.insts, &block.terminator.kind, entry_max, version);
                }
                "stack-normalize" => {
                    changed |= normalize(&mut block.insts, version, entry_max, literal_copy_order)
                }
                "stack-dedup" => changed |= dedup_stack(&mut block.insts, version),
                "reorder-pushes" => {
                    changed |=
                        reorder(&mut block.insts, version, entry_max, gcx.sess.opts.optimization)
                }
                "block-cse" => changed |= common_expressions(&mut block.insts, version),
                "peephole" => {
                    changed |= peephole(
                        &mut block.insts,
                        version,
                        entry_max,
                        literal_copy_order,
                        calldata_carry,
                        false,
                    );
                    changed |= dedup_stack(&mut block.insts, version);
                }
                _ => unreachable!(),
            }
        }
        if pass == "dce"
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
                        peephole::self_contained_terminal_peak(&module.blocks[target], version)
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

/// Checks static code/gas observers; computed transfers need a separate proof.
fn literal_observers_allow(module: &Module) -> bool {
    module.block_ids().all(|id| {
        module.blocks[id].insts.iter().all(|inst| {
            !matches!(inst.kind, InstKind::PushData { .. } | InstKind::PushDeferred(_))
                && (!matches!(inst.kind, InstKind::PushLabel(_)) || module.private_control_labels)
                && !matches!(inst.kind, InstKind::Op(code)
                if op::stack_io(code).is_none()
                    || matches!(code, op::PC | op::GAS | op::CODESIZE | op::CODECOPY
                        | op::EXTCODECOPY | op::EXTCODESIZE | op::EXTCODEHASH
                        | op::JUMP | op::JUMPI | op::JUMPDEST
                        | op::CALL | op::CALLCODE | op::DELEGATECALL | op::STATICCALL
                        | op::EXTCALL | op::EXTDELEGATECALL | op::EXTSTATICCALL
                        | op::CREATE | op::CREATE2 | op::EOFCREATE))
        })
    })
}

fn canonical(inst: &Instruction) -> bool {
    !inst.keep_with_next
        && !matches!(inst.kind, InstKind::Op(op::JUMPDEST))
        && inst.stack_effect.is_none_or(|actual| verify::effect(&inst.kind) == Some(actual))
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
    mut replacement: Vec<Instruction>,
) -> bool {
    if !super::split_allowed(insts, start)
        || !super::split_allowed(insts, start + len)
        || insts[start..start + len] == replacement
    {
        return false;
    }
    inherit_debug(&insts[start..start + len], &mut replacement);
    // <matched physical instructions> -> <equivalent replacement>
    insts.splice(start..start + len, replacement);
    true
}

/// Carries the bounded union of a rewritten sequence's known source origins.
fn inherit_debug(original: &[Instruction], replacement: &mut [Instruction]) {
    if replacement.is_empty() {
        return;
    }
    let mut sources = original.iter().filter_map(|inst| inst.debug.as_deref());
    let Some(first) = sources.next() else { return };
    let mut debug = first.clone();
    for source in sources {
        debug.merge(source);
    }
    // NOTE: Generated operand helpers without an origin add no source span.
    // Intermediate function events have no reliable checkpoint after a rewrite;
    // retain only events at the original sequence boundaries, without extra code.
    debug.function_invoke = None;
    debug.function_exit = None;
    for inst in replacement.iter_mut() {
        inst.debug = Some(Box::new(debug.clone()));
    }
    replacement[0].debug.as_mut().unwrap().function_invoke = original
        .first()
        .and_then(|inst| inst.debug.as_deref())
        .and_then(|debug| debug.function_invoke);
    replacement.last_mut().unwrap().debug.as_mut().unwrap().function_exit = original
        .last()
        .and_then(|inst| inst.debug.as_deref())
        .and_then(|debug| debug.function_exit);
}

/// Returns required incoming words, net height change and relative peak.
pub(super) fn stack_usage(insts: &[Instruction]) -> Option<(i64, i64, i64)> {
    let mut height = 0i64;
    let mut peak = 0i64;
    let mut required = 0i64;
    for inst in insts {
        if matches!(inst.kind, InstKind::Op(op::JUMPDEST)) {
            return None;
        }
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
    let trial = simplify_schedule(version, input);
    let (bytes, gas) = immediate::cost(version, &trial);
    (gas, bytes)
}

/// Simplifies a physical fragment without increasing its required input or stack peak.
pub(super) fn simplify_schedule(
    version: solar_config::EvmVersion,
    input: &[Instruction],
) -> Vec<Instruction> {
    let mut trial = input.to_vec();
    simplify_schedule_in_place(version, &mut trial, false);
    trial
}

/// Requires strict conservative improvement and nonincrease in later literal estimates.
/// Bounded constructions use relative capacity, without pricing adjacent reuse or caching.
pub(super) fn scheduling_literal_costs_fit(
    version: solar_config::EvmVersion,
    original: &[Instruction],
    candidate: &[Instruction],
) -> bool {
    let mut old = Vec::with_capacity(original.len());
    let mut new = Vec::with_capacity(candidate.len());
    for (literal_copy_order, orient) in [(false, false), (true, false), (true, true)] {
        if orient {
            // <literal-aware query bodies> -> <finally oriented query bodies>
            orientation::orient(&mut old);
            orientation::orient(&mut new);
        } else {
            // <original query bodies> -> <locally simplified query bodies>
            old.clear();
            old.extend_from_slice(original);
            new.clear();
            new.extend_from_slice(candidate);
            simplify_schedule_in_place(version, &mut old, literal_copy_order);
            simplify_schedule_in_place(version, &mut new, literal_copy_order);
        }
        let old_cost = immediate::cost(version, &old);
        let new_cost = immediate::cost(version, &new);
        if new_cost.0 > old_cost.0
            || new_cost.1 > old_cost.1
            || (!literal_copy_order && new_cost == old_cost)
        {
            return false;
        }
        let (Some(old_cost), Some(new_cost)) =
            (bounded_scheduling_cost(version, &old), bounded_scheduling_cost(version, &new))
        else {
            return false;
        };
        if new_cost.0 > old_cost.0 || new_cost.1 > old_cost.1 {
            return false;
        }
    }
    true
}

fn simplify_schedule_in_place(
    version: solar_config::EvmVersion,
    trial: &mut Vec<Instruction>,
    literal_copy_order: bool,
) {
    // <physical sequence> -> <equivalent locally simplified sequence>
    peephole(trial, version, None, literal_copy_order, false, false);
    dead_copies::eliminate(trial, version);
    dedup_stack(trial, version);
    peephole(trial, version, None, literal_copy_order, false, false);
    normalize(trial, version, None, literal_copy_order);
}

fn literal_budget(height: Option<usize>, peak: Option<i64>, relative_height: Option<i64>) -> usize {
    height
        .map(|height| 1024usize.saturating_sub(height))
        .or_else(|| usize::try_from(peak? - relative_height?).ok())
        .unwrap_or(1)
}

fn bounded_scheduling_cost(
    version: solar_config::EvmVersion,
    input: &[Instruction],
) -> Option<(usize, usize)> {
    let (_, _, peak) = stack_usage(input)?;
    let mut height = 0i64;
    let mut cost = (0, 0);
    for inst in input {
        let next = if let InstKind::Push(value) = inst.kind
            && canonical(inst)
        {
            immediate::materialization_cost_bounded(
                version,
                value,
                literal_budget(None, Some(peak), Some(height)),
            )
        } else {
            immediate::cost(version, std::slice::from_ref(inst))
        };
        cost.0 += next.0;
        cost.1 += next.1;
        let (inputs, outputs) = verify::effect(&inst.kind).or(inst.stack_effect)?;
        height = height.checked_sub(i64::from(inputs))?.checked_add(i64::from(outputs))?;
    }
    Some(cost)
}
