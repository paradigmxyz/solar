//! Materialize and pack program data.

use super::EvmPass;
use crate::{
    MAX_DATA_SUBSTRING_ENTRIES,
    backend::evm::{
        ir::{
            BlockId, Data, DataId, DataRef, Instruction, Module, PushValue,
            default_instruction_stack_effect, immediate_materialization_cost,
        },
        op, push_len,
    },
};
use alloy_primitives::{Bytes, U256};
use memchr::memmem;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;

pub(super) struct PackData;

impl EvmPass for PackData {
    fn name(&self) -> &'static str {
        "pack-data"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut references = data_references(module);
        let packed = pack_data(module, &references);
        if packed {
            references = data_references(module);
        }
        let materialized = materialize_data(gcx, module, &references);
        if materialized {
            let references = data_references(module);
            pack_data(module, &references);
        }
        packed || materialized
    }
}

pub(super) struct FinalizeData;

impl EvmPass for FinalizeData {
    fn name(&self) -> &'static str {
        "finalize-data"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let references = data_references(module);
        pack_data(module, &references)
    }
}

struct Rewrite {
    block: BlockId,
    start: usize,
    end: usize,
    old_size: usize,
    old_gas: usize,
}

struct PreparedRewrite {
    block: BlockId,
    start: usize,
    end: usize,
    data: DataRef,
    size: usize,
}

struct DataPool {
    entries: Vec<(DataId, Bytes, usize, bool)>,
    exact: FxHashMap<Bytes, DataRef>,
}

enum Placement {
    Existing(DataRef),
    New { additional_bytes: isize, contained: FxHashSet<DataId> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Improvement {
    runtime_gas: i128,
    bytes: i128,
}

impl Improvement {
    fn add(&mut self, other: Self) {
        self.runtime_gas += other.runtime_gas;
        self.bytes += other.bytes;
    }
}

impl DataPool {
    fn new(data: &IndexVec<DataId, Data>, references: &DataReferences) -> Self {
        Self {
            entries: data
                .iter_enumerated()
                .map(|(id, data)| {
                    (id, data.bytes.clone(), references.counts[id], references.subslice_safe[id])
                })
                .collect(),
            exact: data
                .iter_enumerated()
                .map(|(id, data)| (data.bytes.clone(), DataRef::new(id, 0)))
                .collect(),
        }
    }

    fn placement(
        &self,
        data: &[u8],
        allow_absorption: bool,
        total_reference_count: usize,
    ) -> Placement {
        if let Some(&data) = self.exact.get(data) {
            return Placement::Existing(data);
        }
        if self.entries.len() >= MAX_DATA_SUBSTRING_ENTRIES {
            return Placement::New {
                additional_bytes: data.len() as isize,
                contained: FxHashSet::default(),
            };
        }
        let mut contained = FxHashSet::default();
        let mut removed = 0;
        for (id, known, _, subslice_safe) in &self.entries {
            if let Some(offset) = memmem::find(known, data) {
                return Placement::Existing(DataRef::new(*id, data_offset(offset)));
            }
            if allow_absorption
                && *subslice_safe
                && memmem::find(data, known).is_some()
                && known.len() > total_reference_count
            {
                contained.insert(*id);
                removed += known.len();
            }
        }
        Placement::New { additional_bytes: data.len() as isize - removed as isize, contained }
    }

    fn intern(
        &mut self,
        module: &mut Module,
        bytes: Bytes,
        reference_count: usize,
        placement: Placement,
    ) -> DataRef {
        let contained = match placement {
            Placement::Existing(data) => {
                if self.entries.len() < MAX_DATA_SUBSTRING_ENTRIES {
                    self.entries
                        .iter_mut()
                        .find(|(id, _, _, _)| *id == data.id)
                        .expect("existing data is pooled")
                        .2 += reference_count;
                }
                return data;
            }
            Placement::New { contained, .. } => contained,
        };
        let mut reference_count = reference_count;
        if !contained.is_empty() {
            self.entries.retain(|(id, bytes, inherited_references, _)| {
                let retain = !contained.contains(id);
                if !retain {
                    self.exact.remove(bytes);
                    reference_count += *inherited_references;
                }
                retain
            });
        }
        let id = module.data.push(Data { bytes: bytes.clone(), named: true });
        self.entries.push((id, bytes.clone(), reference_count, true));
        self.exact.insert(bytes, DataRef::new(id, 0));
        DataRef::new(id, 0)
    }
}

fn materialize_data(gcx: Gcx<'_>, module: &mut Module, references: &DataReferences) -> bool {
    let mut groups = FxHashMap::<Bytes, Vec<Rewrite>>::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let mut start = 0;
        while start < block.instructions.len() {
            let Some((data, rewrite)) = find_run(gcx, block_id, &block.instructions, start) else {
                start += 1;
                continue;
            };
            start = rewrite.end;
            groups.entry(data).or_default().push(rewrite);
        }
    }
    if groups.is_empty() {
        return false;
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_unstable_by(|(a, _), (b, _)| {
        a.len().cmp(&b.len()).then_with(|| a.as_ref().cmp(b.as_ref()))
    });

    let total_reference_count = references.counts.iter().sum();
    let allow_absorption =
        module.data.len().saturating_add(groups.len()) < MAX_DATA_SUBSTRING_ENTRIES;
    let mut pool = DataPool::new(&module.data, references);
    let mut prepared = Vec::new();
    let mut rejected = Vec::<(Bytes, Vec<Rewrite>)>::new();
    for (data, rewrites) in groups {
        let placement = pool.placement(&data, allow_absorption, total_reference_count);
        let additional_bytes = placement_additional_bytes(&placement);
        let mut improvement = rewrite_improvement(gcx, data.len(), &rewrites, additional_bytes);
        let mut absorbed = Vec::new();
        for (index, (contained, contained_rewrites)) in
            rejected.iter().take(MAX_DATA_SUBSTRING_ENTRIES).enumerate()
        {
            let Some(offset) = memmem::find(&data, contained) else {
                continue;
            };
            let contained_improvement =
                rewrite_improvement(gcx, contained.len(), contained_rewrites, 0);
            if is_profitable(gcx, contained_improvement) {
                improvement.add(contained_improvement);
                absorbed.push((index, offset));
            }
        }
        if !is_profitable(gcx, improvement) {
            rejected.push((data, rewrites));
            continue;
        }

        let size = data.len();
        let reference_count = rewrites.len()
            + absorbed.iter().map(|(index, _)| rejected[*index].1.len()).sum::<usize>();
        let data_ref = pool.intern(module, data.clone(), reference_count, placement);
        prepare_rewrites(&mut prepared, rewrites, data_ref, size);
        for (index, offset) in absorbed.into_iter().rev() {
            let (contained, rewrites) = rejected.swap_remove(index);
            let offset =
                data_ref.offset.checked_add(data_offset(offset)).expect("data offset overflow");
            prepare_rewrites(
                &mut prepared,
                rewrites,
                DataRef::new(data_ref.id, offset),
                contained.len(),
            );
        }
    }
    prepared.sort_unstable_by_key(|rewrite| (rewrite.block, rewrite.start));
    for rewrite in prepared.iter().rev() {
        module.blocks[rewrite.block].instructions.splice(
            rewrite.start..rewrite.end,
            [
                Instruction::push_value(U256::from(rewrite.size)),
                Instruction::push_data(rewrite.data),
                Instruction::opcode(op::DUP3),
                Instruction::opcode(op::CODECOPY),
            ],
        );
    }
    !prepared.is_empty()
}

fn prepare_rewrites(
    prepared: &mut Vec<PreparedRewrite>,
    rewrites: Vec<Rewrite>,
    data: DataRef,
    size: usize,
) {
    prepared.extend(rewrites.into_iter().map(|rewrite| PreparedRewrite {
        block: rewrite.block,
        start: rewrite.start,
        end: rewrite.end,
        data,
        size,
    }));
}

fn find_run(
    gcx: Gcx<'_>,
    block: BlockId,
    instructions: &[Instruction],
    start: usize,
) -> Option<(Bytes, Rewrite)> {
    let [value, dup, store, ..] = instructions.get(start..)? else { return None };
    let first = value.concrete_immediate()?;
    if raw_opcode(dup) != Some(op::DUP2) || raw_opcode(store) != Some(op::MSTORE) {
        return None;
    }

    let mut end = start + 3;
    let mut words = 1usize;
    while let Some(window) = instructions.get(end..end + 6) {
        let [offset, dup, add, value, swap, store] = window else { unreachable!() };
        if offset.concrete_immediate() != Some(U256::from(words * 32))
            || raw_opcode(dup) != Some(op::DUP2)
            || raw_opcode(add) != Some(op::ADD)
            || raw_opcode(swap) != Some(op::SWAP1)
            || raw_opcode(store) != Some(op::MSTORE)
        {
            break;
        }
        if value.concrete_immediate().is_none() {
            break;
        }
        words += 1;
        end += 6;
    }
    let mut data = Vec::with_capacity(words * 32);
    data.extend_from_slice(&first.to_be_bytes::<32>());
    for window in instructions[start + 3..end].as_chunks::<6>().0 {
        data.extend_from_slice(&window[3].concrete_immediate().unwrap().to_be_bytes::<32>());
    }
    let instructions = &instructions[start..end];
    let old_size = instructions.iter().map(|inst| encoded_len(gcx, inst)).sum();
    let old_gas = instructions.iter().map(|inst| static_gas(gcx, inst)).sum();
    Some((data.into(), Rewrite { block, start, end, old_size, old_gas }))
}

fn rewrite_improvement(
    gcx: Gcx<'_>,
    size: usize,
    rewrites: &[Rewrite],
    additional_bytes: isize,
) -> Improvement {
    let old_bytes = rewrites.iter().map(|rewrite| rewrite.old_size).sum::<usize>() as i128;
    let new_bytes = (data_copy_size(gcx, size) * rewrites.len()) as i128 + additional_bytes as i128;
    let old_gas = rewrites.iter().map(|rewrite| rewrite.old_gas).sum::<usize>() as i128;
    let new_gas = (data_copy_gas(size) * rewrites.len()) as i128;
    let bytes = old_bytes - new_bytes;
    let runtime_gas = old_gas - new_gas;
    Improvement { runtime_gas, bytes }
}

fn is_profitable(gcx: Gcx<'_>, improvement: Improvement) -> bool {
    if gcx.sess.opts.optimization.is_gas() {
        // Static sites have no execution-frequency estimate, so never buy gas
        // by growing code for a copy that may stay cold.
        improvement.runtime_gas > 0 && improvement.bytes >= 0
    } else {
        improvement.bytes > 0
    }
}

fn placement_additional_bytes(placement: &Placement) -> isize {
    match placement {
        Placement::Existing(_) => 0,
        Placement::New { additional_bytes, .. } => *additional_bytes,
    }
}

fn pack_data(module: &mut Module, references: &DataReferences) -> bool {
    if module.data.is_empty() {
        return false;
    }
    let total_reference_count = references.counts.iter().sum();
    let mut referenced = references
        .counts
        .iter_enumerated()
        .filter_map(|(id, &count)| (count != 0).then_some(id))
        .collect::<Vec<_>>();
    if referenced.is_empty() {
        module.data.clear();
        return true;
    }
    if module.data.len() == 1 {
        return false;
    }
    referenced.sort_unstable_by(|&a, &b| {
        module.data[b].bytes.len().cmp(&module.data[a].bytes.len()).then_with(|| a.cmp(&b))
    });

    let mut packed = IndexVec::<DataId, Data>::new();
    let mut sources = IndexVec::<DataId, DataId>::new();
    let mut exact = FxHashMap::<Bytes, DataId>::default();
    let mut remap = FxHashMap::default();
    for old_id in referenced {
        let data = &module.data[old_id];
        let data_ref = if let Some(&id) = exact.get(&data.bytes) {
            DataRef::new(id, 0)
        } else if let Some(data_ref) = (references.subslice_safe[old_id]
            && module.data.len() < MAX_DATA_SUBSTRING_ENTRIES)
            .then(|| find_data(&packed, &sources, &data.bytes, old_id, total_reference_count))
            .flatten()
        {
            data_ref
        } else {
            let id = packed.push(Data { bytes: data.bytes.clone(), named: data.named });
            sources.push(old_id);
            exact.insert(data.bytes.clone(), id);
            DataRef::new(id, 0)
        };
        packed[data_ref.id].named |= data.named;
        remap.insert(old_id, data_ref);
    }

    let mut order = packed.indices().collect::<Vec<_>>();
    order.sort_unstable_by_key(|&id| sources[id]);
    let mut ordered = IndexVec::<DataId, Data>::new();
    let mut packed_remap = IndexVec::<DataId, DataId>::from_vec(vec![DataId::new(0); packed.len()]);
    for old_id in order {
        let new_id = ordered.push(packed[old_id].clone());
        packed_remap[old_id] = new_id;
    }
    for data in remap.values_mut() {
        data.id = packed_remap[data.id];
    }

    if ordered == module.data {
        return false;
    }
    module.data = ordered;
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            if let Some(PushValue::Data(data)) = &mut inst.value {
                let base = remap[&data.id];
                data.id = base.id;
                data.offset = data.offset.checked_add(base.offset).expect("data offset overflow");
            }
        }
    }
    true
}

fn find_data(
    data: &IndexVec<DataId, Data>,
    sources: &IndexVec<DataId, DataId>,
    needle: &[u8],
    needle_id: DataId,
    total_reference_count: usize,
) -> Option<DataRef> {
    data.iter_enumerated().find_map(|(id, known)| {
        memmem::find(&known.bytes, needle).and_then(|offset| {
            let cannot_widen = sources[id] < needle_id;
            // A later address can widen its own PUSH and shift every other data
            // relocation. Each can widen by at most one byte within EVM size limits.
            (cannot_widen || needle.len() > total_reference_count)
                .then(|| DataRef::new(id, data_offset(offset)))
        })
    })
}

#[derive(Clone, Copy)]
enum DataStackValue {
    Unknown,
    Immediate(U256),
    Data(DataRef),
}

struct DataReferences {
    counts: IndexVec<DataId, usize>,
    subslice_safe: IndexVec<DataId, bool>,
}

fn data_references(module: &Module) -> DataReferences {
    let mut references = DataReferences {
        counts: IndexVec::from_vec(vec![0; module.data.len()]),
        subslice_safe: IndexVec::from_vec(vec![true; module.data.len()]),
    };
    let mut stack = Vec::new();
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(data) = inst.pushed_data() {
                references.counts[data.id] += 1;
                stack.push(DataStackValue::Data(data));
                continue;
            }
            if let Some(value) = inst.concrete_immediate() {
                stack.push(DataStackValue::Immediate(value));
                continue;
            }
            if inst.is_encoded_push() {
                stack.push(DataStackValue::Unknown);
                continue;
            }

            match inst.opcode {
                opcode if (op::DUP1..=op::DUP16).contains(&opcode) => {
                    let depth = usize::from(opcode - op::DUP1) + 1;
                    let value = stack
                        .len()
                        .checked_sub(depth)
                        .map_or(DataStackValue::Unknown, |index| stack[index]);
                    stack.push(value);
                }
                opcode if (op::SWAP1..=op::SWAP16).contains(&opcode) => {
                    let depth = usize::from(opcode - op::SWAP1) + 1;
                    ensure_stack_depth(&mut stack, depth + 1);
                    let top = stack.len() - 1;
                    stack.swap(top, top - depth);
                }
                op::DUPN => {
                    mark_stack_data_unsafe(&stack, &mut references.subslice_safe);
                    stack.push(DataStackValue::Unknown);
                }
                op::SWAPN | op::EXCHANGE => {
                    mark_stack_data_unsafe(&stack, &mut references.subslice_safe);
                    stack.fill(DataStackValue::Unknown);
                }
                _ => {
                    let Some(effect) =
                        inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))
                    else {
                        mark_stack_data_unsafe(&stack, &mut references.subslice_safe);
                        stack.clear();
                        continue;
                    };
                    let inputs = usize::from(effect.inputs);
                    ensure_stack_depth(&mut stack, inputs);
                    let first_input = stack.len() - inputs;
                    for (index, value) in stack[first_input..].iter().rev().enumerate() {
                        let DataStackValue::Data(data) = value else { continue };
                        let bounded = inst.opcode == op::CODECOPY
                            && index == 1
                            && matches!(
                                stack.get(stack.len() - 3),
                                Some(DataStackValue::Immediate(size))
                                    if data_copy_is_bounded(module, *data, *size)
                            );
                        if !bounded {
                            references.subslice_safe[data.id] = false;
                        }
                    }
                    stack.truncate(first_input);
                    stack.extend(std::iter::repeat_n(
                        DataStackValue::Unknown,
                        usize::from(effect.outputs),
                    ));
                }
            }
        }
        mark_stack_data_unsafe(&stack, &mut references.subslice_safe);
        stack.clear();
    }
    references
}

fn mark_stack_data_unsafe(stack: &[DataStackValue], subslice_safe: &mut IndexVec<DataId, bool>) {
    for value in stack {
        if let DataStackValue::Data(data) = value {
            subslice_safe[data.id] = false;
        }
    }
}

fn ensure_stack_depth(stack: &mut Vec<DataStackValue>, depth: usize) {
    if stack.len() < depth {
        stack.splice(0..0, std::iter::repeat_n(DataStackValue::Unknown, depth - stack.len()));
    }
}

fn data_copy_is_bounded(module: &Module, data: DataRef, size: U256) -> bool {
    let Ok(size) = usize::try_from(size) else { return false };
    module.data.get(data.id).is_some_and(|entry| {
        (data.offset as usize).checked_add(size).is_some_and(|end| end <= entry.bytes.len())
    })
}

fn data_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect("data offset exceeds `u32`")
}

fn encoded_len(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    inst.concrete_immediate().map_or(1, |value| super::compact_pushes::selected_len(gcx, value))
}

fn data_copy_size(gcx: Gcx<'_>, size: usize) -> usize {
    // Use PUSH3 for the unresolved data address so this estimate cannot grow
    // an EIP-170-sized program if the final address crosses the PUSH2 boundary.
    push_len(gcx.sess.opts.evm_version, U256::from(size)) + 4 + 2
}

fn data_copy_gas(size: usize) -> usize {
    12 + 3 * size.div_ceil(32)
}

fn static_gas(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    inst.concrete_immediate()
        .map_or(3, |value| immediate_materialization_cost(gcx.sess.opts.evm_version, value).1)
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}
