//! Literal data packing and contiguous memory-copy formation.
//!
//! Store runs are recognized as physical stack sequences retaining one base
//! pointer. Their complete word bytes are interned jointly so that repeated runs
//! and contained runs can share one data region. A rewrite must pay for the data
//! bytes and every replacement reference; gas mode requires at least two words
//! and leaves highly repeated runs for global outlining. Existing data is folded
//! only into an earlier entry to avoid expanding early references to later data.
//! Any escaped data address, out-of-range copy or observable program size blocks
//! packing. Memory copies combine only disjoint, contiguous source/destination
//! ranges on MCOPY-capable forks. All changes precede relocation and assembly.

use super::{Data, DataId, EvmPass, InstKind, Instruction, Module};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_config::OptimizationMode;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_interface::sym;
use solar_sema::Gcx;

pub(super) struct ConstantData;
pub(super) struct PackData;
pub(super) struct CoalesceCopies;

impl EvmPass for ConstantData {
    fn name(&self) -> &'static str {
        "constant-data"
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        pack(gcx, module, false)
    }
}
impl EvmPass for PackData {
    fn name(&self) -> &'static str {
        "pack-data"
    }
    fn is_required(&self) -> bool {
        true
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        pack(gcx, module, true)
    }
}
impl EvmPass for CoalesceCopies {
    fn name(&self) -> &'static str {
        "coalesce-copies"
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !gcx.sess.opts.evm_version.has_mcopy() {
            return false;
        }
        let Ok(heights) = super::verify::stack_heights(module) else { return false };
        let reachable = super::verify::physical_reachability(module);
        let mut changed = false;
        for id in module.block_ids().collect::<Vec<_>>() {
            let mut index = 0;
            while index + 8 <= module.blocks[id].insts.len() {
                let Some(run) = copy_run(&module.blocks[id].insts[index..]) else {
                    index += 1;
                    continue;
                };
                if let Some(mut replacement) = run.replacement
                    && super::verify::rewrite_fits(
                        module,
                        &heights,
                        &reachable,
                        id,
                        index,
                        index + run.len,
                        &replacement,
                    )
                {
                    if module.debug_info_tracked {
                        transfer_copy_debug(
                            &module.blocks[id].insts[index..index + run.len],
                            &mut replacement,
                        );
                    }
                    // mstore(dest+i, mload(src+i)) -> mcopy(dest, src, bytes)
                    module.blocks[id].insts.splice(index..index + run.len, replacement);
                    changed = true;
                    index += 4;
                } else {
                    index += run.len;
                }
            }
        }
        changed
    }
}

/// Unions a sequential copy run's origins, placing its sole consistent events on the copy.
fn transfer_copy_debug(source: &[Instruction], replacement: &mut [Instruction; 4]) {
    let mut origins = source.iter().filter_map(|inst| inst.debug.as_deref());
    let Some(first) = origins.next() else { return };
    let mut debug = first.clone();
    for origin in origins {
        debug.merge(origin);
    }
    // An event absent from other operations is not a conflicting alternative path.
    let invoke = source.iter().find_map(|inst| inst.debug.as_ref()?.function_invoke);
    let exit = source.iter().find_map(|inst| inst.debug.as_ref()?.function_exit);
    debug.function_invoke = invoke.filter(|event| {
        source
            .iter()
            .filter_map(|inst| inst.debug.as_ref()?.function_invoke)
            .all(|other| other == *event)
    });
    debug.function_exit = exit.filter(|event| {
        source
            .iter()
            .filter_map(|inst| inst.debug.as_ref()?.function_exit)
            .all(|other| other == *event)
    });
    // size; source; destination; copy
    replacement[3].debug = Some(Box::new(debug.clone()));
    debug.function_invoke = None;
    debug.function_exit = None;
    for inst in &mut replacement[..3] {
        inst.debug = Some(Box::new(debug.clone()));
    }
}

struct CopyRun {
    len: usize,
    replacement: Option<[Instruction; 4]>,
}

/// Recognizes a contiguous copy run without proving its surrounding stack capacity.
fn copy_run(insts: &[Instruction]) -> Option<CopyRun> {
    let (src, dest) = word_copy(insts)?;
    let mut words = 1usize;
    while let Some((next_src, next_dest)) = word_copy(&insts[words * 4..]) {
        if src.checked_add(U256::from(words * 32)) != Some(next_src)
            || dest.checked_add(U256::from(words * 32)) != Some(next_dest)
        {
            break;
        }
        words += 1;
    }
    let bytes = U256::from(words * 32);
    let replacement = if words >= 2
        && let Some(src_end) = src.checked_add(bytes)
        && let Some(dest_end) = dest.checked_add(bytes)
        && (src_end <= dest || dest_end <= src)
    {
        // push bytes; push src; push dest; mcopy
        Some(
            [
                InstKind::Push(bytes),
                InstKind::Push(src),
                InstKind::Push(dest),
                InstKind::Op(op::MCOPY),
            ]
            .map(Into::into),
        )
    } else {
        None
    };
    Some(CopyRun { len: words * 4, replacement })
}

/// Estimates copy scheduling after legal contiguous runs can coalesce.
///
/// This query does not emit a rewrite or assume its stack proof succeeded. The
/// actual pass still requires room for MCOPY's three operands. Memory expansion
/// is omitted, as with other physical scheduling estimates.
pub(super) fn copy_cost(
    version: solar_config::EvmVersion,
    insts: &[Instruction],
) -> (usize, usize) {
    let mut cost = (0, 0);
    let mut index = 0;
    while index < insts.len() {
        let (len, (bytes, gas)) = if version.has_mcopy()
            && let Some(run) = copy_run(&insts[index..])
            && let Some(replacement) = run.replacement
        {
            let (bytes, gas) = super::immediate::cost(version, &replacement);
            (run.len, (bytes, gas + 3 * (run.len / 4)))
        } else {
            (1, super::immediate::cost(version, &insts[index..index + 1]))
        };
        cost.0 += gas;
        cost.1 += bytes;
        index += len;
    }
    cost
}

fn word_copy(insts: &[Instruction]) -> Option<(U256, U256)> {
    let [source, load, destination, store, ..] = insts else { return None };
    if let InstKind::Push(src) = source.kind
        && load.kind == InstKind::Op(op::MLOAD)
        && let InstKind::Push(dest) = destination.kind
        && store.kind == InstKind::Op(op::MSTORE)
        && [source, load, destination, store].iter().all(|inst| inst.stack_effect.is_none())
    {
        Some((src, dest))
    } else {
        None
    }
}

fn movable_data(module: &Module) -> bool {
    for id in module.block_ids() {
        let insts = &module.blocks[id].insts;
        for (index, inst) in insts.iter().enumerate() {
            match inst.kind {
                InstKind::Op(op::CODESIZE) => return false,
                InstKind::PushData { id, offset } => {
                    if index == 0 || index + 2 >= insts.len() {
                        return false;
                    }
                    let InstKind::Push(length) = insts[index - 1].kind else { return false };
                    if !matches!(insts[index + 1].kind, InstKind::Push(_) | InstKind::Dup(_))
                        || insts[index + 2].kind != InstKind::Op(op::CODECOPY)
                        || length
                            > U256::from(
                                module.data[id].bytes.len().saturating_sub(offset as usize),
                            )
                    {
                        return false;
                    }
                }
                InstKind::Op(op::CODECOPY)
                    if index < 2 || !matches!(insts[index - 2].kind, InstKind::PushData { .. }) =>
                {
                    return false;
                }
                _ => {}
            }
        }
    }
    true
}

struct Run {
    block: super::BlockId,
    start: usize,
    end: usize,
    bytes: Vec<u8>,
    savings: usize,
    data: Option<(DataId, u32)>,
}

fn store_runs(gcx: Gcx<'_>, module: &Module) -> Vec<Run> {
    let mut runs = Vec::new();
    for id in module.block_ids() {
        let insts = &module.blocks[id].insts;
        let mut index = 0;
        while index + 3 <= insts.len() {
            let [value, address, store, ..] = &insts[index..] else { break };
            if let InstKind::Push(value) = value.kind
                && address.kind == InstKind::Dup(2)
                && store.kind == InstKind::Op(op::MSTORE)
            {
                let mut bytes = value.to_be_bytes::<32>().to_vec();
                let mut end = index + 3;
                while let [offset, base, add, value, swap, store, ..] = &insts[end..] {
                    if offset.kind != InstKind::Push(U256::from(bytes.len()))
                        || base.kind != InstKind::Dup(2)
                        || add.kind != InstKind::Op(op::ADD)
                        || swap.kind != InstKind::Swap(1)
                        || store.kind != InstKind::Op(op::MSTORE)
                    {
                        break;
                    }
                    let InstKind::Push(value) = value.kind else { break };
                    bytes.extend_from_slice(&value.to_be_bytes::<32>());
                    end += 6;
                }
                if insts[index..end].iter().all(|inst| inst.stack_effect.is_none()) {
                    let before = insts[index..end]
                        .iter()
                        .map(|inst| match inst.kind {
                            InstKind::Push(value) => {
                                super::immediate_materialization_cost(
                                    gcx.sess.opts.evm_version,
                                    value,
                                )
                                .0
                            }
                            _ => 1,
                        })
                        .sum::<usize>();
                    let after = super::immediate_materialization_cost(
                        gcx.sess.opts.evm_version,
                        U256::from(bytes.len()),
                    )
                    .0 + 4;
                    runs.push(Run {
                        block: id,
                        start: index,
                        end,
                        bytes,
                        savings: before.saturating_sub(after),
                        data: None,
                    });
                }
                index = end;
            } else {
                index += 1;
            }
        }
    }
    runs
}

fn subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        Some(0)
    } else {
        haystack.windows(needle.len()).position(|window| window == needle)
    }
}

fn pack(gcx: Gcx<'_>, module: &mut Module, existing: bool) -> bool {
    if !movable_data(module) {
        return false;
    }
    if matches!(gcx.sess.opts.optimization, OptimizationMode::None) {
        return if existing { compact_data(module, false) } else { false };
    }
    let mut changed = existing && compact_data(module, true);
    let Ok(heights) = super::verify::stack_heights(module) else { return changed };
    let reachable = super::verify::physical_reachability(module);
    let mut runs = store_runs(gcx, module);
    runs.retain(|run| {
        super::verify::rewrite_fits(
            module,
            &heights,
            &reachable,
            run.block,
            run.start,
            run.end,
            &[
                InstKind::Push(U256::from(run.bytes.len())).into(),
                InstKind::Push(U256::ZERO).into(),
                InstKind::Dup(3).into(),
                InstKind::Op(op::CODECOPY).into(),
            ],
        )
    });
    let gas = gcx.sess.opts.optimization.is_gas();
    let mut order = (0..runs.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| std::cmp::Reverse(runs[index].bytes.len()));
    for index in order {
        if runs[index].data.is_some() {
            continue;
        }
        let bytes = &runs[index].bytes;
        let occurrences = runs.iter().filter(|run| run.bytes == *bytes).count();
        if gas && (bytes.len() < 64 || occurrences >= 4) {
            continue;
        }
        let reused = module
            .data
            .iter_enumerated()
            .find_map(|(id, data)| subslice(&data.bytes, bytes).map(|offset| (id, offset as u32)));
        let group = runs
            .iter()
            .enumerate()
            .filter_map(|(index, run)| {
                (run.data.is_none() && (!gas || run.bytes.len() >= 64))
                    .then(|| subslice(bytes, &run.bytes).map(|offset| (index, offset as u32)))
                    .flatten()
            })
            .collect::<Vec<_>>();
        let savings = group.iter().map(|&(index, _)| runs[index].savings).sum::<usize>();
        if reused.is_none() && savings <= bytes.len() {
            continue;
        }
        let (data_id, base_offset) = if let Some(reused) = reused {
            reused
        } else {
            // @data literal bytes
            (module.data.push(Data { name: Some(sym::literal), bytes: bytes.clone() }), 0)
        };
        for (index, offset) in group {
            runs[index].data = Some((data_id, base_offset + offset));
        }
    }
    for run in runs.into_iter().rev() {
        if let Some((id, offset)) = run.data {
            // base; mstore(base+i, word_i) -> base; size; data; dup3; codecopy
            let mut replacement = [
                InstKind::Push(U256::from(run.bytes.len())),
                InstKind::PushData { id, offset },
                InstKind::Dup(3),
                InstKind::Op(op::CODECOPY),
            ]
            .map(Into::into);
            if module.debug_info_tracked {
                transfer_copy_debug(
                    &module.blocks[run.block].insts[run.start..run.end],
                    &mut replacement,
                );
            }
            module.blocks[run.block].insts.splice(run.start..run.end, replacement);
            changed = true;
        }
    }
    changed
}

fn compact_data(module: &mut Module, merge: bool) -> bool {
    let mut used = DenseBitSet::new_empty(module.data.len());
    for id in module.block_ids() {
        for inst in &module.blocks[id].insts {
            if let InstKind::PushData { id, .. } = inst.kind {
                used.insert(id);
            }
        }
    }
    let mut data = IndexVec::<DataId, Data>::new();
    let mut relocation =
        IndexVec::<DataId, Option<(DataId, u32)>>::from_vec(vec![None; module.data.len()]);
    for id in used.iter() {
        let source = &module.data[id];
        let shared = merge
            .then(|| {
                data.iter_enumerated().find_map(|(id, entry)| {
                    subslice(&entry.bytes, &source.bytes).map(|offset| (id, offset as u32))
                })
            })
            .flatten();
        let target = shared.unwrap_or_else(|| (data.push(source.clone()), 0));
        relocation[id] = Some(target);
    }
    let changed = data != module.data;
    if !changed {
        return false;
    }
    // old data+offset -> packed data+(base_offset+offset)
    for id in module.block_ids().collect::<Vec<_>>() {
        for inst in &mut module.blocks[id].insts {
            if let InstKind::PushData { id, offset } = &mut inst.kind {
                let (target, base) = relocation[*id].expect("referenced data is retained");
                *id = target;
                *offset += base;
            }
        }
    }
    module.data = data;
    true
}
