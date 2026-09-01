//! Materialize and pack program data.

use super::EvmPass;
use crate::backend::evm::{
    ir::{
        BlockId, Data, DataId, DataRef, Instruction, Module, PushValue,
        default_instruction_stack_effect, immediate_materialization_cost,
    },
    op::{self, WORD_BYTES, data_copy_cost, data_copy_gas, data_copy_is_profitable},
};
use alloy_primitives::{Bytes, U256};
use memchr::memmem;
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_interface::sym;
use solar_sema::Gcx;

/// Bounds quadratic arbitrary-substring pooling; exact interning remains unbounded.
const MAX_DATA_SUBSTRING_ENTRIES: usize = 1024;

/// Bounds rewrites whose local cost does not model lost global code sharing.
const MAX_SHARED_DATA_COPY_SITES: usize = 4;

pub(super) struct PackExistingData;

impl EvmPass for PackExistingData {
    fn name(&self) -> &'static str {
        "pack-existing-data"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let optimization = gcx.sess.opts.optimization;
        pack_existing_data(module, optimization.is_gas() || optimization.is_size())
    }
}

pub(super) struct PackData;

impl EvmPass for PackData {
    fn name(&self) -> &'static str {
        "pack-data"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let optimization = gcx.sess.opts.optimization;
        if !(optimization.is_gas() || optimization.is_size()) {
            return pack_existing_data(module, false);
        }
        let (references, groups) = data_references_and_runs(gcx, module);
        if references.layout_is_observable() {
            return false;
        }
        let packed = pack_data(module, &references, true);
        let materialized = materialize_data(gcx, module, groups);
        if materialized {
            let references = data_references(module);
            if !references.layout_is_observable() {
                pack_data(module, &references, true);
            }
        }
        packed || materialized
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

struct PoolEntry {
    id: DataId,
    bytes: Bytes,
}

struct DataPool {
    entries: Vec<PoolEntry>,
    exact: FxHashMap<Bytes, DataRef>,
}

type RewriteGroups = FxHashMap<Bytes, Vec<Rewrite>>;

enum Placement {
    Existing(DataRef),
    New,
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
            entries: data
                .iter_enumerated()
                .map(|(id, data)| PoolEntry { id, bytes: data.bytes.clone() })
                .collect(),
            exact: data
                .iter_enumerated()
                .map(|(id, data)| (data.bytes.clone(), DataRef::new(id, 0)))
                .collect(),
        }
    }

    fn placement(&self, data: &[u8]) -> Placement {
        if let Some(&data) = self.exact.get(data) {
            return Placement::Existing(data);
        }
        if self.entries.len() >= MAX_DATA_SUBSTRING_ENTRIES {
            return Placement::New;
        }
        for entry in &self.entries {
            if let Some(offset) = memmem::find(&entry.bytes, data) {
                return Placement::Existing(DataRef::new(entry.id, data_offset(offset)));
            }
        }
        Placement::New
    }

    fn intern(&mut self, module: &mut Module, bytes: Bytes, placement: Placement) -> DataRef {
        if let Placement::Existing(data) = placement {
            return data;
        }
        let id = module.data.push(Data { bytes: bytes.clone(), name: Some(sym::literal) });
        self.entries.push(PoolEntry { id, bytes: bytes.clone() });
        self.exact.insert(bytes, DataRef::new(id, 0));
        DataRef::new(id, 0)
    }
}

fn materialize_data(gcx: Gcx<'_>, module: &mut Module, groups: RewriteGroups) -> bool {
    if groups.is_empty() {
        return false;
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_unstable_by(|(a, _), (b, _)| {
        a.len().cmp(&b.len()).then_with(|| a.as_ref().cmp(b.as_ref()))
    });

    let mut pool = DataPool::new(&module.data);
    let mut prepared = Vec::new();
    let mut rejected = Vec::<(Bytes, Vec<Rewrite>)>::new();
    for (data, rewrites) in groups {
        if rewrites.len() > MAX_SHARED_DATA_COPY_SITES {
            continue;
        }
        let placement = pool.placement(&data);
        let additional_bytes = placement_additional_bytes(&placement, data.len());
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
    if dup.as_evm_opcode() != Some(op::DUP2) || store.as_evm_opcode() != Some(op::MSTORE) {
        return None;
    }

    let mut end = start + 3;
    let mut words = 1usize;
    while let Some(window) = instructions.get(end..end + 6) {
        let [offset, dup, add, value, swap, store] = window else { unreachable!() };
        if offset.concrete_immediate() != Some(U256::from(words * WORD_BYTES))
            || dup.as_evm_opcode() != Some(op::DUP2)
            || add.as_evm_opcode() != Some(op::ADD)
            || swap.as_evm_opcode() != Some(op::SWAP1)
            || store.as_evm_opcode() != Some(op::MSTORE)
        {
            break;
        }
        if value.concrete_immediate().is_none() {
            break;
        }
        words += 1;
        end += 6;
    }
    let mut data = Vec::with_capacity(words * WORD_BYTES);
    data.extend_from_slice(&first.to_be_bytes::<WORD_BYTES>());
    for window in instructions[start + 3..end].as_chunks::<6>().0 {
        data.extend_from_slice(
            &window[3].concrete_immediate().unwrap().to_be_bytes::<WORD_BYTES>(),
        );
    }
    let instructions = &instructions[start..end];
    if !instructions.iter().all(Instruction::has_canonical_stack_effect) {
        return None;
    }
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
    // Static sites have no execution-frequency estimate, so never buy gas by
    // growing code for a copy that may stay cold.
    data_copy_is_profitable(gcx.sess.opts.optimization, improvement.runtime_gas, improvement.bytes)
}

fn pack_existing_data(module: &mut Module, allow_subslices: bool) -> bool {
    if module.data.is_empty() {
        return false;
    }
    let references = data_references(module);
    if references.layout_is_observable() {
        return false;
    }
    pack_data(module, &references, allow_subslices)
}

fn placement_additional_bytes(placement: &Placement, size: usize) -> isize {
    match placement {
        Placement::Existing(_) => 0,
        Placement::New => size as isize,
    }
}

fn pack_data(module: &mut Module, references: &DataReferences, allow_subslices: bool) -> bool {
    if module.data.is_empty() {
        return false;
    }
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
        } else if let Some(data_ref) = (allow_subslices
            && references.subslice_safe[old_id]
            && module.data.len() < MAX_DATA_SUBSTRING_ENTRIES)
            .then(|| find_data(&packed, &sources, &data.bytes, old_id))
            .flatten()
        {
            data_ref
        } else {
            let id = packed.push(Data { bytes: data.bytes.clone(), name: data.name });
            sources.push(old_id);
            exact.insert(data.bytes.clone(), id);
            DataRef::new(id, 0)
        };
        if packed[data_ref.id].name.is_none() {
            packed[data_ref.id].name = data.name;
        }
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
) -> Option<DataRef> {
    data.iter_enumerated().find_map(|(id, known)| {
        if sources[id] < needle_id {
            memmem::find(&known.bytes, needle).map(|offset| DataRef::new(id, data_offset(offset)))
        } else {
            None
        }
    })
}

#[derive(Clone, Copy)]
enum DataStackValue {
    Unknown,
    Immediate(usize),
    Data(DataRef),
}

struct DataReferences {
    counts: IndexVec<DataId, usize>,
    subslice_safe: IndexVec<DataId, bool>,
    layout_observable: bool,
}

impl DataReferences {
    fn new(module: &Module) -> Self {
        Self {
            counts: IndexVec::from_vec(vec![0; module.data.len()]),
            subslice_safe: IndexVec::from_vec(vec![true; module.data.len()]),
            layout_observable: false,
        }
    }

    fn layout_is_observable(&self) -> bool {
        self.layout_observable
            || self
                .counts
                .iter()
                .zip(&self.subslice_safe)
                .any(|(&count, &subslice_safe)| count != 0 && !subslice_safe)
    }
}

fn data_references(module: &Module) -> DataReferences {
    scan_data_references(module, |_, _, _| {})
}

pub(crate) fn data_layout_is_observable(module: &Module) -> bool {
    data_references(module).layout_is_observable()
}

fn data_references_and_runs(gcx: Gcx<'_>, module: &Module) -> (DataReferences, RewriteGroups) {
    let mut groups = RewriteGroups::default();
    let mut next_run_start = 0;
    let references = scan_data_references(module, |block_id, index, instructions| {
        if index == 0 {
            next_run_start = 0;
        }
        if index >= next_run_start
            && let Some((data, rewrite)) = find_run(gcx, block_id, instructions, index)
        {
            next_run_start = rewrite.end;
            let rewrites = groups.entry(data).or_default();
            if rewrites.len() <= MAX_SHARED_DATA_COPY_SITES {
                rewrites.push(rewrite);
            }
        }
    });
    (references, groups)
}

fn scan_data_references(
    module: &Module,
    mut visit: impl FnMut(BlockId, usize, &[Instruction]),
) -> DataReferences {
    let mut references = DataReferences::new(module);
    let mut stack = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for (index, inst) in block.instructions.iter().enumerate() {
            visit(block_id, index, &block.instructions);
            track_data_reference(module, inst, &mut stack, &mut references);
        }
        mark_stack_data_unsafe(&stack, &mut references.subslice_safe);
        stack.clear();
    }
    references
}

fn track_data_reference(
    module: &Module,
    inst: &Instruction,
    stack: &mut Vec<DataStackValue>,
    references: &mut DataReferences,
) {
    if inst.opcode == op::CODESIZE {
        references.layout_observable = true;
    }
    if let Some(data) = inst.pushed_data() {
        references.counts[data.id] += 1;
        stack.push(DataStackValue::Data(data));
        return;
    }
    if let Some(value) = inst.concrete_immediate() {
        stack.push(
            usize::try_from(value).map_or(DataStackValue::Unknown, DataStackValue::Immediate),
        );
        return;
    }
    if inst.is_encoded_push() {
        stack.push(DataStackValue::Unknown);
        return;
    }
    if let Some(stack_op) = inst.as_stack_op() {
        match stack_op {
            op::StackOp::Dup(depth) => {
                let depth = usize::from(depth);
                let value = stack
                    .len()
                    .checked_sub(depth)
                    .map_or(DataStackValue::Unknown, |index| stack[index]);
                stack.push(value);
            }
            op::StackOp::Swap(depth) => {
                let depth = usize::from(depth);
                ensure_stack_depth(stack, depth + 1);
                let top = stack.len() - 1;
                stack.swap(top, top - depth);
            }
            op::StackOp::Exchange(first, second) => {
                let first = usize::from(first);
                let second = usize::from(second);
                ensure_stack_depth(stack, second + 1);
                let top = stack.len() - 1;
                stack.swap(top - first, top - second);
            }
            op::StackOp::Pop => {
                ensure_stack_depth(stack, 1);
                stack.pop();
            }
        }
        return;
    }

    // A raw branch has a physical successor that this local scan cannot follow.
    // A data address that survives it may be used there.
    if inst.has_raw_branch_target() {
        mark_stack_data_unsafe(stack, &mut references.subslice_safe);
    }

    let Some(effect) = inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))
    else {
        mark_stack_data_unsafe(stack, &mut references.subslice_safe);
        stack.clear();
        return;
    };
    let inputs = usize::from(effect.inputs);
    ensure_stack_depth(stack, inputs);
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
    stack.extend(std::iter::repeat_n(DataStackValue::Unknown, usize::from(effect.outputs)));
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

fn data_copy_is_bounded(module: &Module, data: DataRef, size: usize) -> bool {
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
    data_copy_cost(gcx.sess.opts.evm_version, size).0
}

fn static_gas(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    inst.concrete_immediate()
        .map_or(3, |value| immediate_materialization_cost(gcx.sess.opts.evm_version, value).1)
}
