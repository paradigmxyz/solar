//! Materialize and pack program data.

use super::EvmPass;
use crate::backend::evm::{
    ir::{
        BlockId, Data, DataId, DataRef, Instruction, Module, PushValue,
        immediate_materialization_cost,
    },
    op, push_len,
};
use alloy_primitives::{Bytes, U256};
use memchr::memmem;
use solar_data_structures::{
    bit_set::DenseBitSet,
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
        let packed = pack_data(module);
        let materialized = materialize_data(gcx, module);
        if materialized {
            pack_data(module);
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
        pack_data(module)
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
    entries: Vec<(DataId, Bytes)>,
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
    fn new(data: &IndexVec<DataId, Data>) -> Self {
        Self {
            entries: data.iter_enumerated().map(|(id, data)| (id, data.bytes.clone())).collect(),
        }
    }

    fn placement(&self, data: &[u8]) -> Placement {
        let mut contained = FxHashSet::default();
        let mut removed = 0;
        for (id, known) in &self.entries {
            if let Some(offset) = memmem::find(known, data) {
                return Placement::Existing(DataRef::new(*id, data_offset(offset)));
            }
            if memmem::find(data, known).is_some() {
                contained.insert(*id);
                removed += known.len();
            }
        }
        Placement::New { additional_bytes: data.len() as isize - removed as isize, contained }
    }

    fn intern(&mut self, module: &mut Module, bytes: Bytes, placement: Placement) -> DataRef {
        let contained = match placement {
            Placement::Existing(data) => return data,
            Placement::New { contained, .. } => contained,
        };
        self.entries.retain(|(id, _)| !contained.contains(id));
        let id = module.data.push(Data { bytes: bytes.clone(), named: true });
        self.entries.push((id, bytes));
        DataRef::new(id, 0)
    }
}

fn materialize_data(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut rewrites = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let mut start = 0;
        while start < block.instructions.len() {
            let Some((data, rewrite)) = find_run(gcx, block_id, &block.instructions, start) else {
                start += 1;
                continue;
            };
            start = rewrite.end;
            rewrites.push((data, rewrite));
        }
    }
    let mut groups = FxHashMap::<Bytes, Vec<Rewrite>>::default();
    for (data, rewrite) in rewrites {
        groups.entry(data).or_default().push(rewrite);
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_unstable_by(|(a, _), (b, _)| {
        a.len().cmp(&b.len()).then_with(|| a.as_ref().cmp(b.as_ref()))
    });

    let mut pool = DataPool::new(&module.data);
    let mut prepared = Vec::new();
    let mut rejected = Vec::<(Bytes, Vec<Rewrite>)>::new();
    for (data, rewrites) in groups {
        let placement = pool.placement(&data);
        let additional_bytes = placement_additional_bytes(&placement);
        let mut improvement = rewrite_improvement(gcx, data.len(), &rewrites, additional_bytes);
        let mut absorbed = Vec::new();
        for (index, (contained, contained_rewrites)) in rejected.iter().enumerate() {
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
        let data_ref = pool.intern(module, data.clone(), placement);
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
    if words < 2 {
        return None;
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

fn pack_data(module: &mut Module) -> bool {
    let mut referenced = DenseBitSet::new_empty(module.data.len());
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(PushValue::Data(data)) = inst.value {
                referenced.insert(data.id);
            }
        }
    }
    let mut referenced = referenced.iter().collect::<Vec<_>>();
    referenced.sort_unstable_by(|&a, &b| {
        module.data[b].bytes.len().cmp(&module.data[a].bytes.len()).then_with(|| a.cmp(&b))
    });

    let mut packed = IndexVec::<DataId, Data>::new();
    let mut remap = FxHashMap::default();
    for old_id in referenced {
        let data = &module.data[old_id];
        let data_ref = if let Some(data_ref) = find_data(&packed, &data.bytes) {
            if data.named {
                packed[data_ref.id].named = true;
            }
            data_ref
        } else {
            let id = packed.push(Data { bytes: data.bytes.clone(), named: data.named });
            DataRef::new(id, 0)
        };
        remap.insert(old_id, data_ref);
    }

    let changed = packed != module.data;
    module.data = packed;
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            if let Some(PushValue::Data(data)) = &mut inst.value {
                let base = remap[&data.id];
                data.id = base.id;
                data.offset = data.offset.checked_add(base.offset).expect("data offset overflow");
            }
        }
    }
    changed
}

fn find_data(data: &IndexVec<DataId, Data>, needle: &[u8]) -> Option<DataRef> {
    data.iter_enumerated().find_map(|(id, known)| {
        memmem::find(&known.bytes, needle).map(|offset| DataRef::new(id, data_offset(offset)))
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
