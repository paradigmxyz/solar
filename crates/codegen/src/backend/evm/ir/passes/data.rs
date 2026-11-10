//! Materialize and pack program data.
//!
//! `pack-data` interns equal data objects, pools contained byte strings, and replaces runs of
//! constant memory stores with one copy each. MIR keeps a constant byte-string object as a
//! `datacopy` when the shared `Target` price says a copy wins, and writes short data as stores;
//! this pass prices the runs of stores that remain against the same copy, where the final opcodes
//! are visible. A run is found by symbolically executing pushes,
//! stack operations, constant expressions (folded with the MIR evaluator), and constant `MSTORE`
//! and `MSTORE8`s to one base: a stack word plus constant offsets, or absolute addresses. Stores
//! may come in any order and overlap, but must fill one contiguous range, since a copy would
//! overwrite any gap. The copy takes the window's place and rebuilds the stack the window left: it
//! pops consumed inputs or pushes the constants and address words the window produced. Among starts
//! that write the same bytes to the same end, the cheapest wins. Zero runs copy from past the end
//! of calldata and need no data; other runs copy from the data pool. The pass prices each run with
//! the `Target` cost model and groups equal byte strings so every site of a group shares one entry,
//! accepting a group when its total saving improves the objective; under `-Ogas` that means less
//! runtime gas and no more bytes, since static sites have no execution frequency to repay code
//! growth. Creation code that runs once may trade gas for bytes at the calldata price of its model,
//! but blocks that may run more than once keep the runtime model. Rejected groups can still ride
//! along as substrings of an accepted entry. Whole runs are judged first, because equal runs at
//! several sites share one entry and so can pay jointly when no site pays alone. Only the runs that
//! no group accepts are split, since a short word between long strings only grows the data, and the
//! runs inside them are judged the same way. A copy of data that ends in zeros stops before them
//! when a zero word store clears the last word first, if that is cheaper.
//!
//! A copy needs the destination, source, and size on the stack at once, so it can peak above the
//! stores it replaces and past `MAX_STACK_DEPTH`. The input is valid, so every depth its path
//! reaches fits. A copy is safe when its peak, measured from the block's entry, is no higher than
//! the highest depth the old path reaches: the instructions before the run, and those after it up
//! to the first conditional jump or terminal, with the terminator's own growth. Otherwise the
//! entry bound from `Verifier::stack_entry_upper_bounds` must show that the peak fits. With
//! neither, the stores stay. The copy rebuilds the stack the run left, so the checks of different
//! runs in a block do not affect each other.
//!
//! The pass leaves a module alone when a data address may reach anything but a `CODECOPY` that
//! stays inside its entry. The last entry of creation code is the tail: the runtime code ends the
//! deployment code and constructor arguments follow it, which they find by subtracting its address
//! from `CODESIZE` or copying from it. The tail stays last and, when the arguments are found that
//! way, in place; new entries go before it, so `CODESIZE` alone does not stop the pass. The scan
//! tracks each block's stack and follows addresses through a few unconditional jumps, since shared
//! tails can move a copy into another block. Pooling uses bounded substring search to avoid
//! quadratic compile time on large data sets. Library relocations must match within a shared range;
//! literal stores never share relocatable bytes.
//!
//! Runs end at the first other instruction, so a computed word between literal stores splits
//! them, and runs do not cross block boundaries; `cfg-simplify` merges straight-line chains first.
//!
//! Deferred contract bytecode stays opaque in the pipeline: each deferred entry keeps its own
//! storage, and a `push_data_size` counts as a reference that bounds a copy of its entry's exact
//! length. Final assembly links the bytes in, folds the sizes into literals, and packs the
//! existing data once more, without store runs, so linked bytecode can share storage, such as
//! runtime code inside the creation code that contains it.

use super::{EvmPass, utils::is_split_point};
use crate::{
    backend::evm::{
        codegen::MAX_STACK_DEPTH,
        ir::{
            BlockId, BlockTarget, Data, DataId, DataRef, DataSize, Instruction, Metadata, Module,
            ModuleTarget, PushValue, Terminator, TerminatorKind, verify::Verifier,
        },
        op::{self, WORD_BYTES},
    },
    link::{LibraryRelocation, Trailer},
    mir::utils::eval,
    target::{Cost, Target},
};
use alloy_primitives::{Bytes, U256};
use memchr::memmem;
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxIndexMap},
};
use solar_interface::sym;
use solar_sema::Gcx;
use std::{cell::OnceCell, ops::Range};

/// Bounds quadratic arbitrary-substring pooling; exact interning remains unbounded.
const MAX_DATA_SUBSTRING_ENTRIES: usize = 1024;

/// Bounds the instructions one literal-store run may span, keeping each scan start linear.
const MAX_RUN_INSTRUCTIONS: usize = 256;

/// Bounds the unconditional jumps followed from a block to find where its data addresses are used.
const MAX_DATA_JUMP_HOPS: usize = 4;

pub(super) struct PackData;

impl EvmPass for PackData {
    fn name(&self) -> &'static str {
        "pack-data"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let Some(mut scan) = scan_data_references(module, None) else { return false };
        let limits =
            StackLimits { evm_version: gcx.sess.opts.evm_version, entries: OnceCell::new() };
        let groups = find_runs(gcx, module, &limits);
        let packed = pack_data(module, &mut scan);
        let materialized = materialize_data(gcx, module, groups, &limits);
        if materialized && let Some(mut scan) = scan_data_references(module, scan.tail) {
            pack_data(module, &mut scan);
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
    /// The instructions before the size and source pushes: a zero word store that clears the bytes
    /// the shortened copy leaves out.
    head: Vec<Instruction>,
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
}

#[derive(Clone, Copy)]
struct Improvement {
    runtime_gas: i64,
    bytes: i64,
}

impl Improvement {
    /// The total saving of `rewrites`.
    fn of(rewrites: &[Rewrite], target: &Target) -> Self {
        let mut total = Self { runtime_gas: 0, bytes: 0 };
        for rewrite in rewrites {
            let saving = rewrite.saving(target);
            total.runtime_gas += saving.runtime_gas;
            total.bytes += saving.bytes;
        }
        total
    }

    fn is_profitable(self, target: &Target) -> bool {
        // Static sites have no execution-frequency estimate, so never buy gas by
        // growing code for a copy that may stay cold.
        target.improves(self.runtime_gas, self.bytes)
    }
}

/// The literal program data that new copies can share.
struct DataPool {
    entries: Vec<(DataId, Bytes)>,
    exact: FxHashMap<Bytes, DataRef>,
}

impl DataPool {
    fn new(data: &IndexVec<DataId, Data>) -> Self {
        // Relocated and deferred bytes are not final, and verifiers replace trailers, so constants
        // never share them.
        let entries = data
            .iter_enumerated()
            .filter_map(|(id, data)| {
                let literal = data.library_relocations.is_empty() && data.trailers.is_empty();
                Some((id, data.bytes.known().filter(|_| literal)?.clone()))
            })
            .collect::<Vec<_>>();
        let exact =
            entries.iter().map(|(id, bytes)| (bytes.clone(), DataRef::new(*id, 0))).collect();
        Self { entries, exact }
    }

    /// Finds `data` in an existing entry.
    fn find(&self, data: &Bytes) -> Option<DataRef> {
        if let Some(&data) = self.exact.get(data) {
            return Some(data);
        }
        if self.entries.len() >= MAX_DATA_SUBSTRING_ENTRIES {
            return None;
        }
        let mut finder = NeedleFinder::new(data.clone());
        self.entries
            .iter()
            .find_map(|(id, bytes)| Some(DataRef::new(*id, data_offset(finder.find(bytes)?))))
    }

    fn push(&mut self, module: &mut Module, bytes: Bytes) -> DataRef {
        let id = module.data.push(Data::new(bytes.clone(), Some(sym::literal)));
        self.entries.push((id, bytes.clone()));
        self.exact.insert(bytes, DataRef::new(id, 0));
        DataRef::new(id, 0)
    }
}

fn materialize_data(
    gcx: Gcx<'_>,
    module: &mut Module,
    groups: FxHashMap<Bytes, Vec<Rewrite>>,
    limits: &StackLimits,
) -> bool {
    if groups.is_empty() {
        return false;
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    let sort_groups = |groups: &mut Vec<(Bytes, Vec<Rewrite>)>| {
        groups.sort_unstable_by(|(a, _), (b, _)| {
            a.len().cmp(&b.len()).then_with(|| a.as_ref().cmp(b.as_ref()))
        });
    };
    sort_groups(&mut groups);

    let creation = module.target(gcx);
    let runtime = module.runtime_target(gcx);
    let target_of = |repeats| if repeats { &runtime } else { &creation };
    let any_repeats = |rewrites: &[Rewrite]| rewrites.iter().any(|rewrite| rewrite.repeats);
    let mut pool = DataPool::new(&module.data);
    let mut prepared = Vec::<(Rewrite, Option<DataRef>)>::new();
    let mut rejected = Vec::<(NeedleFinder, Vec<Rewrite>)>::new();
    let mut absorbed = Vec::new();
    let mut scratch = RunScratch::default();
    for splits in [true, false] {
        for (data, rewrites) in std::mem::take(&mut groups) {
            if data.iter().all(|&byte| byte == 0) {
                // Zero fills need no data, so each site stands alone.
                prepared.extend(
                    rewrites
                        .into_iter()
                        .filter(|rewrite| {
                            rewrite.saving(&runtime).is_profitable(target_of(rewrite.repeats))
                        })
                        .map(|rewrite| (rewrite, None)),
                );
                continue;
            }
            let existing = pool.find(&data);
            let mut improvement = Improvement::of(&rewrites, &runtime);
            if existing.is_none() {
                improvement.bytes -= data.len() as i64;
            }
            let mut repeats = any_repeats(&rewrites);
            absorbed.clear();
            for (index, (contained, contained_rewrites)) in
                rejected.iter_mut().take(MAX_DATA_SUBSTRING_ENTRIES).enumerate()
            {
                let Some(offset) = contained.find(&data) else { continue };
                let saving = Improvement::of(contained_rewrites, &runtime);
                let contained_repeats = any_repeats(contained_rewrites);
                if saving.is_profitable(target_of(contained_repeats)) {
                    improvement.runtime_gas += saving.runtime_gas;
                    improvement.bytes += saving.bytes;
                    repeats |= contained_repeats;
                    absorbed.push((index, offset));
                }
            }
            if !improvement.is_profitable(target_of(repeats)) {
                rejected.push((NeedleFinder::new(data), rewrites));
                continue;
            }

            let data_ref = existing.unwrap_or_else(|| pool.push(module, data));
            prepared.extend(rewrites.into_iter().map(|rewrite| (rewrite, Some(data_ref))));
            for (index, offset) in absorbed.drain(..).rev() {
                let (_, rewrites) = rejected.swap_remove(index);
                let offset =
                    data_ref.offset.checked_add(data_offset(offset)).expect("data offset overflow");
                let data_ref = DataRef::new(data_ref.id, offset);
                prepared.extend(rewrites.into_iter().map(|rewrite| (rewrite, Some(data_ref))));
            }
        }
        // A run that no site profits from may hold shorter runs that do. They wait for the joint
        // evaluation of whole runs, which can accept a run that only pays when sites share it.
        if splits {
            let mut inner = FxHashMap::<Bytes, Vec<Rewrite>>::default();
            for rewrite in rejected.drain(..).flat_map(|(_, rewrites)| rewrites) {
                let block = &module.blocks[rewrite.block];
                let target = block.target(gcx, module.optimization, module.code_follows);
                let stack = BlockStack::new(module, limits, rewrite.block);
                for (data, mut window) in
                    split_run(&target, &block.instructions, &rewrite, &stack, &mut scratch)
                {
                    window.repeats = rewrite.repeats;
                    inner.entry(data).or_default().push(window);
                }
            }
            groups.extend(inner);
            sort_groups(&mut groups);
        }
    }
    prepared.sort_unstable_by_key(|(rewrite, _)| (rewrite.block, rewrite.start));
    let changed = !prepared.is_empty();
    for (rewrite, data) in prepared.into_iter().rev() {
        let instructions = &mut module.blocks[rewrite.block].instructions;
        // The copy carries every origin of the stores it replaces; their function events land
        // on the copy itself, the last replacement.
        // The first store seeds it, so that a store whose location was dropped or explained keeps
        // that kind instead of merging into the default kind of an empty seed.
        let mut metadata = Metadata::default();
        metadata.preserve_debug_loc(&instructions[rewrite.start].metadata);
        for inst in &instructions[rewrite.start..rewrite.end] {
            metadata.combine_debug_locs([&inst.metadata]);
            metadata.move_function_events(&inst.metadata);
        }
        // <head>
        // push size
        // push data | calldatasize
        // <tail>
        let mut replacement = rewrite
            .head
            .into_iter()
            .chain([
                Instruction::push_value(U256::from(rewrite.size)),
                data.map_or_else(|| Instruction::opcode(op::CALLDATASIZE), Instruction::push_data),
            ])
            .chain(rewrite.tail)
            .map(|inst| inst.with_debug_loc(&metadata))
            .collect::<Vec<_>>();
        let copy = replacement
            .iter()
            .rposition(|inst| matches!(inst.opcode, op::CODECOPY | op::CALLDATACOPY))
            .unwrap();
        replacement[copy].metadata.combine_debug_locs([&metadata]);
        replacement[copy].metadata.move_function_events(&metadata);
        instructions.splice(rewrite.start..rewrite.end, replacement);
    }
    changed
}

/// Finds the literal-store runs of every block, grouped by the bytes they write.
fn find_runs(
    gcx: Gcx<'_>,
    module: &Module,
    limits: &StackLimits,
) -> FxHashMap<Bytes, Vec<Rewrite>> {
    let mut groups = FxHashMap::<_, Vec<_>>::default();
    let mut scratch = RunScratch::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        // A run ends at a store, so no run starts after the last one.
        let Some(last_store) = block
            .instructions
            .iter()
            .rposition(|inst| matches!(inst.opcode, op::MSTORE | op::MSTORE8))
        else {
            continue;
        };
        let target = block.target(gcx, module.optimization, module.code_follows);
        let stack = BlockStack::new(module, limits, block_id);
        let mut start = 0;
        while start <= last_store {
            let Some((data, mut rewrite)) =
                find_run(&target, &stack, &block.instructions, start, &mut scratch)
            else {
                start += 1;
                continue;
            };
            rewrite.repeats = !block.runs_once();
            start = rewrite.end;
            groups.entry(data).or_default().push(rewrite);
        }
    }
    groups
}

/// Finds the literal-store run that starts at `start`. A later start that writes the same bytes
/// and ends at the same place wins when it saves at least as much: leading instructions that the
/// copy would only rebuild add nothing.
fn find_run(
    target: &Target,
    stack: &BlockStack<'_>,
    instructions: &[Instruction],
    start: usize,
    scratch: &mut RunScratch,
) -> Option<(Bytes, Rewrite)> {
    let mut best = scan_run(target, stack, instructions, start, scratch)?;
    for later in start + 1..best.first_store {
        if let Some(run) = scan_run(target, stack, instructions, later, scratch)
            && run.rewrite.end == best.rewrite.end
            && run.data == best.data
            && run.saves_at_least(&best, target)
        {
            best = run;
        }
    }
    Some((best.data, best.rewrite))
}

/// Calls `visit` with every run inside `parent`, `parent` itself included, that starts at a store's
/// first operand and ends at a store. The pass prices them all, so a run of `n` stores has about
/// `n^2 / 2` windows, each bounded by `MAX_RUN_INSTRUCTIONS`.
fn for_each_window(
    target: &Target,
    instructions: &[Instruction],
    parent: &Rewrite,
    stack: &BlockStack<'_>,
    scratch: &mut RunScratch,
    mut visit: impl FnMut(Run),
) {
    for start in parent.start..parent.end {
        // A window that starts in the middle of a store's operands only repeats the work of the
        // window that starts at the operands' first push.
        if start != parent.start
            && !matches!(instructions[start - 1].opcode, op::MSTORE | op::MSTORE8)
        {
            continue;
        }
        scan_ends(target, instructions, start, scratch);
        for index in 0..scratch.ends.len() {
            if scratch.ends[index].end <= parent.end
                && let Some(window) = run_ending_at(target, stack, scratch, index)
            {
                visit(window);
            }
        }
    }
}

/// Finds the non-overlapping runs inside `run`, other than `run` itself, that pay for their own
/// data, taking the one that saves the most first.
fn split_run(
    target: &Target,
    instructions: &[Instruction],
    run: &Rewrite,
    stack: &BlockStack<'_>,
    scratch: &mut RunScratch,
) -> Vec<(Bytes, Rewrite)> {
    let mut windows = Vec::<Run>::new();
    let mut best = None::<Run>;
    for_each_window(target, instructions, run, stack, scratch, |window| {
        if (window.rewrite.start, window.rewrite.end) == (run.start, run.end)
            || !window.is_profitable(target)
        {
            return;
        }
        if best.as_ref().is_some_and(|best| best.rewrite.start != window.rewrite.start) {
            windows.extend(best.take());
        }
        if best.as_ref().is_none_or(|best| window.saves_at_least(best, target)) {
            best = Some(window);
        }
    });
    windows.extend(best);
    windows.sort_by(|a, b| {
        target.cmp(a.rewrite.old + b.pooled_new(), b.rewrite.old + a.pooled_new()).reverse()
    });
    let mut taken = Vec::<Range<usize>>::new();
    let mut found = Vec::new();
    for window in windows {
        let range = window.rewrite.start..window.rewrite.end;
        if taken.iter().all(|taken| range.end <= taken.start || taken.end <= range.start) {
            taken.push(range);
            found.push((window.data, window.rewrite));
        }
    }
    found
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

/// The buffers of a run scan, reused across scans.
#[derive(Default)]
struct RunScratch {
    stack: Vec<RunWord>,
    stores: Vec<(u64, U256, bool)>,
    ranges: Vec<(u64, u64)>,
    ends: Vec<RunEnd>,
    extras: Vec<RunWord>,
    /// The start, destination base slot, and first store of the last scan.
    start: usize,
    destination: Option<usize>,
    first_store: usize,
}

/// Stores of constants to one base address that a single copy can replace.
struct Run {
    rewrite: Rewrite,
    data: Bytes,
    zero: bool,
    first_store: usize,
}

/// A point where a run scan can stop: after a store that leaves the stores in one range and a stack
/// a copy can rebuild. `extras` indexes the stack words above `kept` in `RunScratch::extras`.
struct RunEnd {
    end: usize,
    old: Cost,
    stores: usize,
    range: (u64, u64),
    inputs: usize,
    kept: usize,
    extras: Range<usize>,
}

impl Run {
    /// The cost of the copy with the bytes it adds to the data pool, when it needs data.
    fn pooled_new(&self) -> Cost {
        let data = if self.zero { 0 } else { self.data.len() as u32 };
        self.rewrite.new + Cost::new(0, data)
    }

    /// Whether this run pays for its data alone, without sharing it with other sites.
    fn is_profitable(&self, target: &Target) -> bool {
        let saving = Improvement {
            runtime_gas: i64::from(target.runtime_gas(self.rewrite.old))
                - i64::from(target.runtime_gas(self.pooled_new())),
            bytes: i64::from(self.rewrite.old.bytes) - i64::from(self.pooled_new().bytes),
        };
        saving.is_profitable(target)
    }

    /// Whether this saves at least as much as `other`, counting the data each adds.
    fn saves_at_least(&self, other: &Self, target: &Target) -> bool {
        target
            .cmp(other.rewrite.old + self.pooled_new(), self.rewrite.old + other.pooled_new())
            .is_le()
    }
}

/// Symbolically runs the pushes, stack operations, constant expressions, and constant `MSTORE`
/// and `MSTORE8`s from `start`, and returns the longest prefix whose stores fill one contiguous
/// range and whose final stack a copy can rebuild.
fn scan_run(
    target: &Target,
    stack: &BlockStack<'_>,
    instructions: &[Instruction],
    start: usize,
    scratch: &mut RunScratch,
) -> Option<Run> {
    scan_ends(target, instructions, start, scratch);
    run_ending_at(target, stack, scratch, scratch.ends.len().checked_sub(1)?)
}

/// Scans the stores from `start` and leaves every point where the run can stop in `scratch.ends`.
fn scan_ends(
    target: &Target,
    instructions: &[Instruction],
    start: usize,
    scratch: &mut RunScratch,
) {
    scratch.ends.clear();
    if !is_split_point(instructions, start) {
        return;
    }
    let RunScratch { stack, stores, ranges, ends, extras, .. } = scratch;
    stack.clear();
    stores.clear();
    ranges.clear();
    extras.clear();
    let mut inputs = 0;
    let mut old = Cost::ZERO;
    let mut first_store = None;
    let mut base = None;
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
            ensure(stack, &mut inputs, stack_op.required_depth());
            apply_stack_op(stack, stack_op);
            old += cost;
            continue;
        }
        let opcode = inst.opcode;
        if matches!(opcode, op::MSTORE | op::MSTORE8) {
            ensure(stack, &mut inputs, 2);
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
            insert_range(ranges, (offset, offset + width));
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
                let first_extra = extras.len();
                extras.extend_from_slice(&stack[kept..]);
                ends.push(RunEnd {
                    end,
                    old,
                    stores: stores.len(),
                    range,
                    inputs,
                    kept,
                    extras: first_extra..extras.len(),
                });
            }
            continue;
        }
        let Some((input_count, 1)) = op::stack_io(opcode) else { break };
        let input_count = usize::from(input_count);
        ensure(stack, &mut inputs, input_count);
        let operands = stack.drain(stack.len() - input_count..).collect::<SmallVec<[_; 3]>>();
        let arguments = operands.iter().rev().map(|word| word.offset).collect::<SmallVec<[_; 3]>>();
        let result = match operands[..] {
            [a, b] if opcode == op::ADD && a.slot.xor(b.slot).is_some() => {
                RunWord { slot: a.slot.or(b.slot), offset: a.offset.wrapping_add(b.offset) }
            }
            _ if operands.iter().all(|word| word.slot.is_none()) => {
                let Some(value) = eval::eval_opcode(opcode, &arguments) else { break };
                RunWord::constant(value)
            }
            _ => break,
        };
        let arguments = arguments.into_iter().map(Some).collect::<SmallVec<[_; 3]>>();
        old += target.opcode_with_immediates(opcode, &arguments);
        stack.push(result);
    }

    scratch.start = start;
    scratch.destination = base.flatten();
    scratch.first_store = first_store.unwrap_or_default();
}

/// Builds and prices the copy that replaces the run that stops at `scratch.ends[index]`.
fn run_ending_at(
    target: &Target,
    stack: &BlockStack<'_>,
    scratch: &RunScratch,
    index: usize,
) -> Option<Run> {
    let RunScratch { stores, ends, extras, start, destination, first_store, .. } = scratch;
    let run_end = &ends[index];
    let (end, old, store_count) = (run_end.end, run_end.old, run_end.stores);
    let (low, high) = run_end.range;
    let (inputs, kept) = (run_end.inputs, run_end.kept);
    let extras = &extras[run_end.extras.clone()];
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
    let destination = RunWord { slot: *destination, offset: U256::from(low) };
    let plan = CopyPlan {
        target,
        stack,
        start: *start,
        first_store: *first_store,
        end,
        old,
        inputs,
        kept,
        extras,
    };
    let whole = plan.price(destination, &data, None)?;
    // A copy of data with zeros at its end can stop before them if a zero word store clears the
    // last word first. The store costs code but the pool gets fewer bytes.
    let zeros = data.iter().rev().take_while(|&&byte| byte == 0).count().min(WORD_BYTES - 1);
    let shortened = if !whole.zero && zeros > 0 && data.len() > WORD_BYTES {
        plan.price(destination, &data[..data.len() - zeros], Some(data.len() - WORD_BYTES))
    } else {
        None
    };
    Some(match shortened {
        Some(shortened) if !whole.saves_at_least(&shortened, target) => shortened,
        _ => whole,
    })
}

/// What a copy that replaces the stores of a run depends on.
struct CopyPlan<'a> {
    target: &'a Target,
    stack: &'a BlockStack<'a>,
    start: usize,
    first_store: usize,
    end: usize,
    old: Cost,
    inputs: usize,
    kept: usize,
    extras: &'a [RunWord],
}

impl CopyPlan<'_> {
    /// Prices the copy of `data` to `destination`, which first clears the word at `clear` bytes
    /// from the start of the range, if any.
    fn price(&self, destination: RunWord, data: &[u8], clear: Option<usize>) -> Option<Run> {
        let Self { target, stack, start, first_store, end, old, inputs, kept, extras } = *self;
        let zero = data.iter().all(|&byte| byte == 0);
        let tail = copy_tail(destination, zero, inputs, kept, extras);
        // push 0; mstore destination + clear
        let head = clear.map_or_else(Vec::new, |clear| {
            let word = RunWord { offset: destination.offset + U256::from(clear), ..destination };
            [Instruction::push_value(U256::ZERO)]
                .into_iter()
                .chain(push_address(word, 1))
                .chain([Instruction::opcode(op::MSTORE)])
                .collect()
        });
        if !stack.admits(start, &head, &tail) {
            return None;
        }
        let mut new = target.copy_operands(data.len(), zero);
        for inst in head.iter().chain(&tail) {
            new += instruction_cost(target, inst)?;
        }
        let data = Bytes::copy_from_slice(data);
        let size = data.len();
        let rewrite =
            Rewrite { block: stack.block, start, end, size, old, new, head, tail, repeats: false };
        Some(Run { rewrite, data, zero, first_store })
    }
}

/// The verifier's entry bounds for a module, computed when a copy first needs them.
struct StackLimits {
    evm_version: EvmVersion,
    entries: OnceCell<IndexVec<BlockId, Option<usize>>>,
}

/// The stack limit of one block: what its original path reaches, and so what a copy may reach.
struct BlockStack<'a> {
    module: &'a Module,
    limits: &'a StackLimits,
    block: BlockId,
    /// The depth before each instruction, relative to the block's entry.
    depths: Vec<isize>,
    /// The highest depth, relative to the block's entry, that the original path reaches around
    /// each instruction: before it, and after it up to the first conditional jump or terminal,
    /// with the terminator's own growth.
    peaks: Vec<isize>,
}

impl<'a> BlockStack<'a> {
    fn new(module: &'a Module, limits: &'a StackLimits, block_id: BlockId) -> Self {
        let block = &module.blocks[block_id];
        let mut depths = vec![0];
        for inst in &block.instructions {
            depths.push(depths[depths.len() - 1] + net_growth(inst));
        }
        let growth = block.terminator.as_ref().map_or(0, |term| {
            term.kind.lowering_stack_growth(module.next_block(block_id)) as isize
        });
        let mut peaks = vec![0; depths.len()];
        let mut reach = depths[block.instructions.len()] + growth;
        for (index, inst) in block.instructions.iter().enumerate().rev() {
            if inst
                .as_evm_opcode()
                .is_some_and(|opcode| opcode == op::JUMPI || op::is_terminal(opcode))
            {
                reach = depths[index + 1];
            }
            reach = reach.max(depths[index]);
            peaks[index] = reach;
        }
        let mut before = 0;
        for (peak, &depth) in peaks.iter_mut().zip(&depths) {
            before = before.max(depth);
            *peak = (*peak).max(before);
        }
        Self { module, limits, block: block_id, depths, peaks }
    }

    /// Returns whether the copy of `head`, the size and source pushes, and `tail` can replace the
    /// run that starts at `start` without passing the stack limit.
    fn admits(&self, start: usize, head: &[Instruction], tail: &[Instruction]) -> bool {
        let mut depth = self.depths[start];
        let mut peak = depth;
        for inst in head {
            depth += net_growth(inst);
            peak = peak.max(depth);
        }
        // push size; push data | calldatasize
        depth += 2;
        peak = peak.max(depth);
        for inst in tail {
            depth += net_growth(inst);
            peak = peak.max(depth);
        }
        peak <= self.peaks[start] || {
            let entries = self.limits.entries.get_or_init(|| {
                Verifier::stack_entry_upper_bounds(self.module, self.limits.evm_version)
            });
            entries[self.block]
                .is_some_and(|entry| entry as isize + peak <= MAX_STACK_DEPTH as isize)
        }
    }
}

/// The words the instruction leaves on the stack, less the words it takes.
fn net_growth(inst: &Instruction) -> isize {
    let effect = inst.stack_effect();
    isize::from(effect.outputs) - isize::from(effect.inputs)
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
    tail.extend(push_address(destination, 2));
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

/// The instructions that push `address` when `pushed` words sit above the inputs of the run.
fn push_address(address: RunWord, pushed: usize) -> Vec<Instruction> {
    let dup = |depth: usize| Instruction::stack_op(op::StackOp::Dup(depth as u8));
    match address {
        RunWord { slot: Some(slot), offset } if offset.is_zero() => vec![dup(slot + 1 + pushed)],
        RunWord { slot: Some(slot), offset } => vec![
            Instruction::push_value(offset),
            dup(slot + 2 + pushed),
            Instruction::opcode(op::ADD),
        ],
        RunWord { slot: None, offset } => vec![Instruction::push_value(offset)],
    }
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

/// Applies a stack operation to a symbolic stack that is deep enough for it.
fn apply_stack_op<T: Copy>(stack: &mut Vec<T>, stack_op: op::StackOp) {
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
}

/// Shares storage between data entries after deferred data is linked in.
pub(in crate::backend) fn pack_linked_data(module: &mut Module) -> bool {
    !module.data.is_empty()
        && scan_data_references(module, None).is_some_and(|mut scan| pack_data(module, &mut scan))
}

fn pack_data(module: &mut Module, scan: &mut DataScan) -> bool {
    let DataScan { referenced, tail, in_place } = scan;
    let in_place = *in_place;
    if module.data.is_empty() {
        return false;
    }
    let mut order = module
        .data
        .indices()
        .filter(|&id| referenced.contains(id) || module.data[id].emit_in_runtime)
        .collect::<Vec<_>>();
    if order.is_empty() {
        module.data.clear();
        *tail = None;
        return true;
    }
    if module.data.len() == 1 {
        return false;
    }
    order.sort_unstable_by(|&a, &b| {
        let len = |id: DataId| module.data[id].bytes.known().map_or(0, |bytes| bytes.len());
        len(b).cmp(&len(a)).then_with(|| a.cmp(&b))
    });

    // Entries keyed by the first old entry that created them.
    let mut packed = FxIndexMap::<DataId, Data>::default();
    let mut exact =
        FxHashMap::<(Bytes, Vec<LibraryRelocation>, Vec<Range<usize>>), DataId>::default();
    let mut remap = FxHashMap::default();
    let subslices =
        !module.optimization.is_none() && module.data.len() < MAX_DATA_SUBSTRING_ENTRIES;
    for old_id in order {
        let data = &module.data[old_id];
        // Deferred bytes are not known yet, so they are never shared, and the tail stays in place
        // when its address is used to find the constructor arguments.
        let key = (data.bytes.known())
            .filter(|_| !data.emit_in_runtime && !(in_place && Some(old_id) == *tail))
            .map(|bytes| {
                let trailers = data.trailers.iter().map(|trailer| trailer.range.clone());
                (bytes.clone(), data.library_relocations.clone(), trailers.collect())
            });
        let shared = key.as_ref().and_then(|key| {
            let exact = exact.get(key).map(|&id| DataRef::new(id, 0));
            exact.or_else(|| {
                let at_end = Some(old_id) == *tail;
                subslices.then(|| find_data(&packed, data, old_id, at_end)).flatten()
            })
        });
        let data_ref = shared.unwrap_or_else(|| {
            packed.insert(old_id, data.clone());
            if let Some(key) = key {
                exact.insert(key, old_id);
            }
            DataRef::new(old_id, 0)
        });
        let entry = &mut packed[&data_ref.id];
        if entry.name.is_none() {
            entry.name = data.name;
        }
        add_trailer_owners(entry, &data.trailers, data_ref.offset as usize);
        remap.insert(old_id, data_ref);
    }

    // The entry that holds the tail stays last, which new entries would otherwise follow, and the
    // trailers of the runtime code stay at its end.
    // A tail that no instruction references is gone with the other unreferenced entries.
    let host = tail.and_then(|tail| remap.get(&tail)).map(|data_ref| data_ref.id);
    let last = |id: &DataId, data: &Data| (Some(*id) == host || data.emit_in_runtime, *id);
    packed.sort_unstable_by(|id, data, other_id, other| last(id, data).cmp(&last(other_id, other)));
    if packed.values().eq(&module.data) {
        return false;
    }
    for data_ref in remap.values_mut() {
        data_ref.id = DataId::new(packed.get_index_of(&data_ref.id).unwrap());
    }
    *tail = tail.and_then(|tail| remap.get(&tail)).map(|data_ref| data_ref.id);
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
    module.data = packed.into_values().collect();
    true
}

/// Finds `needle` inside an entry created from an older entry than `needle_id`, at its end when
/// `at_end`.
fn find_data(
    packed: &FxIndexMap<DataId, Data>,
    needle: &Data,
    needle_id: DataId,
    at_end: bool,
) -> Option<DataRef> {
    let needle_bytes = needle.bytes.known()?;
    let mut finder = NeedleFinder::new(needle_bytes.clone());
    packed.iter().find_map(|(&id, known)| {
        if id >= needle_id {
            return None;
        }
        let haystack = known.bytes.known()?;
        let offset = if at_end {
            // The tail must end its host, which goes last, so the deployment code ends with it.
            haystack.ends_with(needle_bytes).then(|| haystack.len() - needle_bytes.len())?
        } else {
            finder.find(haystack)?
        };
        let end = offset + needle_bytes.len();
        let compatible = known
            .library_relocations
            .iter()
            .copied()
            .filter(|reloc| reloc.offset < end && reloc.offset + 20 > offset)
            .map(|reloc| (reloc.offset.checked_sub(offset), reloc.library))
            .eq(needle.library_relocations.iter().map(|reloc| (Some(reloc.offset), reloc.library)))
            // Verifiers replace trailer bytes, so shared bytes must cover the same trailers.
            && known
                .trailers
                .iter()
                .filter(|trailer| trailer.range.start < end && trailer.range.end > offset)
                .map(|trailer| (trailer.range.start.checked_sub(offset), trailer.range.end - offset))
                .eq(needle.trailers.iter().map(|trailer| (Some(trailer.range.start), trailer.range.end)));
        compatible.then(|| DataRef::new(id, data_offset(offset)))
    })
}

/// Records the owners of `trailers`, which start `offset` bytes into `shared`, on the trailers of
/// `shared` that cover the same bytes. Owners never affect sharing, so contracts with
/// byte-identical code keep one copy.
fn add_trailer_owners(shared: &mut Data, trailers: &[Trailer], offset: usize) {
    for trailer in trailers {
        let trailer = trailer.shifted(offset);
        let shared = shared
            .trailers
            .iter_mut()
            .find(|shared| shared.range == trailer.range)
            .expect("shared data covers the same trailers");
        for contract in trailer.contracts {
            if !shared.contracts.contains(&contract) {
                shared.contracts.push(contract);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum DataStackValue {
    Unknown,
    CodeSize,
    Immediate(usize),
    Data(DataRef),
    Size(DataSize),
}

pub(crate) fn data_layout_is_observable(module: &Module) -> bool {
    scan_data_references(module, None).is_none()
}

/// The data that instructions reference, and the entry that must stay last.
struct DataScan {
    referenced: DenseBitSet<DataId>,
    /// The last entry of creation code, which the runtime code ends the deployment code with and
    /// constructor arguments follow, so only bytes before it may change.
    tail: Option<DataId>,
    /// Whether the tail must keep its place and bytes, because code subtracts its address from
    /// `CODESIZE` or copies from it without a known size.
    in_place: bool,
}

/// Scans the data that instructions reference, or returns `None` when a data address may reach
/// anything but a `CODECOPY` that stays inside its entry, or an address of the tail that is not
/// subtracted from `CODESIZE` or copied from. `pinned` is the tail of an earlier scan, which new
/// entries may follow.
fn scan_data_references(module: &Module, pinned: Option<DataId>) -> Option<DataScan> {
    let tail = pinned.or_else(|| module.data.indices().next_back().filter(|_| module.code_follows));
    let mut scan =
        DataScan { referenced: DenseBitSet::new_empty(module.data.len()), tail, in_place: false };
    let mut stack = Vec::new();
    for block in &module.blocks {
        // The stack starts empty, so without a data push no slot can hold a data address
        // or size.
        let has_data = block
            .instructions
            .iter()
            .any(|inst| inst.pushed_data().is_some() || inst.pushed_data_size().is_some());
        if !has_data {
            continue;
        }
        for inst in &block.instructions {
            if escapes_data(module, inst, &mut stack, &mut scan) {
                return None;
            }
        }
        // Shared tails can move a copy behind a jump, so follow data addresses that
        // survive an unconditional jump into its target with this path's stack.
        let mut terminator = &block.terminator;
        for _ in 0..MAX_DATA_JUMP_HOPS {
            let Some(Terminator { kind: TerminatorKind::Jump(target), .. }) = terminator else {
                break;
            };
            if !holds_data(&stack) {
                break;
            }
            let target = &module.blocks[*target];
            for inst in &target.instructions {
                if escapes_data(module, inst, &mut stack, &mut scan) {
                    return None;
                }
            }
            terminator = &target.terminator;
        }
        if holds_data(&stack) {
            return None;
        }
        stack.clear();
    }
    Some(scan)
}

fn holds_data(stack: &[DataStackValue]) -> bool {
    stack.iter().any(|value| matches!(value, DataStackValue::Data(_)))
}

/// Executes `inst` on the symbolic `stack`, records the data it pushes in `referenced`, and
/// returns whether it makes the data layout observable.
fn escapes_data(
    module: &Module,
    inst: &Instruction,
    stack: &mut Vec<DataStackValue>,
    scan: &mut DataScan,
) -> bool {
    if inst.opcode == op::CODESIZE {
        stack.push(DataStackValue::CodeSize);
        return false;
    }
    if let Some(data) = inst.pushed_data() {
        scan.referenced.insert(data.id);
        stack.push(DataStackValue::Data(data));
        return false;
    }
    if let Some(size) = inst.pushed_data_size() {
        scan.referenced.insert(size.data);
        stack.push(DataStackValue::Size(size));
        return false;
    }
    if let Some(value) = inst.concrete_immediate() {
        stack.push(
            usize::try_from(value).map_or(DataStackValue::Unknown, DataStackValue::Immediate),
        );
        return false;
    }
    if inst.is_encoded_push() {
        stack.push(DataStackValue::Unknown);
        return false;
    }
    if let Some(stack_op) = inst.as_stack_op() {
        ensure_stack_depth(stack, stack_op.required_depth());
        apply_stack_op(stack, stack_op);
        return false;
    }

    // A raw branch has a physical successor that this local scan cannot follow.
    // A data address that survives it may be used there.
    if inst.has_raw_branch_target() && holds_data(stack) {
        return true;
    }

    let effect = inst.stack_effect();
    let inputs = usize::from(effect.inputs);
    ensure_stack_depth(stack, inputs);
    let first_input = stack.len() - inputs;
    for (index, value) in stack[first_input..].iter().rev().enumerate() {
        let DataStackValue::Data(data) = value else { continue };
        // Bytes after the tail stay after it, since new entries go before it, so `codesize -
        // address` and a copy to the end of the code read the same bytes.
        let at_tail = Some(data.id) == scan.tail;
        let contained = match inst.opcode {
            op::CODECOPY if index == 1 => {
                let bounded = match stack[first_input] {
                    DataStackValue::Immediate(size) => data_copy_is_bounded(module, *data, size),
                    DataStackValue::Size(size) => size.is_length_of(*data),
                    _ => false,
                };
                scan.in_place |= at_tail && !bounded;
                bounded || at_tail
            }
            op::SUB if index == 1 => {
                let from_code_end =
                    at_tail && matches!(stack.last(), Some(DataStackValue::CodeSize));
                scan.in_place |= from_code_end;
                from_code_end
            }
            _ => false,
        };
        if !contained {
            return true;
        }
    }
    stack.truncate(first_input);
    stack.extend(std::iter::repeat_n(DataStackValue::Unknown, usize::from(effect.outputs)));
    false
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
