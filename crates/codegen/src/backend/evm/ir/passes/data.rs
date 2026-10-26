//! Materialize and pack program data.
//!
//! `pack-existing-data` interns equal data objects and pools contained byte strings without
//! changing code. `pack-data` also replaces runs of constant memory stores with one copy each. A
//! run is found by symbolically executing pushes, stack operations, constant expressions (folded
//! with the opcode table's word semantics), and constant `MSTORE` and `MSTORE8`s to one base: a
//! stack word plus constant offsets, or absolute addresses. Stores may come in any order and
//! overlap, but must fill one contiguous range, since a copy would overwrite any gap. The copy
//! takes the window's place and rebuilds the stack the window left: it pops consumed inputs or
//! pushes the constants and address words the window produced. Among starts that write the same
//! bytes to the same end, the cheapest wins. Zero runs copy from past the end of calldata and need
//! no data; other runs copy from the data pool. The pass prices each run with the `Target` cost
//! model and groups equal byte strings so every site of a group shares one entry, accepting a group
//! when its total saving improves the objective; under `-Ogas` that means less runtime gas and no
//! more bytes, since static sites have no execution frequency to repay code growth. Creation code
//! that runs once may trade gas for bytes at the calldata price of its model, but blocks that may
//! run more than once keep the runtime model. Rejected groups can still ride along as substrings of
//! an accepted entry.
//!
//! Both passes leave a module alone when `CODESIZE` can observe the changed data layout, or when
//! a data address may reach anything but a `CODECOPY` that stays inside its entry. The scan tracks
//! each block's stack and follows addresses through a few unconditional jumps, since shared tails
//! can move a copy into another block. Pooling uses bounded substring search to avoid quadratic
//! compile time on large data sets. Library relocations must match within a shared range; literal
//! stores never share relocatable bytes.
//!
//! Runs end at the first other instruction, so a computed word between literal stores splits
//! them, and runs do not cross block boundaries; `cfg-simplify` merges straight-line chains first.
//!
//! Deferred contract bytecode stays opaque in the pipeline: each deferred entry keeps its own
//! storage, and a `push_data_size` counts as a reference that bounds a copy of its entry's exact
//! length. Final assembly links the bytes in, folds the sizes into literals, and runs
//! `pack-existing-data` once more so linked bytecode can share storage, such as runtime code
//! inside the creation code that contains it.

use super::{EvmPass, utils::is_split_point};
use crate::{
    backend::evm::{
        ir::{
            BlockId, Data, DataId, DataRef, DataSize, Instruction, Metadata, Module, PushValue,
            Terminator, TerminatorKind,
        },
        op::{self, WORD_BYTES},
    },
    link::LibraryRelocation,
    target::{Cost, Target},
};
use alloy_primitives::{Bytes, U256};
use memchr::memmem;
use smallvec::SmallVec;
use solar_data_structures::{index::IndexVec, map::FxHashMap};
use solar_interface::sym;
use solar_sema::Gcx;

/// Bounds quadratic arbitrary-substring pooling; exact interning remains unbounded.
const MAX_DATA_SUBSTRING_ENTRIES: usize = 1024;

/// Bounds the instructions one literal-store run may span, keeping each scan start linear.
const MAX_RUN_INSTRUCTIONS: usize = 256;

/// Bounds the unconditional jumps followed from a block to find where its data addresses are used.
const MAX_DATA_JUMP_HOPS: usize = 4;

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

/// A literal-store run that one copy can replace.
struct Rewrite {
    block: BlockId,
    start: usize,
    end: usize,
    /// Byte length of the copy.
    size: usize,
    /// Cost of the replaced instructions.
    old: Cost,
    /// Cost of the replacement, without the bytes it may add to the data pool.
    new: Cost,
    /// The replacement after its size and source pushes: the destination, the copy, and the
    /// stack the replaced instructions left.
    tail: Vec<Instruction>,
    /// Whether the block may run more than once, which prices the run as runtime code.
    repeats: bool,
}

impl Rewrite {
    fn saving(&self, target: &Target) -> Improvement {
        Improvement {
            runtime_gas: i64::from(target.runtime_gas(self.old))
                - i64::from(target.runtime_gas(self.new)),
            bytes: i64::from(self.old.bytes) - i64::from(self.new.bytes),
        }
    }

    /// Whether this saves at least as much as `other` under the objective, compared without
    /// signed costs: `other.old + self.new <= self.old + other.new`.
    fn saves_at_least(&self, other: &Self, target: &Target) -> bool {
        target.cmp(other.old + self.new, self.old + other.new).is_le()
    }
}

struct PreparedRewrite {
    rewrite: Rewrite,
    /// The copied program data, or `None` for a zero fill from past the end of calldata.
    data: Option<DataRef>,
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
    runtime_gas: i64,
    bytes: i64,
}

impl Improvement {
    fn add(&mut self, other: Self) {
        self.runtime_gas += other.runtime_gas;
        self.bytes += other.bytes;
    }
}

impl DataPool {
    fn new(data: &IndexVec<DataId, Data>) -> Self {
        // Relocated and deferred bytes are not final, so constants never share them.
        let literals = || {
            data.iter_enumerated().filter_map(|(id, data)| {
                data.library_relocations.is_empty().then_some(())?;
                Some((id, data.bytes.known()?.clone()))
            })
        };
        Self {
            entries: literals().map(|(id, bytes)| PoolEntry { id, bytes }).collect(),
            exact: literals().map(|(id, bytes)| (bytes, DataRef::new(id, 0))).collect(),
        }
    }

    fn placement(&self, data: &Bytes) -> Placement {
        if let Some(&data) = self.exact.get(data) {
            return Placement::Existing(data);
        }
        if self.entries.len() >= MAX_DATA_SUBSTRING_ENTRIES {
            return Placement::New;
        }
        let mut finder = NeedleFinder::new(data.clone());
        for entry in &self.entries {
            if let Some(offset) = finder.find(&entry.bytes) {
                return Placement::Existing(DataRef::new(entry.id, data_offset(offset)));
            }
        }
        Placement::New
    }

    fn intern(&mut self, module: &mut Module, bytes: Bytes, placement: Placement) -> DataRef {
        if let Placement::Existing(data) = placement {
            return data;
        }
        let id = module.data.push(Data::new(bytes.clone(), Some(sym::literal)));
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

    let creation = module.target(gcx);
    let runtime = Target::new(gcx);
    let target_of = |rewrites: &[Rewrite]| {
        if rewrites.iter().any(|rewrite| rewrite.repeats) { runtime } else { creation }
    };
    let mut pool = DataPool::new(&module.data);
    let mut prepared = Vec::new();
    let mut rejected = Vec::<(NeedleFinder, Vec<Rewrite>)>::new();
    for (data, rewrites) in groups {
        if data.iter().all(|&byte| byte == 0) {
            // Zero fills need no data, so each site stands alone.
            prepared.extend(
                rewrites
                    .into_iter()
                    .filter(|rewrite| {
                        is_profitable(
                            &target_of(std::slice::from_ref(rewrite)),
                            rewrite.saving(&runtime),
                        )
                    })
                    .map(|rewrite| PreparedRewrite { rewrite, data: None }),
            );
            continue;
        }
        let placement = pool.placement(&data);
        let additional_bytes = placement_additional_bytes(&placement, data.len());
        let mut improvement = rewrite_improvement(&rewrites, additional_bytes, &runtime);
        let mut repeats = rewrites.iter().any(|rewrite| rewrite.repeats);
        let mut absorbed = Vec::new();
        for (index, (contained, contained_rewrites)) in
            rejected.iter_mut().take(MAX_DATA_SUBSTRING_ENTRIES).enumerate()
        {
            let Some(offset) = contained.find(&data) else {
                continue;
            };
            let contained_improvement = rewrite_improvement(contained_rewrites, 0, &runtime);
            if is_profitable(&target_of(contained_rewrites), contained_improvement) {
                improvement.add(contained_improvement);
                repeats |= contained_rewrites.iter().any(|rewrite| rewrite.repeats);
                absorbed.push((index, offset));
            }
        }
        if !is_profitable(&if repeats { runtime } else { creation }, improvement) {
            rejected.push((NeedleFinder::new(data), rewrites));
            continue;
        }

        let data_ref = pool.intern(module, data, placement);
        prepare_rewrites(&mut prepared, rewrites, data_ref);
        for (index, offset) in absorbed.into_iter().rev() {
            let (_, rewrites) = rejected.swap_remove(index);
            let offset =
                data_ref.offset.checked_add(data_offset(offset)).expect("data offset overflow");
            prepare_rewrites(&mut prepared, rewrites, DataRef::new(data_ref.id, offset));
        }
    }
    prepared.sort_unstable_by_key(|prepared| (prepared.rewrite.block, prepared.rewrite.start));
    let changed = !prepared.is_empty();
    for PreparedRewrite { rewrite, data } in prepared.into_iter().rev() {
        let instructions = &mut module.blocks[rewrite.block].instructions;
        // The copy carries every origin of the stores it replaces; their function events land
        // on the copy itself, the last replacement.
        let mut metadata = Metadata::default();
        for inst in &instructions[rewrite.start..rewrite.end] {
            metadata.absorb_debug_info(&inst.metadata);
        }
        // push size
        // push data | calldatasize
        // <tail>
        let mut replacement = [
            Instruction::push_value(U256::from(rewrite.size)),
            data.map_or_else(|| Instruction::opcode(op::CALLDATASIZE), Instruction::push_data),
        ]
        .into_iter()
        .chain(rewrite.tail)
        .map(|inst| inst.with_source_debug(&metadata))
        .collect::<Vec<_>>();
        let copy = replacement
            .iter()
            .rposition(|inst| matches!(inst.opcode, op::CODECOPY | op::CALLDATACOPY))
            .unwrap();
        replacement[copy].metadata.absorb_debug_info(&metadata);
        instructions.splice(rewrite.start..rewrite.end, replacement);
    }
    changed
}

fn prepare_rewrites(prepared: &mut Vec<PreparedRewrite>, rewrites: Vec<Rewrite>, data: DataRef) {
    prepared
        .extend(rewrites.into_iter().map(|rewrite| PreparedRewrite { rewrite, data: Some(data) }));
}

/// Finds the literal-store run that starts at `start`. A later start that writes the same bytes
/// and ends at the same place wins when it saves at least as much: leading instructions that the
/// copy would only rebuild add nothing.
fn find_run(
    target: &Target,
    block: BlockId,
    instructions: &[Instruction],
    start: usize,
) -> Option<(Bytes, Rewrite)> {
    let mut best = scan_run(target, block, instructions, start)?;
    for later in start + 1..best.first_store {
        if let Some(run) = scan_run(target, block, instructions, later)
            && run.rewrite.end == best.rewrite.end
            && run.data == best.data
            && run.rewrite.saves_at_least(&best.rewrite, target)
        {
            best = run;
        }
    }
    Some((best.data, best.rewrite))
}

/// A word on the symbolic stack of a literal-store run: input slot `slot` of the run, counted
/// from the top of the stack the run starts with, plus `offset`, or the constant `offset`.
#[derive(Clone, Copy, PartialEq, Eq)]
struct RunWord {
    slot: Option<usize>,
    offset: U256,
}

impl RunWord {
    const fn constant(value: U256) -> Self {
        Self { slot: None, offset: value }
    }

    const fn input(slot: usize) -> Self {
        Self { slot: Some(slot), offset: U256::ZERO }
    }
}

/// Stores of constants to one base address that a single copy can replace.
struct Run {
    rewrite: Rewrite,
    data: Bytes,
    first_store: usize,
}

/// The state of a run scan after a store; the stack words above `kept` are saved separately.
struct RunEnd {
    end: usize,
    old: Cost,
    stores: usize,
    range: (u64, u64),
    inputs: usize,
    kept: usize,
}

/// Symbolically runs the pushes, stack operations, constant expressions, and constant `MSTORE`
/// and `MSTORE8`s from `start`, and returns the longest prefix whose stores fill one contiguous
/// range and whose final stack a copy can rebuild.
fn scan_run(
    target: &Target,
    block: BlockId,
    instructions: &[Instruction],
    start: usize,
) -> Option<Run> {
    if !is_split_point(instructions, start) {
        return None;
    }
    let mut stack = Vec::<RunWord>::new();
    let mut inputs = 0;
    let mut old = Cost::ZERO;
    let mut base = None;
    let mut first_store = None;
    let mut stores = Vec::<(u64, U256, bool)>::new();
    let mut ranges = Vec::<(u64, u64)>::new();
    let mut best = None;
    let mut extras = Vec::new();
    let ensure = |stack: &mut Vec<RunWord>, inputs: &mut usize, depth: usize| {
        while stack.len() < depth {
            stack.insert(0, RunWord::input(*inputs));
            *inputs += 1;
        }
    };
    for (index, inst) in instructions.iter().enumerate().skip(start).take(MAX_RUN_INSTRUCTIONS) {
        if let Some(value) = inst.concrete_immediate() {
            stack.push(RunWord::constant(value));
            old += target.push(value);
            continue;
        }
        if inst.is_encoded_push() {
            break;
        }
        if let Some(stack_op) = inst.as_stack_op() {
            let Some(cost) = target.stack_op(stack_op) else { break };
            ensure(&mut stack, &mut inputs, stack_op.required_depth());
            let top = stack.len() - 1;
            match stack_op {
                op::StackOp::Dup(depth) => stack.push(stack[top + 1 - usize::from(depth)]),
                op::StackOp::Swap(depth) => stack.swap(top, top - usize::from(depth)),
                op::StackOp::Exchange(first, second) => {
                    stack.swap(top - usize::from(first), top - usize::from(second))
                }
                op::StackOp::Pop => {
                    stack.pop();
                }
            }
            old += cost;
            continue;
        }
        let opcode = inst.opcode;
        if matches!(opcode, op::MSTORE | op::MSTORE8) {
            ensure(&mut stack, &mut inputs, 2);
            let address = stack.pop().unwrap();
            let value = stack.pop().unwrap();
            let width = if opcode == op::MSTORE { WORD_BYTES as u64 } else { 1 };
            let Ok(offset) = u64::try_from(address.offset) else { break };
            if value.slot.is_some()
                || *base.get_or_insert(address.slot) != address.slot
                || offset.checked_add(width).is_none()
            {
                break;
            }
            first_store.get_or_insert(index);
            stores.push((offset, value.offset, opcode == op::MSTORE));
            insert_range(&mut ranges, (offset, offset + width));
            old += target.opcode(opcode);
            let end = index + 1;
            let kept = stack
                .iter()
                .zip((0..inputs).rev())
                .take_while(|&(&word, slot)| word == RunWord::input(slot))
                .count();
            if let [range] = ranges[..]
                && is_split_point(instructions, end)
                && (kept == inputs || kept == stack.len())
            {
                extras.clear();
                extras.extend_from_slice(&stack[kept..]);
                best = Some(RunEnd { end, old, stores: stores.len(), range, inputs, kept });
            }
            continue;
        }
        let Some((input_count, 1)) = op::stack_io(opcode) else { break };
        let input_count = usize::from(input_count);
        ensure(&mut stack, &mut inputs, input_count);
        let operands = stack.split_off(stack.len() - input_count);
        let arguments = operands.iter().rev().map(|word| word.offset).collect::<SmallVec<[_; 3]>>();
        let result = match operands[..] {
            [a, b] if opcode == op::ADD && a.slot.xor(b.slot).is_some() => {
                RunWord { slot: a.slot.or(b.slot), offset: a.offset.wrapping_add(b.offset) }
            }
            _ if operands.iter().all(|word| word.slot.is_none()) => {
                let Some(value) = op::eval(opcode, &arguments) else { break };
                RunWord::constant(value)
            }
            _ => break,
        };
        let arguments = arguments.into_iter().map(Some).collect::<SmallVec<[_; 3]>>();
        old += target.opcode_with_immediates(opcode, &arguments);
        stack.push(result);
    }

    let RunEnd { end, old, stores: store_count, range: (low, high), inputs, kept } = best?;
    let mut data = vec![0; (high - low) as usize];
    for &(offset, value, word) in &stores[..store_count] {
        let at = (offset - low) as usize;
        if word {
            data[at..at + WORD_BYTES].copy_from_slice(&value.to_be_bytes::<WORD_BYTES>());
        } else {
            data[at] = value.byte(0);
        }
    }
    // The deepest replacement `DUP` reaches the base below the size and source pushes, or an input
    // below every rebuilt word.
    if inputs + extras.len() + 4 > usize::from(u8::MAX) {
        return None;
    }
    let zero = data.iter().all(|&byte| byte == 0);
    let destination = RunWord { slot: base.flatten(), offset: U256::from(low) };
    let tail = copy_tail(destination, zero, inputs, kept, &extras);
    // Reserve PUSH3 for an unresolved data address.
    let source = if zero { target.opcode(op::CALLDATASIZE) } else { target.opcode(op::PUSH3) };
    let mut new = target.push(U256::from(data.len())) + source + target.copied_words(data.len());
    for inst in &tail {
        new += instruction_cost(target, inst)?;
    }
    let rewrite = Rewrite { block, start, end, size: data.len(), old, new, tail, repeats: false };
    Some(Run { rewrite, data: data.into(), first_store: first_store? })
}

/// Adds `range` to the sorted disjoint `ranges`, merging touching ranges.
fn insert_range(ranges: &mut Vec<(u64, u64)>, (mut low, mut high): (u64, u64)) {
    ranges.retain(|&(start, end)| {
        let touches = start <= high && low <= end;
        if touches {
            low = low.min(start);
            high = high.max(end);
        }
        !touches
    });
    let index = ranges.partition_point(|&(start, _)| start < low);
    ranges.insert(index, (low, high));
}

/// The instructions after a copy's size and source pushes: its destination, the copy, and the
/// rebuilt stack, where the run consumed the inputs above `kept` and pushed `extras`.
fn copy_tail(
    destination: RunWord,
    zero: bool,
    inputs: usize,
    kept: usize,
    extras: &[RunWord],
) -> Vec<Instruction> {
    let dup = |depth: usize| Instruction::stack_op(op::StackOp::Dup(depth as u8));
    let mut tail = Vec::new();
    // dup base | push offset; dup base; add | push address
    match destination {
        RunWord { slot: Some(slot), offset } if offset.is_zero() => tail.push(dup(slot + 3)),
        RunWord { slot: Some(slot), offset } => {
            tail.extend([Instruction::push_value(offset), dup(slot + 4)]);
            tail.push(Instruction::opcode(op::ADD));
        }
        RunWord { slot: None, offset } => tail.push(Instruction::push_value(offset)),
    }
    // codecopy | calldatacopy
    tail.push(Instruction::opcode(if zero { op::CALLDATACOPY } else { op::CODECOPY }));
    // pop
    tail.extend((kept..inputs).map(|_| Instruction::stack_op(op::StackOp::Pop)));
    for (index, extra) in extras.iter().enumerate() {
        // push value | dup input [push offset; add]
        match extra.slot {
            None => tail.push(Instruction::push_value(extra.offset)),
            Some(slot) => {
                tail.push(dup(slot + 1 + index));
                if !extra.offset.is_zero() {
                    tail.extend([
                        Instruction::push_value(extra.offset),
                        Instruction::opcode(op::ADD),
                    ]);
                }
            }
        }
    }
    tail
}

/// The cost of an instruction in a copy replacement, or `None` when the target lacks it.
fn instruction_cost(target: &Target, inst: &Instruction) -> Option<Cost> {
    if let Some(value) = inst.concrete_immediate() {
        return Some(target.push(value));
    }
    match inst.as_stack_op() {
        Some(stack_op) => target.stack_op(stack_op),
        None => Some(target.opcode(inst.opcode)),
    }
}

fn rewrite_improvement(
    rewrites: &[Rewrite],
    additional_bytes: isize,
    target: &Target,
) -> Improvement {
    let mut improvement = Improvement { runtime_gas: 0, bytes: -(additional_bytes as i64) };
    for rewrite in rewrites {
        improvement.add(rewrite.saving(target));
    }
    improvement
}

fn is_profitable(target: &Target, improvement: Improvement) -> bool {
    // Static sites have no execution-frequency estimate, so never buy gas by
    // growing code for a copy that may stay cold.
    target.improves(improvement.runtime_gas, improvement.bytes)
}

/// Shares storage between data entries after deferred data is linked in.
pub(in crate::backend) fn pack_linked_data(gcx: Gcx<'_>, module: &mut Module) -> bool {
    PackExistingData.run_pass(gcx, module)
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
        .filter_map(|(id, &count)| (count != 0 || module.data[id].emit_in_runtime).then_some(id))
        .collect::<Vec<_>>();
    if referenced.is_empty() {
        module.data.clear();
        return true;
    }
    if module.data.len() == 1 {
        return false;
    }
    referenced.sort_unstable_by(|&a, &b| {
        let len = |id: DataId| module.data[id].bytes.known().map_or(0, |bytes| bytes.len());
        len(b).cmp(&len(a)).then_with(|| a.cmp(&b))
    });

    let mut packed = IndexVec::<DataId, Data>::new();
    let mut sources = IndexVec::<DataId, DataId>::new();
    let mut exact = FxHashMap::<(Bytes, Vec<LibraryRelocation>), DataId>::default();
    let mut remap = FxHashMap::default();
    for old_id in referenced {
        let data = &module.data[old_id];
        // Deferred bytes are not known yet, so they are never shared.
        let key = (data.bytes.known())
            .filter(|_| !data.emit_in_runtime)
            .map(|bytes| (bytes.clone(), data.library_relocations.clone()));
        let data_ref = if let Some(key) = &key
            && let Some(&id) = exact.get(key)
        {
            DataRef::new(id, 0)
        } else if key.is_some()
            && let Some(data_ref) = (allow_subslices
                && references.subslice_safe[old_id]
                && module.data.len() < MAX_DATA_SUBSTRING_ENTRIES)
                .then(|| find_data(&packed, &sources, data, old_id))
                .flatten()
        {
            data_ref
        } else {
            let id = packed.push(data.clone());
            sources.push(old_id);
            if let Some(key) = key {
                exact.insert(key, id);
            }
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
            match &mut inst.value {
                Some(PushValue::Data(data)) => {
                    let base = remap[&data.id];
                    data.id = base.id;
                    data.offset =
                        data.offset.checked_add(base.offset).expect("data offset overflow");
                }
                // Deferred data keeps its own entry.
                Some(PushValue::DataSize(size)) => size.data = remap[&size.data].id,
                _ => {}
            }
        }
    }
    true
}

fn find_data(
    data: &IndexVec<DataId, Data>,
    sources: &IndexVec<DataId, DataId>,
    needle: &Data,
    needle_id: DataId,
) -> Option<DataRef> {
    let needle_bytes = needle.bytes.known()?;
    let mut finder = NeedleFinder::new(needle_bytes.clone());
    data.iter_enumerated().find_map(|(id, known)| {
        if sources[id] >= needle_id {
            return None;
        }
        let offset = finder.find(known.bytes.known()?)?;
        let end = offset + needle_bytes.len();
        let compatible = known
            .library_relocations
            .iter()
            .copied()
            .filter(|reloc| reloc.offset < end && reloc.offset + 20 > offset)
            .map(|reloc| (reloc.offset.checked_sub(offset), reloc.library))
            .eq(needle.library_relocations.iter().map(|reloc| (Some(reloc.offset), reloc.library)));
        compatible.then(|| DataRef::new(id, data_offset(offset)))
    })
}

#[derive(Clone, Copy)]
enum DataStackValue {
    Unknown,
    Immediate(usize),
    Data(DataRef),
    Size(DataSize),
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
    let mut last_store = None;
    let references = scan_data_references(module, |block_id, index, instructions| {
        if index == 0 {
            next_run_start = 0;
            // A run ends at a store, so no run starts after the last one.
            last_store = instructions
                .iter()
                .rposition(|inst| matches!(inst.opcode, op::MSTORE | op::MSTORE8));
        }
        if index >= next_run_start && last_store.is_some_and(|last| index <= last) {
            let block = &module.blocks[block_id];
            let target = block.target(gcx, module.code_follows);
            if let Some((data, mut rewrite)) = find_run(&target, block_id, instructions, index) {
                rewrite.repeats = !block.runs_once();
                next_run_start = rewrite.end;
                groups.entry(data).or_default().push(rewrite);
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
        // The stack starts empty, so without a data push no slot can hold a data address
        // or size.
        let has_data = block
            .instructions
            .iter()
            .any(|inst| inst.pushed_data().is_some() || inst.pushed_data_size().is_some());
        for (index, inst) in block.instructions.iter().enumerate() {
            visit(block_id, index, &block.instructions);
            if has_data {
                track_data_reference(module, inst, &mut stack, &mut references);
            } else if inst.opcode == op::CODESIZE {
                references.layout_observable = true;
            }
        }
        if has_data {
            // Shared tails can move a copy behind a jump, so follow data addresses that
            // survive an unconditional jump into its target with this path's stack.
            let mut terminator = &block.terminator;
            for _ in 0..MAX_DATA_JUMP_HOPS {
                let Some(Terminator { kind: TerminatorKind::Jump(target), .. }) = terminator else {
                    break;
                };
                if !stack.iter().any(|value| matches!(value, DataStackValue::Data(_))) {
                    break;
                }
                let target = &module.blocks[*target];
                for inst in &target.instructions {
                    track_data_reference(module, inst, &mut stack, &mut references);
                }
                terminator = &target.terminator;
            }
            mark_stack_data_unsafe(&stack, &mut references.subslice_safe);
            stack.clear();
        }
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
    if let Some(size) = inst.pushed_data_size() {
        references.counts[size.data] += 1;
        stack.push(DataStackValue::Size(size));
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

    let effect = inst.stack_effect();
    let inputs = usize::from(effect.inputs);
    ensure_stack_depth(stack, inputs);
    let first_input = stack.len() - inputs;
    for (index, value) in stack[first_input..].iter().rev().enumerate() {
        let DataStackValue::Data(data) = value else { continue };
        let bounded = inst.opcode == op::CODECOPY
            && index == 1
            && match stack.get(stack.len() - 3) {
                Some(DataStackValue::Immediate(size)) => data_copy_is_bounded(module, *data, *size),
                Some(DataStackValue::Size(size)) => size.is_length_of(*data),
                _ => false,
            };
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
        let end = (data.offset as usize).checked_add(size);
        end.zip(entry.bytes.known()).is_some_and(|(end, bytes)| end <= bytes.len())
    })
}

/// Finds one needle in several haystacks, building the long-haystack searcher that
/// [`memmem::find`] would build for each at most once.
struct NeedleFinder {
    needle: Bytes,
    finder: Option<memmem::Finder<'static>>,
}

impl NeedleFinder {
    fn new(needle: Bytes) -> Self {
        Self { needle, finder: None }
    }

    fn find(&mut self, haystack: &[u8]) -> Option<usize> {
        if haystack.len() < self.needle.len() {
            return None;
        }
        // `memmem::find` uses Rabin-Karp below this haystack length.
        if haystack.len() < 64 {
            return memmem::find(haystack, &self.needle);
        }
        self.finder
            .get_or_insert_with(|| memmem::Finder::new(&self.needle).into_owned())
            .find(haystack)
    }
}

fn data_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect("data offset exceeds `u32`")
}
