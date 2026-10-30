//! Outline repeated closed computations and large immediate pushes.
//!
//! The pass finds repeated straight-line machine instruction runs, replaces each profitable site
//! with a jump to one shared body, and returns from that body through a stack-held continuation.
//! It also finds structurally equal runs whose concrete pushes differ, turning those pushes into
//! stack parameters in size mode, with up to sixteen result words. A final specialized path
//! shares repeated large pushes when the call and return sequence is smaller than spelling out
//! each literal.
//!
//! Gas-mode candidates are closed stack computations: they leave the incoming stack untouched
//! and produce up to sixteen outputs. The hidden return address remains below those outputs;
//! `SWAP1` through `SWAPn` rotate it to the top for the return jump. Size mode also permits bounded
//! input arguments. Candidates have known stack effects and contain no control flow or position
//! or gas observations. Profitability includes the shared body,
//! per-site call sequence, continuation labels, and target-dependent push widths. Constant
//! recipes charge call/return gas against deposited bytes using the requested optimizer run count
//! in gas mode. Sites are selected without overlap, and new blocks and labels are installed
//! through the normal EVM IR CFG representation.
//!
//! Gas mode keeps computations and immediate pushes in known loop blocks inline, since each
//! outlined site adds call and return jumps on every iteration and static duplicate counts cannot
//! justify that. Cold blocks, which do not run on a successful call, are the exception: their
//! sites pay no gas, as [`Target::site_gas`] prices them like a size build, so a loop of cold
//! blocks is no loop, and a share whose sites are all cold needs only to save bytes. In a share
//! that also has hot sites, only those pay the transfer gas.
//!
//! Size mode shares a push inside a loop only when the bytes it saves there repay those jumps over
//! the expected executions, at the target's estimate of a loop's iterations per execution, so a
//! constant that enough other code repeats does not slow down every iteration of an unrelated
//! loop.
//!
//! Replacing a site splits its block around the run, so both ends of a candidate run must be
//! boundaries `keep_with_next` allows to become block boundaries.
//!
//! Enumerating all substrings is potentially quadratic, so the implementation cuts runs at unique
//! instructions, hashes slices from prefix tables, and applies a module-wide candidate budget.
//! Large modules shorten the maximum considered run rather than allowing unbounded compile time.
//! In gas mode, a module-wide screen first asks whether the widest push, or a closed whitelisted
//! prefix that occurs at two starts, could repay its transfer gas over the expected executions.
//! A module with a cold block is screened at no transfer gas, since a share may be all cold.
//! When neither could, the pass builds no run tables. Machine runs are skipped only when no push
//! can be shared either, since a rejected store run keeps its literal from becoming a push share.
//!
//! Outlining runs before late CFG/CSE/DCE cleanup, which removes jump thunks and redundancies
//! exposed by sharing; assembly remains responsible only for final label offsets and push widths.
//! In size mode, a marginal run can also repay its stub by sharing an identical return suffix
//! with a stub already selected in this pass. Its estimate includes the extra jump and landing
//! that the following tail-merge pass introduces; gas mode never relies on this credit. The
//! search checks at most the 64 most recently selected stubs to bound its compile-time cost.

use super::{
    EvmPass,
    utils::{FreshLabels, MachineInstKey, instruction_size_lower_bound, is_split_point},
};
use crate::{
    backend::evm::{
        DebugFunction,
        ir::{
            Block, BlockId, CallEffect, Instruction, Module, ModuleTarget, StackEffect, Terminator,
            TerminatorKind,
        },
        op,
    },
    target::Target,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet, FxHasher},
};
use solar_sema::Gcx;
use std::hash::{Hash, Hasher};

pub(super) struct Outline;

impl EvmPass for Outline {
    fn name(&self) -> &'static str {
        "outline"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        outline(gcx, module)
    }
}

const MIN_MACHINE_RUN: usize = 3;
const MAX_MACHINE_RUN_CANDIDATES: usize = 2_000_000;

fn outline(gcx: Gcx<'_>, module: &mut Module) -> bool {
    tracing::trace!(
        target: "solar::codegen::evm_ir::outline",
        module = %module.name,
        blocks = module.blocks.len(),
        "analyzing machine instruction outlines"
    );
    let mut state = RunState::default();
    // A rejected machine run can veto a push share, so skip runs only when no push can share.
    let pushes = may_share_pushes(gcx, module);
    let runs = pushes || may_share_machine_runs(gcx, module);
    (runs && outline_machine_runs(gcx, module, &mut state))
        | (module.optimization.is_size()
            && outline_parametric_machine_runs(gcx, module, &mut state))
        | (pushes && outline_repeated_pushes(gcx, module, &mut state))
}

/// Returns whether any repeated push could repay its call and return gas in gas mode.
///
/// A share of `n` pushes, each at most a `PUSH32` and its immediate, saves fewer than
/// `n * (PUSH32 bytes - site bytes)` bytes, while its transfers cost `n` times the transfer gas
/// at hot sites and nothing at cold ones.
fn may_share_pushes(gcx: Gcx<'_>, module: &Module) -> bool {
    if !module.optimization.is_gas() {
        return true;
    }
    let target = module.target(gcx);
    let (site_bytes, _, transfer_gas) = push_share_costs(&target);
    target.deposit_repays_gas(
        u64::from(target.opcode(op::PUSH32).bytes).saturating_sub(site_bytes as u64),
        u64::from(screen_transfer_gas(&target, module, transfer_gas)),
    )
}

/// The transfer gas per site that a screen must assume: a share with a cold site pays less than
/// `gas` for it, so a module with a cold block is screened as if every site were cold.
fn screen_transfer_gas(target: &Target, module: &Module, gas: u32) -> u32 {
    target.site_gas(module.blocks.iter().any(|block| block.metadata.hotness.is_cold()), gas)
}

/// Returns whether gas mode could share some closed machine run outside loops.
///
/// A share of `n` runs of `size` bytes saves fewer than `n * (size - transfer bytes)` bytes,
/// while its transfers cost at least `n` times the gas of two pushes, two jumps, and two
/// labels, at hot sites only. Every candidate is a closed run of whitelisted instructions, and
/// every site of a profitable share starts with the same shortest closed prefix that is large
/// enough. Sharing is possible only if two starts have equal prefixes, which hashes can rule out.
/// The screen shares the outliner's candidate budget and gives up, answering yes, once it is spent.
fn may_share_machine_runs(gcx: Gcx<'_>, module: &Module) -> bool {
    if !module.optimization.is_gas() {
        return true;
    }
    let target = module.target(gcx);
    let transfer_size = transfer_size(&target);
    let transfer_gas = screen_transfer_gas(&target, module, transfer_gas(&target));
    let profitable = |size: usize| {
        target
            .deposit_repays_gas(size.saturating_sub(transfer_size) as u64, u64::from(transfer_gas))
    };
    let mut metrics = Vec::new();
    let mut prefixes = FxHashSet::default();
    let mut budget = MAX_MACHINE_RUN_CANDIDATES;
    for block in &module.blocks {
        if in_hot_loop(block) {
            continue;
        }
        metrics.clear();
        metrics.extend(block.instructions.iter().map(|inst| {
            shared_body_effect(inst)
                .map(|effect| (effect, instruction_size_lower_bound(target, inst)))
        }));
        let mut remaining = metrics.iter().flatten().map(|&(_, size)| size).sum::<usize>();
        for start in 0..metrics.len() {
            // Neither this start nor any later one can reach a profitable size.
            if !profitable(remaining) {
                break;
            }
            remaining -= metrics[start].map_or(0, |(_, size)| size);
            if !is_split_point(&block.instructions, start) {
                continue;
            }
            let mut stack = StackDelta::default();
            let mut run_size = 0usize;
            for (end, &metric) in metrics.iter().enumerate().skip(start) {
                let Some((effect, size)) = metric else { break };
                stack.apply(effect);
                if stack.inputs > 0 {
                    break;
                }
                let Some(left) = budget.checked_sub(1) else { return true };
                budget = left;
                run_size += size;
                if profitable(run_size) {
                    let mut hasher = FxHasher::default();
                    for inst in &block.instructions[start..=end] {
                        MachineInstKey::new(inst).hash(&mut hasher);
                    }
                    if !prefixes.insert(hasher.finish()) {
                        return true;
                    }
                    break;
                }
            }
        }
    }
    false
}

/// Whether `block` is in a loop that runs on a successful call.
fn in_hot_loop(block: &Block) -> bool {
    block.metadata.in_loop && !block.metadata.hotness.is_cold()
}

/// Whether `site` is in a block that does not run on a successful call.
fn is_cold(module: &Module, site: &Site) -> bool {
    module.blocks[site.block].metadata.hotness.is_cold()
}

/// The transfer gas `sites` pay together, which a cold site does not.
fn sites_transfer_gas(module: &Module, target: &Target, sites: &[Site], gas: u32) -> u64 {
    sites.iter().map(|site| u64::from(target.site_gas(is_cold(module, site), gas))).sum()
}

/// The blocks of the sites that run on a successful call.
fn hot_blocks<'a>(module: &'a Module, sites: &'a [Site]) -> impl Iterator<Item = BlockId> + 'a {
    sites.iter().filter(|site| !is_cold(module, site)).map(|site| site.block)
}

/// Whether any of `blocks` may run more than once.
fn repeats(module: &Module, blocks: impl IntoIterator<Item = BlockId>) -> bool {
    blocks.into_iter().any(|block| !module.blocks[block].runs_once())
}

/// Returns the bytes of one call site:
///
/// push2 return
/// push2 body
/// jump
/// jumpdest
fn transfer_size(target: &Target) -> usize {
    (target.opcode(op::PUSH2).bytes * 2
        + target.opcode(op::JUMP).bytes
        + target.opcode(op::JUMPDEST).bytes) as usize
}

/// Returns the gas of one call and return, excluding the shared body's rotations.
fn transfer_gas(target: &Target) -> u32 {
    target.runtime_gas(
        (target.opcode(op::PUSH2) + target.opcode(op::JUMP) + target.opcode(op::JUMPDEST)).times(2),
    )
}

/// Returns the per-site bytes, shared body bytes, and transfer gas of a push share.
fn push_share_costs(target: &Target) -> (usize, usize, u32) {
    // The shared body around the push:
    // jumpdest
    // swap1
    // jump
    let body_bytes = (target.opcode(op::JUMPDEST).bytes
        + target.opcode(op::SWAP1).bytes
        + target.opcode(op::JUMP).bytes) as usize;
    (
        transfer_size(target),
        body_bytes,
        transfer_gas(target) + target.runtime_gas(target.opcode(op::SWAP1)),
    )
}

fn outline_machine_runs(gcx: Gcx<'_>, module: &mut Module, state: &mut RunState) -> bool {
    // Enumerating every contiguous run is quadratic in a block's length, and
    // hashing each run's instructions made it cubic. Two exact filters and a
    // candidate budget keep it in hand:
    //
    // - A run can only be outlined if it occurs at least twice, so every instruction in it occurs
    //   at least twice module-wide. Runs are cut at any instruction that does not, which ends them
    //   at the unique pushes that separate most straight-line code.
    // - A run's hash comes from a per-block prefix table, so it costs the same whatever the run's
    //   length. Equality compares interned instruction IDs, preserving exact grouping.
    // - Large modules use shorter runs so candidate storage stays bounded. Longer repeated
    //   sequences can still be outlined in chunks.
    let hashes = RunHashes::new(module, MachineInstKey::new);
    let repeated_instructions = hashes.repeated_count();
    if repeated_instructions == 0 || repeated_instructions > MAX_MACHINE_RUN_CANDIDATES {
        return false;
    }
    let max_run_length = (MIN_MACHINE_RUN - 1 + MAX_MACHINE_RUN_CANDIDATES / repeated_instructions)
        .max(MIN_MACHINE_RUN);
    let target = module.target(gcx);
    let transfer_size = transfer_size(&target);
    let shuffle_size = target.opcode(op::SWAP1).bytes as usize;

    let is_gas = target.optimization().is_gas();
    let is_size = target.optimization().is_size();
    let metrics = hashes.metrics(module, |block, inst| {
        if is_gas && in_hot_loop(block) {
            return None;
        }
        shared_body_effect(inst).map(|effect| (effect, instruction_size_lower_bound(target, inst)))
    });
    let groups = repeated_runs(
        module,
        &hashes,
        &metrics,
        max_run_length,
        MIN_MACHINE_RUN,
        |(stack, run_size): &mut (StackDelta, usize), (effect, size)| {
            stack.apply(effect);
            *run_size += size;
            // Inputs never shrink, so no longer run can have fewer. Open runs need size mode.
            stack.inputs <= if is_size { 16 } else { 0 }
        },
        |&(stack, run_size), block, start, len| {
            let (inputs, outputs) = (stack.inputs, stack.outputs());
            // Price both address pushes at PUSH2, plus the jump, continuation label, and
            // input shuffles. Runs no larger than this site cannot pass the shared stub's
            // profitability check regardless of occurrence count, so do not intern them.
            let can_amortize = run_size > transfer_size + inputs as usize * shuffle_size;
            (can_amortize
                && outputs <= 16
                && is_split_point(&module.blocks[block].instructions, start + len))
            .then_some(Site { block, start, len, inputs: inputs as u16, outputs: outputs as u16 })
        },
    );
    let chosen = choose_groups(&hashes, groups, |free, chosen| {
        let first = free[0];
        // Profitability reads only opcodes and values, so copy the body once it is chosen.
        let run = &module.blocks[first.block].instructions[first.start..first.start + first.len];
        let run_size = lower_bound(target, run);
        let stub_size = run_size
            + (target.opcode(op::JUMPDEST).bytes
                + target.opcode(op::SWAP1).bytes * u32::from(first.outputs)
                + target.opcode(op::JUMP).bytes) as usize;
        let outlined_size =
            free.len() * (transfer_size + usize::from(first.inputs) * shuffle_size) + stub_size;
        let saved = if is_size && free.len() * run_size <= outlined_size {
            shared_return_tail_savings(&target, run, first, chosen)
        } else {
            0
        };
        if free.len() * run_size <= outlined_size.saturating_sub(saved) {
            return None;
        }
        // repeated run at each site
        // =>
        // push continuation; jump shared; ...; swap outputs; jump continuation
        //
        // Charge the transfer gas at the requested execution count against
        // the deposited bytes saved by sharing. Static occurrence counts do
        // not make the calls cold, even when every site is outside a loop.
        if is_gas {
            let saved_bytes = run_size * free.len() - outlined_size;
            // Hot sites that may run more than once pay the transfer gas at the runtime price.
            let target = if repeats(module, hot_blocks(module, &free)) {
                module.runtime_target(gcx)
            } else {
                target
            };
            let transfer_gas = transfer_gas(&target)
                + target.runtime_gas(target.opcode(op::SWAP1))
                    * u32::from(first.outputs.saturating_add(first.inputs));
            if !target.deposit_repays_gas(
                saved_bytes as u64,
                sites_transfer_gas(module, &target, &free, transfer_gas),
            ) {
                if let [value, address, store, ..] = run
                    && address.is_encoded_push()
                    && matches!(store.opcode, op::MSTORE | op::MSTORE8)
                    && let Some(value) = value.concrete_immediate()
                {
                    state.inline_store_literals.insert(value);
                }
                return None;
            }
        }
        let mut body = run.to_vec();
        merge_site_source_spans(module, &mut body, &free);
        clear_function_invokes(&mut body);
        Some(Group { body, sites: free, parameters: Vec::new() })
    });
    install_groups(module, state, chosen)
}

/// Outlines closed computations that differ only in a bounded number of immediate pushes.
/// Each differing immediate becomes a stack argument to one shared stub.
fn outline_parametric_machine_runs(
    gcx: Gcx<'_>,
    module: &mut Module,
    state: &mut RunState,
) -> bool {
    const MIN_RUN_LENGTH: usize = 12;
    const MAX_RUN_LENGTH: usize = 64;
    const MAX_PARAMETERS: usize = 8;

    let target = module.runtime_target(gcx);
    let hashes = RunHashes::new(module, |inst| {
        inst.concrete_immediate().is_none().then(|| MachineInstKey::new(inst))
    });
    let metrics = hashes.metrics(module, |_, inst| {
        shared_body_effect(inst).map(|effect| (effect, inst.concrete_immediate().is_some()))
    });
    let groups = repeated_runs(
        module,
        &hashes,
        &metrics,
        MAX_RUN_LENGTH,
        MIN_RUN_LENGTH,
        |(stack, immediate_pushes): &mut (StackDelta, usize), (effect, parameter)| {
            stack.apply(effect);
            *immediate_pushes += usize::from(parameter);
            // Neither count shrinks, so no longer run can pass these limits.
            stack.inputs == 0 && *immediate_pushes <= MAX_PARAMETERS * 2
        },
        |&(stack, immediate_pushes), block, start, len| {
            let outputs = stack.outputs();
            (outputs <= 16
                && immediate_pushes != 0
                && is_split_point(&module.blocks[block].instructions, start + len))
            .then_some(Site { block, start, len, inputs: 0, outputs: outputs as u16 })
        },
    );
    let chosen =
        choose_groups(&hashes, groups, |free, _| {
            let first = free[0];
            let run =
                |site: &Site| &module.blocks[site.block].instructions[site.start..][..site.len];
            // Sites share a run key, so an instruction that differs between them is an immediate
            // push at every site.
            let parameters = (0..first.len)
                .filter(|&index| {
                    free[1..].iter().any(|site| {
                        MachineInstKey::new(&run(site)[index])
                            != MachineInstKey::new(&run(&first)[index])
                    })
                })
                .collect::<Vec<_>>();
            if parameters.is_empty() || parameters.len() > MAX_PARAMETERS {
                return None;
            }
            let mut body = run(&first).to_vec();
            merge_site_source_spans(module, &mut body, &free);
            clear_function_invokes(&mut body);
            let body = parameterize_body(&body, &parameters)?;
            let inline_size = free.iter().map(|site| lower_bound(target, run(site))).sum::<usize>();
            let parameter_bytes = free
                .iter()
                .flat_map(|site| {
                    parameters
                        .iter()
                        .map(move |&index| instruction_size_lower_bound(target, &run(site)[index]))
                })
                .sum::<usize>();
            // The byte per parameter pays for no emitted instruction. It is a margin: groups are
            // taken longest first, and without it a barely profitable long group can claim sites
            // that a shorter group shares better.
            let site_size = (if free.len() >= 4 { 7 } else { 8 }) + parameters.len();
            let stub_size = 1 + lower_bound(target, &body) + usize::from(first.outputs) + 1;
            (inline_size >= parameter_bytes + free.len() * site_size + stub_size + 2)
                .then(|| Group { body, sites: free, parameters })
        });
    install_groups(module, state, chosen)
}

fn parameterize_body(body: &[Instruction], parameters: &[usize]) -> Option<Vec<Instruction>> {
    let mut result = Vec::with_capacity(body.len());
    let mut delta = 0i32;
    for (index, inst) in body.iter().enumerate() {
        if parameters.contains(&index) {
            let depth = usize::try_from(delta).ok()?;
            if depth > 16 {
                return None;
            }
            result.extend((1..=depth).map(swap));
            delta += 1;
            continue;
        }
        let (_, pops, pushes) = shared_body_effect(inst)?;
        delta = delta - i32::from(pops) + i32::from(pushes);
        if delta < 0 {
            return None;
        }
        result.push(inst.clone());
    }
    Some(result)
}

fn outline_repeated_pushes(gcx: Gcx<'_>, module: &mut Module, state: &mut RunState) -> bool {
    let target = module.target(gcx);
    let (site_bytes, body_bytes, transfer_gas) = push_share_costs(&target);
    let mut sites = FxHashMap::<U256, SmallVec<[Site; 2]>>::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let is_gas = target.optimization().is_gas();
        // Size mode prices every loop block, cold or not, by the iterations it may run.
        let in_loop = if is_gas { in_hot_loop(block) } else { block.metadata.in_loop };
        if is_gas && in_loop {
            continue;
        }
        for (index, inst) in block.instructions.iter().enumerate() {
            if let Some(value) = inst.concrete_immediate()
                && is_split_point(&block.instructions, index)
                && is_split_point(&block.instructions, index + 1)
                // A loop site pays the call and return on every iteration.
                && (!in_loop
                    || target.shares_in_loop(
                        (target.push(value).bytes as usize).saturating_sub(site_bytes) as u64,
                        u64::from(transfer_gas),
                    ))
            {
                sites.entry(value).or_default().push(Site {
                    block: block_id,
                    start: index,
                    len: 1,
                    inputs: 0,
                    outputs: 1,
                });
            }
        }
    }

    const MIN_SAVING: usize = 8;
    sites.retain(|value, occurrences| {
        // Do not recreate the rejected store outline as a literal-returning call.
        if occurrences.len() < 2 || state.inline_store_literals.contains(value) {
            return false;
        }
        let push_size = target.push(*value).bytes as usize;
        // Hot sites that may run more than once pay the transfer gas at the runtime price.
        let (target, transfer_gas) = if repeats(module, hot_blocks(module, occurrences)) {
            let runtime = module.runtime_target(gcx);
            (runtime, push_share_costs(&runtime).2)
        } else {
            (target, transfer_gas)
        };
        let inline = occurrences.len() * push_size;
        let outlined = occurrences.len() * site_bytes + push_size + body_bytes;
        let saved_bytes = inline.saturating_sub(outlined);
        inline >= outlined + MIN_SAVING
            && (!target.optimization().is_gas()
                || target.deposit_repays_gas(
                    saved_bytes as u64,
                    sites_transfer_gas(module, &target, occurrences, transfer_gas),
                ))
    });
    let mut sites = sites.into_iter().collect::<Vec<_>>();
    sites.sort_unstable_by_key(|&(value, _)| value);
    let groups = sites
        .into_iter()
        .map(|(value, sites)| {
            // The stub appends its output swap to the body.
            let mut body = Vec::with_capacity(2);
            body.push(Instruction::push_value(value).with_debug_info_dropped());
            merge_site_source_spans(module, &mut body, &sites);
            Group { body, sites, parameters: Vec::new() }
        })
        .collect();
    install_groups(module, state, groups)
}

/// Returns the groups to outline, longest runs first. Each takes the sites of a run that no
/// earlier group claimed and that are pairwise disjoint, and `build` prices the group.
///
/// Periodic runs produce occurrences that overlap each other: window `[0, L)` and window
/// `[p, p + L)` are byte-identical when the period `p < L` divides the pattern. Two overlapping
/// occurrences cannot both be replaced, and the descending-order edit application would split the
/// block at a stale index, so every chosen site is pairwise disjoint.
fn choose_groups(
    hashes: &RunHashes,
    mut groups: Vec<(RunSlice<'_>, SmallVec<[Site; 2]>)>,
    mut build: impl FnMut(SmallVec<[Site; 2]>, &[Group]) -> Option<Group>,
) -> Vec<Group> {
    groups.sort_unstable_by_key(|(key, sites)| {
        (std::cmp::Reverse(key.ids.len()), sites[0].block.index(), sites[0].start)
    });
    // The instructions of chosen sites, by flat index.
    let mut claimed = DenseBitSet::new_empty(hashes.ids.len());
    let flat = |site: &Site| {
        let first = hashes.starts[site.block] + site.start;
        first..first + site.len
    };
    let mut chosen = Vec::new();
    for (_, sites) in groups {
        let mut free = SmallVec::<[Site; 2]>::new();
        for site in sites {
            let end = site.start + site.len;
            if claimed.contains_any(flat(&site))
                || free.iter().any(|other| {
                    other.block == site.block
                        && other.start < end
                        && site.start < other.start + other.len
                })
            {
                continue;
            }
            free.push(site);
        }
        if free.len() < 2 {
            continue;
        }
        if let Some(group) = build(free, &chosen) {
            for site in &group.sites {
                claimed.insert_range(flat(site));
            }
            chosen.push(group);
        }
    }
    chosen
}

/// Adds one stub block per group and replaces each site with a call to it.
fn install_groups(module: &mut Module, state: &mut RunState, groups: Vec<Group>) -> bool {
    if groups.is_empty() {
        return false;
    }
    let outlined_sites = groups.iter().map(|group| group.sites.len()).sum::<usize>();
    let labels = state.labels.get_or_insert_with(|| FreshLabels::new(module));
    let Some(mut labels) = labels.take(groups.len() + outlined_sites).map(Vec::into_iter) else {
        return false;
    };
    tracing::debug!(
        target: "solar::codegen::evm_ir::outline",
        module = %module.name,
        outlined_groups = groups.len(),
        outlined_sites,
        "outlined repeated instruction runs"
    );

    let mut edits = Vec::with_capacity(outlined_sites);
    for group in groups {
        let Site { inputs, outputs, .. } = group.sites[0];
        let mut stub = Block::new(labels.next().expect("reserved one label per outline stub"));
        stub.metadata.repeats = repeats(module, group.sites.iter().map(|site| site.block));
        stub.instructions = group.body;
        stub.instructions.extend((1..=usize::from(outputs)).map(swap));
        stub.terminator =
            Some(Terminator::new(TerminatorKind::Op(op::JUMP)).with_debug_info_dropped());
        stub.metadata.call_effect =
            stub_call_effect(usize::from(inputs) + group.parameters.len(), outputs);
        let stub = module.add_block(stub);
        for site in group.sites {
            let source = &module.blocks[site.block].instructions[site.start..];
            let mut parameters = group
                .parameters
                .iter()
                .rev()
                .map(|&index| source[index].clone())
                .collect::<Vec<_>>();
            clear_function_invokes(&mut parameters);
            edits.push(Edit { site, stub, parameters });
        }
    }
    apply_outline_edits(module, edits, &mut labels);
    debug_assert!(labels.next().is_none());
    true
}

fn merge_site_source_spans(module: &Module, body: &mut [Instruction], sites: &[Site]) {
    for site in sites {
        let instructions =
            &module.blocks[site.block].instructions[site.start..site.start + site.len];
        for (body_instruction, site_instruction) in body.iter_mut().zip(instructions) {
            body_instruction.metadata.merge_source_spans(&site_instruction.metadata);
        }
    }
}

fn clear_function_invokes(instructions: &mut [Instruction]) {
    for instruction in instructions {
        instruction.metadata.take_function_invoke();
    }
}

fn range_function_invoke(instructions: &[Instruction]) -> Option<DebugFunction> {
    let mut functions =
        instructions.iter().filter_map(|instruction| instruction.metadata.function_invoke());
    let function = functions.next()?;
    functions.all(|other| other == function).then_some(function)
}

fn swap(depth: usize) -> Instruction {
    Instruction::stack_op(op::StackOp::Swap(depth as u8)).with_debug_info_dropped()
}

/// Splits each site's block, applying a block's edits from its last site to its first so earlier
/// indices stay valid.
fn apply_outline_edits(
    module: &mut Module,
    mut edits: Vec<Edit>,
    labels: &mut impl Iterator<Item = u32>,
) {
    edits.sort_unstable_by_key(|edit| (edit.site.block, std::cmp::Reverse(edit.site.start)));
    assert!(
        edits.windows(2).all(|pair| pair[0].site.block != pair[1].site.block
            || pair[1].site.start + pair[1].site.len <= pair[0].site.start),
        "outline edits overlap"
    );
    for edit in &edits {
        let label = labels.next().expect("reserved one label per outline site");
        split_outline_site(module, edit, label);
    }
}

/// The call effect of a stub whose body replaces `inputs` words with `outputs` words above the
/// return address that its site leaves below them.
fn stub_call_effect(inputs: usize, outputs: u16) -> Option<CallEffect> {
    Some(CallEffect {
        ret: u8::try_from(inputs).ok()?,
        effect: StackEffect::new(u8::try_from(inputs + 1).ok()?, u8::try_from(outputs).ok()?),
    })
}

fn split_outline_site(module: &mut Module, edit: &Edit, continuation_label: u32) {
    let Site { block, start, len, inputs, .. } = edit.site;
    let function_invoke =
        range_function_invoke(&module.blocks[block].instructions[start..start + len]);
    // prefix; push continuation; rotate return below inputs; push parameters; jump stub
    // continuation: suffix; original terminator
    let mut continuation = Block::new(continuation_label);
    continuation.metadata = module.blocks[block].metadata;
    continuation.metadata.is_continuation = true;
    // The call convention belongs to the function's entry block, not to the code after the stub.
    continuation.metadata.call_effect = None;
    continuation.instructions = module.blocks[block].instructions.split_off(start + len);
    module.blocks[block].instructions.truncate(start);
    continuation.terminator = module.blocks[block].terminator.take();
    let continuation = module.add_block(continuation);
    let instructions = &mut module.blocks[block].instructions;
    instructions.push(Instruction::push_block(continuation).with_debug_info_dropped());
    instructions.extend((1..=usize::from(inputs)).rev().map(swap));
    instructions.extend_from_slice(&edit.parameters);
    let mut terminator = Terminator::new(TerminatorKind::Jump(edit.stub)).with_debug_info_dropped();
    if let Some(function) = function_invoke {
        terminator.metadata.set_function_invoke(function);
    }
    module.blocks[block].terminator = Some(terminator);
}

/// The words an instruction reads, pops, and pushes, when it may run in a shared body: a push, a
/// stack operation, or a position-independent operation.
fn shared_body_effect(inst: &Instruction) -> Option<(u16, u16, u16)> {
    if inst.is_encoded_push() {
        return Some((0, 0, 1));
    }
    if let Some(stack_op) = inst.as_stack_op() {
        let required = u16::try_from(stack_op.required_depth()).ok()?;
        let growth = stack_op.net_growth();
        let inputs = u16::try_from(growth.saturating_neg().max(0)).ok()?;
        let outputs = u16::try_from(growth.max(0)).ok()?;
        return Some((required, inputs, outputs));
    }
    let definition = inst.definition().filter(|definition| definition.is_position_independent())?;
    let (inputs, outputs) = definition.stack_io?;
    Some((u16::from(inputs), u16::from(inputs), u16::from(outputs)))
}

fn lower_bound(target: Target, instructions: &[Instruction]) -> usize {
    instructions.iter().map(|inst| instruction_size_lower_bound(target, inst)).sum()
}

#[derive(Clone, Copy, Debug)]
struct Site {
    block: BlockId,
    start: usize,
    len: usize,
    inputs: u16,
    outputs: u16,
}

/// A run to outline: the stub body and the sites it replaces, which share one stack convention. A
/// site pushes the continuation, rotates it below its `inputs` words, then pushes the
/// `parameters`, which are body indices whose immediates each site passes as stack arguments
/// instead.
struct Group {
    body: Vec<Instruction>,
    sites: SmallVec<[Site; 2]>,
    parameters: Vec<usize>,
}

struct Edit {
    site: Site,
    stub: BlockId,
    parameters: Vec<Instruction>,
}

/// Instruction tables over all blocks: whether each instruction occurs more than
/// once module-wide, and prefix hashes so any run hashes in constant time.
///
/// `prefix[i + 1] = prefix[i] * BASE + hash(inst[i])`, so the run `[start, end]`
/// hashes to `prefix[end + 1] - prefix[start] * BASE^len`. Equal instruction
/// sequences always produce equal hashes. Equality compares interned instruction IDs,
/// retaining exact machine identity without decoding each instruction again.
///
/// The blocks share flat tables. Block `b`'s instructions start at `starts[b]`, and its
/// prefixes, which hold one more entry, at `starts[b] + b`.
struct RunHashes {
    ids: Vec<usize>,
    /// The module-wide occurrence count of each interned instruction ID.
    counts: Vec<u32>,
    starts: IndexVec<BlockId, usize>,
    prefixes: Vec<u64>,
    powers: Vec<u64>,
}

impl RunHashes {
    const BASE: u64 = 0x100_0000_01b3;

    fn new<K: Copy + Eq + Hash>(module: &Module, key: impl Fn(&Instruction) -> K) -> Self {
        // Number each distinct key once, with its occurrence count and hash.
        let total = module.blocks.iter().map(|block| block.instructions.len()).sum();
        let mut interned = FxHashMap::<K, u32>::default();
        let mut counts = Vec::<u32>::new();
        let mut hashes = Vec::new();
        let mut ids = Vec::with_capacity(total);
        let mut prefixes = Vec::with_capacity(total + module.blocks.len());
        let mut starts = IndexVec::with_capacity(module.blocks.len());
        let mut longest = 0;
        for block in &module.blocks {
            starts.push(ids.len());
            longest = longest.max(block.instructions.len());
            let mut last = 0u64;
            prefixes.push(last);
            for inst in &block.instructions {
                let key = key(inst);
                let id = *interned.entry(key).or_insert_with(|| {
                    let mut hasher = FxHasher::default();
                    key.hash(&mut hasher);
                    hashes.push(hasher.finish());
                    counts.push(0);
                    (counts.len() - 1) as u32
                }) as usize;
                counts[id] += 1;
                ids.push(id);
                last = last.wrapping_mul(Self::BASE).wrapping_add(hashes[id]);
                prefixes.push(last);
            }
        }

        let mut powers = Vec::with_capacity(longest + 1);
        powers.push(1u64);
        for index in 0..longest {
            powers.push(powers[index].wrapping_mul(Self::BASE));
        }

        Self { ids, counts, starts, prefixes, powers }
    }

    /// Whether this instruction occurs more than once in the module. A run that
    /// contains an instruction occurring exactly once can never occur twice, so
    /// it can never be outlined.
    fn repeats(&self, block: BlockId, index: usize) -> bool {
        self.counts[self.ids[self.starts[block] + index]] >= 2
    }

    fn repeated_count(&self) -> usize {
        self.ids.iter().filter(|&&id| self.counts[id] >= 2).count()
    }

    /// Returns `metric` of each repeated instruction by flat index, `None` for any other.
    /// Overlapping candidate windows revisit each instruction, so classify it once.
    fn metrics<M: Copy>(
        &self,
        module: &Module,
        metric: impl Fn(&Block, &Instruction) -> Option<M>,
    ) -> Vec<Option<M>> {
        let mut metrics = vec![None; self.ids.len()];
        for (block_id, block) in module.blocks.iter_enumerated() {
            let first = self.starts[block_id];
            for (index, inst) in block.instructions.iter().enumerate() {
                if self.repeats(block_id, index) {
                    metrics[first + index] = metric(block, inst);
                }
            }
        }
        metrics
    }

    /// Hashes the `len` instructions from flat index `first` of `block`.
    fn range(&self, block: BlockId, first: usize, len: usize) -> u64 {
        let prefix = &self.prefixes[first + block.index()..];
        prefix[len].wrapping_sub(prefix[0].wrapping_mul(self.powers[len]))
    }
}

/// The stack effect of a run so far.
#[derive(Clone, Copy, Default)]
struct StackDelta {
    delta: i32,
    inputs: i32,
}

impl StackDelta {
    fn apply(&mut self, (reads, pops, pushes): (u16, u16, u16)) {
        self.inputs = self.inputs.max(i32::from(reads) - self.delta);
        self.delta = self.delta - i32::from(pops) + i32::from(pushes);
    }

    fn outputs(self) -> i32 {
        self.inputs + self.delta
    }
}

/// A run start growing one instruction at a time, with the caller's state for the run so far.
struct Walk<S> {
    block: BlockId,
    /// The flat index of the first instruction, as in [`RunHashes::ids`].
    first: usize,
    len: usize,
    max_len: usize,
    hash: u64,
    state: S,
}

/// Returns the runs of at least `min_len` instructions that occur at least twice, each with its
/// sites ordered by block and start.
///
/// All walks grow together, one instruction per round, so each round can count equal runs. A run
/// that occurs once cannot have a longer extension that occurs twice, so its walk stops. Most
/// runs are unique, which keeps the work well below that of visiting every window of every start.
/// A hash collision only keeps a walk longer, and the exact map below separates the runs, so the
/// result matches grouping every window.
///
/// `metrics` holds each instruction's per-run data by flat index, with `None` for an instruction
/// no run may contain. Runs start at a split point and have at most `max_len` instructions.
/// `step` adds one instruction to a walk's state and returns whether longer runs may still
/// qualify, and `accept` turns a counted run into a candidate site.
fn repeated_runs<'h, M: Copy, S: Copy + Default>(
    module: &Module,
    hashes: &'h RunHashes,
    metrics: &[Option<M>],
    max_len: usize,
    min_len: usize,
    step: impl Fn(&mut S, M) -> bool,
    accept: impl Fn(&S, BlockId, usize, usize) -> Option<Site>,
) -> Vec<(RunSlice<'h>, SmallVec<[Site; 2]>)> {
    // Starts that cannot take their first instruction never become walks.
    let mut walks = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let first = hashes.starts[block_id];
        for index in 0..block.instructions.len() {
            let mut state = S::default();
            if let Some(metric) = metrics[first + index]
                && is_split_point(&block.instructions, index)
                && step(&mut state, metric)
            {
                let max_len = (block.instructions.len() - index).min(max_len);
                walks.push(Walk {
                    block: block_id,
                    first: first + index,
                    len: 1,
                    max_len,
                    hash: 0,
                    state,
                });
            }
        }
    }
    let mut groups = FxHashMap::<RunSlice<'_>, SmallVec<[Site; 2]>>::default();
    let mut counts = Vec::new();
    while let Some(len) = walks.first().map(|walk| walk.len) {
        if len >= min_len {
            // Count each run's hash in a table with eight slots per walk, saturating at two.
            let slots = (walks.len() * 8).next_power_of_two();
            let shift = 64 - slots.trailing_zeros();
            let slot = |hash: u64| (hash.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> shift) as usize;
            counts.clear();
            counts.resize(slots, 0u8);
            for walk in &mut walks {
                walk.hash = hashes.range(walk.block, walk.first, walk.len);
                let count = &mut counts[slot(walk.hash)];
                *count += u8::from(*count < 2);
            }
            walks.retain(|walk| counts[slot(walk.hash)] >= 2);
            for walk in &walks {
                let start = walk.first - hashes.starts[walk.block];
                if let Some(site) = accept(&walk.state, walk.block, start, walk.len) {
                    let key = RunSlice {
                        hash: walk.hash,
                        ids: &hashes.ids[walk.first..walk.first + walk.len],
                    };
                    groups.entry(key).or_default().push(site);
                }
            }
        }
        walks.retain_mut(|walk| {
            walk.len < walk.max_len
                && metrics[walk.first + walk.len].is_some_and(|metric| {
                    walk.len += 1;
                    step(&mut walk.state, metric)
                })
        });
    }
    groups.into_iter().filter(|(_, sites)| sites.len() >= 2).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunSlice<'a> {
    hash: u64,
    ids: &'a [usize],
}

impl Hash for RunSlice<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

#[derive(Default)]
struct RunState {
    inline_store_literals: FxHashSet<U256>,
    labels: Option<FreshLabels>,
}

/// Credits only identical, splittable suffixes of already-selected stubs with the same stack
/// convention. The normal pipeline's later tail merge can share their body and return sequence.
fn shared_return_tail_savings(
    target: &Target,
    body: &[Instruction],
    site: Site,
    chosen: &[Group],
) -> usize {
    chosen
        .iter()
        .rev()
        .take(64)
        .filter(|group| {
            group.sites[0].inputs == site.inputs && group.sites[0].outputs == site.outputs
        })
        .filter_map(|group| {
            let common = body
                .iter()
                .rev()
                .zip(group.body.iter().rev())
                .take_while(|(a, b)| MachineInstKey::new(a) == MachineInstKey::new(b))
                .count();
            let common = (1..=common).rev().find(|&common| {
                is_split_point(body, body.len() - common)
                    && is_split_point(&group.body, group.body.len() - common)
            })?;
            let bytes = lower_bound(*target, &body[body.len() - common..]);
            Some(target.shared_return_tail_savings(bytes as u32, site.outputs) as usize)
        })
        .max()
        .unwrap_or(0)
}
