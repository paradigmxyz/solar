//! Materialize and pack program data.

use super::EvmPass;
use crate::backend::evm::{
    ir::{BlockId, DataId, DataRef, Instruction, Module, PushValue},
    op, push_len,
};
use alloy_primitives::{Bytes, U256};
use memchr::memmem;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_interface::Symbol;
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
    New { additional_bytes: isize, contained: Vec<DataId> },
}

impl DataPool {
    fn new(data: &IndexVec<DataId, Bytes>) -> Self {
        Self { entries: data.iter_enumerated().map(|(id, data)| (id, data.clone())).collect() }
    }

    fn placement(&self, data: &[u8]) -> Placement {
        let mut contained = Vec::new();
        let mut removed = 0;
        for (id, known) in &self.entries {
            if let Some(offset) = memmem::find(known, data) {
                return Placement::Existing(DataRef::new(*id, data_offset(offset)));
            }
            if memmem::find(data, known).is_some() {
                contained.push(*id);
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
        let id = module.data.push(bytes.clone());
        module.data_names.push(Some(crate::data_literal_name(id.index())));
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
        b.len().cmp(&a.len()).then_with(|| a.as_ref().cmp(b.as_ref()))
    });

    let mut pool = DataPool::new(&module.data);
    let mut prepared = Vec::new();
    for (data, rewrites) in groups {
        let placement = pool.placement(&data);
        let additional_bytes = match &placement {
            Placement::Existing(_) => 0,
            Placement::New { additional_bytes, .. } => *additional_bytes,
        };
        let new_code_size = data_copy_size(gcx, data.len()) * rewrites.len();
        let new_size = new_code_size as isize + additional_bytes;
        let old_size = rewrites.iter().map(|rewrite| rewrite.old_size).sum::<usize>();
        if new_size >= old_size as isize {
            continue;
        }
        let size = data.len();
        let data = pool.intern(module, data, placement);
        prepared.extend(rewrites.into_iter().map(|rewrite| PreparedRewrite {
            block: rewrite.block,
            start: rewrite.start,
            end: rewrite.end,
            data,
            size,
        }));
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
    let old_size = instructions[start..end].iter().map(|inst| encoded_len(gcx, inst)).sum();
    Some((data.into(), Rewrite { block, start, end, old_size }))
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
        module.data[b].len().cmp(&module.data[a].len()).then_with(|| a.cmp(&b))
    });

    let mut packed = IndexVec::<DataId, Bytes>::new();
    let mut packed_names = IndexVec::<DataId, Option<Symbol>>::new();
    let mut remap = FxHashMap::default();
    for old_id in referenced {
        let data = &module.data[old_id];
        let data_ref = if let Some(data_ref) = find_data(&packed, data) {
            if module.data_names[old_id].is_some() && packed_names[data_ref.id].is_none() {
                packed_names[data_ref.id] = Some(crate::data_literal_name(data_ref.id.index()));
            }
            data_ref
        } else {
            let id = packed.push(data.clone());
            let name = module.data_names[old_id].map(|_| crate::data_literal_name(id.index()));
            packed_names.push(name);
            DataRef::new(id, 0)
        };
        remap.insert(old_id, data_ref);
    }

    let changed = packed != module.data || packed_names != module.data_names;
    module.data = packed;
    module.data_names = packed_names;
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

fn find_data(data: &IndexVec<DataId, Bytes>, needle: &[u8]) -> Option<DataRef> {
    data.iter_enumerated().find_map(|(id, known)| {
        memmem::find(known, needle).map(|offset| DataRef::new(id, data_offset(offset)))
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

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}
