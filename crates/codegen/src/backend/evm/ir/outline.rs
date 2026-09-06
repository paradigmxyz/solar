//! Outlines profitable repeated physical computations into shared blocks.
//!
//! Candidate windows contain literal pushes, arithmetic and physical stack
//! operations. Size mode additionally permits known memory/storage accesses and
//! logs, preserving their exact order and operands across every call. Calls,
//! computed control, code-relative observations and GAS remain barriers. MSIZE
//! is safe because the call protocol touches only the stack: memory operations
//! and their expansion occur in the same order as before. Their stack contract
//! is derived by abstract height execution; at
//! most sixteen input/output words are accepted. Disjoint occurrences share a
//! block entered with a continuation below their inputs, and the return rotation
//! preserves all outputs. An encoded-size model charges every call, return and
//! continuation label before selecting candidates. Size mode simplifies each
//! complete shared body including its return rotation, and charges and emits
//! that exact sequence so cancellation at their boundary earns its byte savings.
//! Before simplifying, a zero-byte shared-body lower bound rejects candidates
//! whose call protocol alone cannot meet the existing profit margin.
//! Incremental hashes enumerate
//! one window length at a time, retaining the full search with storage linear
//! in original instruction count. Gas mode applies only the
//! best candidate. Size mode retains at most 512 candidates and selects at most
//! sixteen whose original instruction ranges are disjoint. All sites are then
//! split from right to left, so earlier coordinates remain valid without
//! rescanning or nesting newly outlined bodies. Metadata and relocatable
//! observations are excluded. Stack-height prefixes are computed once per block;
//! checking each candidate never rescans instructions preceding its window.

use super::{Block, BlockId, EvmPass, InstKind, Module, TerminatorKind, verify::effect};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHasher},
};
use solar_sema::Gcx;
use std::hash::{Hash, Hasher};

pub(super) struct Outline;

impl EvmPass for Outline {
    fn name(&self) -> &'static str {
        "outline"
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let ids = module.block_ids().collect::<Vec<_>>();
        let Ok(heights) = super::verify::stack_heights(module) else { return false };
        let reachable = super::verify::physical_reachability(module);
        let prefixes = module
            .blocks
            .iter()
            .map(|block| {
                let mut height = 0isize;
                let mut offsets = Vec::with_capacity(block.insts.len() + 1);
                offsets.push(height);
                for inst in &block.insts {
                    let Some((inputs, outputs)) = effect(&inst.kind).or(inst.stack_effect) else {
                        break;
                    };
                    height += outputs as isize - inputs as isize;
                    offsets.push(height);
                }
                offsets
            })
            .collect::<IndexVec<BlockId, Vec<_>>>();
        // Advance every original start by one instruction per round. Only one
        // window length is resident, preserving all candidates with linear storage.
        let mut states = ids
            .iter()
            .flat_map(|&id| {
                (0..module.blocks[id].insts.len())
                    .map(move |start| (id, start, FxHasher::default(), 0))
            })
            .collect::<Vec<_>>();
        let limit = if gcx.sess.opts.optimization.is_size() { 512 } else { 1 };
        let mut ranked = Vec::new();
        for len in 1..=64 {
            let mut groups = FxHashMap::<u64, Vec<Site>>::default();
            states.retain_mut(|(id, start, hash, bytes)| {
                let Some(inst) = module.blocks[*id].insts.get(*start + len - 1) else {
                    return false;
                };
                if inst.stack_effect.is_some()
                    || !outlinable(&inst.kind, gcx.sess.opts.optimization.is_size())
                {
                    return false;
                }
                inst.kind.hash(hash);
                *bytes += size(gcx, &inst.kind);
                if *bytes >= 16 {
                    groups.entry(hash.finish()).or_default().push(Site {
                        id: *id,
                        start: *start,
                        len,
                        parameters: Vec::new(),
                    });
                }
                true
            });
            if states.is_empty() {
                break;
            }
            for (_, candidates) in groups {
                if candidates.len() < 2 {
                    continue;
                }
                let first = &candidates[0];
                let body = module.blocks[first.id].insts[first.start..first.start + first.len]
                    .iter()
                    .map(|inst| inst.kind.clone())
                    .collect::<Vec<_>>();
                let Some((inputs, outputs)) = contract(&body) else { continue };
                let mut sites = Vec::<Site>::new();
                for candidate in candidates {
                    // Hashes identify candidates only; exact equality proves each shared body.
                    if !module.blocks[candidate.id].insts
                        [candidate.start..candidate.start + candidate.len]
                        .iter()
                        .map(|inst| &inst.kind)
                        .eq(&body)
                    {
                        continue;
                    }
                    if sites.last().is_none_or(|last| {
                        last.id != candidate.id || last.start + last.len <= candidate.start
                    }) {
                        sites.push(candidate);
                    }
                }
                sites.retain(|site| peak_fits(&prefixes, &heights, &reachable, site, &body));
                if sites.len() < 2 {
                    continue;
                }
                let bytes = body.iter().map(|inst| size(gcx, inst)).sum::<usize>();
                let before = bytes * sites.len();
                let protocol = sites.len() * (6 + inputs) + 2;
                if before < protocol + 8 {
                    continue;
                }
                let stub = returning_body(gcx, &body, outputs);
                let after = protocol + stub.iter().map(|inst| size(gcx, inst)).sum::<usize>();
                if before < after + 8 {
                    continue;
                }
                let score = (
                    (before - after).saturating_sub(if gcx.sess.opts.optimization.is_gas() {
                        sites.len() * 8
                    } else {
                        0
                    }),
                    body.len(),
                    usize::MAX - sites[0].id.index(),
                    usize::MAX - sites[0].start,
                );
                retain_candidate(&mut ranked, (score, stub, sites, inputs), limit);
            }
        }
        if let Some(parameterized) = parameterized(gcx, module, &prefixes, &heights, &reachable) {
            retain_candidate(&mut ranked, parameterized, limit);
        }
        let selected = disjoint_candidates(ranked);
        if selected.is_empty() {
            return false;
        }
        let mut stubs = Vec::new();
        let mut calls = Vec::new();
        for (_, body, sites, inputs) in selected {
            let stub = Block {
                insts: body.into_iter().map(Into::into).collect(),
                terminator: TerminatorKind::DynamicJump.into(),
                ..Block::default()
            };
            let stub_id = module.append_block(stub);
            stubs.push(stub_id);
            calls.extend(sites.into_iter().map(|site| (site, stub_id, inputs)));
        }
        let mut continuations = Vec::new();
        // Split each source block from right to left, retaining source block order.
        calls.sort_by_key(|(site, _, _)| (site.id, std::cmp::Reverse(site.start)));
        for (site, stub_id, inputs) in calls {
            let block = &mut module.blocks[site.id];
            let continuation = Block {
                insts: block.insts.split_off(site.start + site.len),
                terminator: block.terminator.clone(),
                cold: block.cold,
                loop_header: block.loop_header,
                function_invoke: None,
            };
            let continuation_id = module.append_block(continuation);
            continuations.push(continuation_id);
            let block = &mut module.blocks[site.id];
            // prefix; body; suffix -> prefix; continuation; rotate_below_inputs; jump stub
            block.insts.truncate(site.start);
            for value in site.parameters.into_iter().rev() {
                block.insts.push(InstKind::Push(value).into());
            }
            block.insts.push(InstKind::PushLabel(continuation_id).into());
            for depth in (1..=inputs).rev() {
                block.insts.push(InstKind::Swap(depth as u16).into());
            }
            block.terminator = TerminatorKind::Jump(stub_id).into();
        }
        // original blocks; shared bodies; continuations in stable source order
        let mut layout = ids;
        layout.extend(stubs);
        layout.extend(continuations);
        module.layout = Some(layout);
        true
    }
}

#[derive(Clone)]
struct Site {
    id: BlockId,
    start: usize,
    len: usize,
    parameters: Vec<U256>,
}

fn outlinable(inst: &InstKind, allow_effects: bool) -> bool {
    match inst {
        InstKind::Push(_) | InstKind::Dup(_) | InstKind::Swap(_) | InstKind::Exchange(..) => true,
        InstKind::Op(opcode) => {
            matches!(*opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::SAR | op::CLZ | op::POP)
                || (allow_effects
                    && matches!(
                        *opcode,
                        op::MLOAD
                            | op::MSTORE
                            | op::MSTORE8
                            | op::MSIZE
                            | op::MCOPY
                            | op::SLOAD
                            | op::SSTORE
                            | op::TLOAD
                            | op::TSTORE
                            | op::CALLDATALOAD
                            | op::CALLDATASIZE
                            | op::CALLDATACOPY
                            | op::RETURNDATASIZE
                            | op::RETURNDATACOPY
                            | op::KECCAK256
                            | op::ADDRESS
                            | op::CALLER
                            | op::CALLVALUE
                            | op::LOG0..=op::LOG4
                    ))
        }
        _ => false,
    }
}

fn size(gcx: Gcx<'_>, inst: &InstKind) -> usize {
    match inst {
        InstKind::Push(value) => {
            super::immediate_materialization_cost(gcx.sess.opts.evm_version, *value).0
        }
        InstKind::Dup(depth) | InstKind::Swap(depth) => {
            if *depth <= 16 {
                1
            } else {
                2
            }
        }
        InstKind::Exchange(..) => {
            if gcx.sess.opts.evm_version.has_extended_stack_ops() {
                2
            } else {
                3
            }
        }
        _ => 1,
    }
}

/// Builds the exact shared sequence used by both profitability and emission.
fn returning_body(gcx: Gcx<'_>, body: &[InstKind], outputs: usize) -> Vec<InstKind> {
    // continuation inputs; body -> continuation outputs
    // swap1; ...; swap outputs -> outputs continuation
    let mut stub = body.iter().cloned().map(Into::into).collect::<Vec<_>>();
    stub.extend((1..=outputs).map(|depth| InstKind::Swap(depth as u16).into()));
    if gcx.sess.opts.optimization.is_size() {
        stub = super::local::simplify_schedule(gcx.sess.opts.evm_version, &stub);
    }
    stub.into_iter().map(|inst| inst.kind).collect()
}

fn contract(body: &[InstKind]) -> Option<(usize, usize)> {
    let mut height = 0isize;
    let mut needed = 0isize;
    for inst in body {
        let (inputs, outputs) = match inst {
            InstKind::Dup(depth) => (*depth as isize, *depth as isize + 1),
            InstKind::Swap(depth) | InstKind::Exchange(_, depth) => {
                (*depth as isize + 1, *depth as isize + 1)
            }
            _ => {
                let (inputs, outputs) = effect(inst)?;
                (inputs as isize, outputs as isize)
            }
        };
        needed = needed.max(inputs - height);
        height += outputs - inputs;
    }
    let outputs = needed + height;
    if needed > 16 || outputs > 16 { None } else { Some((needed as usize, outputs as usize)) }
}

type Candidate = ((usize, usize, usize, usize), Vec<InstKind>, Vec<Site>, usize);

fn retain_candidate(ranked: &mut Vec<Candidate>, candidate: Candidate, limit: usize) {
    let index = ranked.partition_point(|old| old.0 >= candidate.0);
    if index < limit {
        ranked.insert(index, candidate);
        ranked.truncate(limit);
    }
}

/// Select whole groups: dropping individual sites would invalidate profitability.
fn disjoint_candidates(ranked: Vec<Candidate>) -> Vec<Candidate> {
    let mut occupied = FxHashMap::<BlockId, Vec<(usize, usize)>>::default();
    let mut selected = Vec::new();
    for candidate in ranked {
        if candidate.2.iter().any(|site| {
            occupied.get(&site.id).is_some_and(|ranges| {
                let index = ranges.partition_point(|&(_, end)| end <= site.start);
                ranges.get(index).is_some_and(|&(start, _)| start < site.start + site.len)
            })
        }) {
            continue;
        }
        for site in &candidate.2 {
            occupied.entry(site.id).or_default().push((site.start, site.start + site.len));
        }
        for ranges in occupied.values_mut() {
            ranges.sort_unstable();
        }
        selected.push(candidate);
        if selected.len() == 16 {
            break;
        }
    }
    selected
}

fn parameterized(
    gcx: Gcx<'_>,
    module: &Module,
    prefixes: &IndexVec<BlockId, Vec<isize>>,
    heights: &super::verify::StackHeights,
    reachable: &DenseBitSet<BlockId>,
) -> Option<Candidate> {
    let mut groups = FxHashMap::<Vec<InstKind>, Vec<Site>>::default();
    for id in module.block_ids() {
        let mut body = Vec::new();
        let mut parameters = Vec::new();
        for inst in &module.blocks[id].insts {
            if inst.stack_effect.is_some()
                || !outlinable(&inst.kind, gcx.sess.opts.optimization.is_size())
                || body.len() == 64
            {
                break;
            }
            if let InstKind::Push(value) = inst.kind {
                parameters.push(value);
                body.push(InstKind::Push(U256::ZERO));
            } else {
                body.push(inst.kind.clone());
            }
        }
        if (1..=4).contains(&parameters.len())
            && contract(&body).is_some_and(|(inputs, _)| inputs == 0)
        {
            groups.entry(body.clone()).or_default().push(Site {
                id,
                start: 0,
                len: body.len(),
                parameters,
            });
        }
    }
    let mut best = None;
    for (body, mut sites) in groups {
        if sites.len() < 2 || sites.iter().all(|site| site.parameters == sites[0].parameters) {
            continue;
        }
        let parameters = sites[0].parameters.len();
        let (_, outputs) = contract(&body)?;
        let mut stack = (0..parameters).rev().map(Some).collect::<Vec<_>>();
        let mut next_parameter = 0;
        let mut skeleton = Vec::new();
        for inst in body {
            if matches!(inst, InstKind::Push(_)) {
                let position = stack.iter().position(|&value| value == Some(next_parameter))?;
                let depth = stack.len() - 1 - position;
                // remaining_parameters values parameter -> remaining_parameters values parameter
                // rotate the requested parameter to the top, preserving other values
                for step in 1..=depth {
                    skeleton.push(InstKind::Swap(step as u16));
                    let top = stack.len() - 1;
                    stack.swap(top, top - step);
                }
                *stack.last_mut()? = None;
                next_parameter += 1;
            } else {
                match inst {
                    InstKind::Dup(depth) => stack.push(stack[stack.len() - depth as usize]),
                    InstKind::Swap(depth) => {
                        let top = stack.len() - 1;
                        stack.swap(top, top - depth as usize);
                    }
                    InstKind::Exchange(a, b) => {
                        let top = stack.len() - 1;
                        stack.swap(top - a as usize, top - b as usize);
                    }
                    _ => {
                        let (inputs, outputs) = effect(&inst)?;
                        stack.truncate(stack.len() - inputs as usize);
                        stack.extend(std::iter::repeat_n(None, outputs as usize));
                    }
                }
                skeleton.push(inst);
            }
        }
        sites.retain(|site| peak_fits(prefixes, heights, reachable, site, &skeleton));
        if sites.len() < 2 {
            continue;
        }
        let before = sites
            .iter()
            .map(|site| {
                module.blocks[site.id].insts[..site.len]
                    .iter()
                    .map(|inst| size(gcx, &inst.kind))
                    .sum::<usize>()
            })
            .sum::<usize>();
        let parameter_cost = sites
            .iter()
            .flat_map(|site| site.parameters.iter())
            .map(|value| size(gcx, &InstKind::Push(*value)))
            .sum::<usize>();
        let protocol = parameter_cost + sites.len() * (6 + parameters) + 2;
        if before < protocol + 8 {
            continue;
        }
        let stub = returning_body(gcx, &skeleton, outputs);
        let after = protocol + stub.iter().map(|inst| size(gcx, inst)).sum::<usize>();
        if before < after + 8 {
            continue;
        }
        let score = (before - after, skeleton.len(), usize::MAX - sites[0].id.index(), usize::MAX);
        if best.as_ref().is_none_or(|old: &Candidate| score > old.0) {
            best = Some((score, stub, sites, parameters));
        }
    }
    best
}

fn peak_fits(
    prefixes: &IndexVec<BlockId, Vec<isize>>,
    heights: &super::verify::StackHeights,
    reachable: &DenseBitSet<BlockId>,
    site: &Site,
    body: &[InstKind],
) -> bool {
    let entry = match heights[site.id] {
        Some((_, entry)) => entry,
        None if !reachable.contains(site.id) => 0,
        None => return false,
    };
    // Even unreachable bodies retain an intrinsic peak at or below the limit.
    let Some(&prefix) = prefixes[site.id].get(site.start) else { return false };
    let mut height = entry as isize + prefix + site.parameters.len() as isize + 1;
    // Continuation push followed by the direct jump's transient target push.
    if height + 1 > 1024 {
        return false;
    }
    for inst in body {
        let Some((inputs, outputs)) = effect(inst) else { return false };
        height += outputs as isize - inputs as isize;
        if height > 1024 {
            return false;
        }
    }
    true
}
