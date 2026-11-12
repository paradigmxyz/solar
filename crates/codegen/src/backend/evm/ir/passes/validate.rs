//! Translation validation of EVM IR passes: SMT queries that a pass preserved each code region it
//! changed.
//!
//! With `-Zvalidate-evm-ir-passes=DIR`, the EVM IR pass manager keeps a copy of the program before
//! every pass that runs, and after a pass that reports a change hands both versions to
//! [`write_queries`]. The program is cut into regions at blocks both versions have, and each region
//! that runs through a block the pass changed, in either version, becomes one SMT-LIB query in
//! `DIR` whose satisfiability means some state on entry to the region makes the two versions
//! observably differ before they leave it. The queries share the MIR pass validator's encoding of
//! memory, storage, the world and outcomes, its file format and its header (see
//! `mir/transform/validate_passes.rs`), with the pass named `evm-ir/<pass>` and the function
//! `region bbN`. A region the encoding does not cover gets a `.skip` file with the reason.
//! `tools/prove-analyses --validate-evm-ir-passes` compiles a corpus this way, solves the queries
//! and reports verdicts per pass.
//!
//! # Regions
//!
//! Blocks correspond by label: passes keep a block's label while they rewrite, move or retarget
//! it, and give the blocks they create fresh ones. Regions start at blocks whose label both
//! versions have: the entry, every block whose address either version pushes (return
//! continuations, entered from another region through a word on the stack), and enough blocks to
//! cut every cycle. A region stops only at the entry and at those cycle cuts, where control
//! continues with the whole state, or when the transaction ends; it runs on through any other
//! block, function entries and continuations included. Every cycle passes through a cut, every
//! jump through a word the region did not push lands on a pushed block, and the entry is a start,
//! so the regions cover every execution: once each region is proved, induction over the cuts and
//! continuations an execution passes proves the two programs equivalent. A region whose blocks
//! are the same in both versions, as text with targets by label, is equal by construction and
//! gets no query.
//!
//! The cycle cuts are found by exploring: a block a path reaches again becomes one, and once every
//! region is acyclic each cut is dropped again if the others still break every cycle, so that a
//! pass that moves code across a loop header, such as one that copies the loop test into the
//! latch, keeps the region that runs through both. Passes that merge, share or duplicate code
//! (tail merging, block CSE, terminal deduplication, shared reverts, outlining, block copies) need
//! no mapping of their own: a block only one version has is never a cut, so a region runs through
//! it, and through an outlined body as if it were inlined at each call, since a jump to an address
//! the region pushed resolves to that block. A region in which a cycle runs through such a block
//! alone is skipped.
//!
//! # Encoding
//!
//! The stack is explicit: a region starts with an unknown stack whose words are the symbols
//! `arg0` (the top), `arg1`, and so on, each instruction moves symbolic words as the EVM does, and
//! the depth relative to the region's entry is known at every point. Paths are merged per block
//! and stack shape, the depth and the positions of block addresses, which keeps every jump to an
//! address the region pushed resolvable; each merged point's guard and state are `ite` chains over
//! its incoming paths, as in the MIR validator, and a load from a merged memory is the merge of
//! the loads from each incoming memory, so it still reads through their stores. Pure opcodes
//! follow the opcode table's word semantics (`smt_opcode`); memory, storage, transient storage,
//! transaction inputs, environment reads, copies, hashes, logs, calls and creations follow the MIR
//! validator, except for copies whose bytes are known: `codecopy` of known data of a constant
//! size, `calldatacopy` from the end of calldata (zeros) and `mcopy` of a constant size write
//! their bytes. The size of the return data is part of the state: zero where no call can run
//! before the region in the version before the pass, the result of the last call after one, and
//! otherwise unknown. A block's address is a symbol of its label, distinct from every other
//! block's, except that an empty continuation block, whose address only control transfers use,
//! stands for the block it passes control to; a data reference is a symbol of the data's contents
//! plus its offset, so moving blocks or data does not change a word. Immutables and library
//! addresses are symbols of their identifiers.
//!
//! A region ends in the outcomes of a transaction (return data, stop as returning no bytes, revert
//! data, `invalid` and opcodes the target lacks, self-destruct), compared as in the MIR
//! validator, with data of a constant size compared word by word, or in a continuation: a jump to
//! a cut, or to an address the region did not push, compared by the target, the stack, the return
//! data's size, all of memory, storage, transient storage and the world. Outcomes of one kind (for
//! data of a constant size, one size; for a continuation, one target block and depth) are merged
//! into one, so a query is linear in its outcomes.
//!
//! A query takes these facts on trust and lists the ones it uses in its header:
//!
//! - `memory-gas`, as in the MIR validator;
//! - `free-memory-pointer`: on entry to a region other than the entry's, the free-memory pointer
//!   lies above the reserved words and below 2^66, as in the MIR validator;
//! - `result-type`: an opcode the table gives a narrow result, such as `caller`, which an
//!   uninterpreted function stands for, returns a word that fits it;
//! - `indexed-jump`: an indexed jump's index lies in its table, which lowering does not check;
//! - `continuation`: an empty block marked as a continuation has an address only control transfers
//!   use.
//!
//! # Limits
//!
//! - A region is explored up to [`MAX_STATES`] block and stack-shape pairs per version.
//! - `pc` and unresolved relocations are not modeled. An opcode the target lacks halts like
//!   `invalid`, but a stack operation deeper than the target's opcodes reach is not checked.
//! - Gas is not compared: `gas` reads the world like other environment reads, and neither stack
//!   overflow nor running out of gas is an outcome. Stack underflow is assumed absent, as the
//!   verifier checks it.
//! - A pass that changes the state at a cut, such as one that moves a stack word across a block
//!   boundary that is a cut, gets a counterexample, since a cut's state is compared whole.
//! - A block's address abstracts the assembled one, so code that computes with an address, or
//!   copies code bytes outside data, is compared through uninterpreted terms.
//!
//! Validation only reads the two versions: it never changes code, and debug info is not compared.
//! It costs a copy of the program per pass only with the flag.

use crate::{
    backend::evm::{
        ir::{Block, BlockId, DataBytes, DataId, Instruction, Module, TerminatorKind},
        op::{self, StackOp},
    },
    mir::transform::{
        prove_analyses::{sanitize, word},
        validate_passes::{
            BOOL, Encoder, Kind, MEMORY, Outcome, Query, STORAGE, State, WORD, WORLD, all, and,
            any, parse_word,
        },
    },
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet, FxIndexMap, FxIndexSet},
};
use std::{
    fmt::Write,
    hash::{DefaultHasher, Hash, Hasher},
    io::Write as _,
    path::Path,
};

/// The most block and stack-shape pairs a region's exploration visits in one version.
const MAX_STATES: usize = 2048;

/// The most bytes of constant-size return or revert data compared word by word.
const MAX_DATA: u64 = 1024;

/// The most bytes of known data a `codecopy` writes into memory exactly.
const MAX_COPY: usize = 4096;

/// Writes a query to `dir`, or stdout when `dir` is `-`, for every region of `after` that runs
/// through a block `pass` changed, with its version in `before`, the program before the pass.
pub(super) fn write_queries(
    pass: &str,
    before: &Module,
    after: &Module,
    dir: &Path,
    evm_version: EvmVersion,
) -> std::io::Result<()> {
    if before == after {
        return Ok(());
    }
    let to_stdout = dir == Path::new("-");
    if !to_stdout {
        std::fs::create_dir_all(dir)?;
    }
    let pass = format!("evm-ir/{pass}");
    let module = after.name.to_string();
    let mut stdout = String::new();
    for (label, result, digest) in regions(&pass, before, after, evm_version) {
        let mut hasher = DefaultHasher::new();
        (&pass, label, digest).hash(&mut hasher);
        let file = format!(
            "{}-{}-r{label}-{:012x}",
            sanitize(&module),
            sanitize(&pass),
            hasher.finish() & 0xffff_ffff_ffff
        );
        let header = format!(
            "; module: {module}\n; function: region bb{label}\n; pass: {pass}\n; claim: {pass} preserves region bb{label}\n"
        );
        let (file, text) = match result {
            Ok(query) => (
                format!("{file}.smt2"),
                format!(
                    "{header}; opaque: {}\n; axioms: {}\n{}",
                    query.opaque.join(" "),
                    query.axioms.join(" "),
                    query.text
                ),
            ),
            Err(reason) => (format!("{file}.skip"), format!("{header}; skipped: {reason}\n")),
        };
        if to_stdout {
            stdout.push_str(&format!("; query: {file}\n{text}"));
        } else {
            std::fs::write(dir.join(file), text)?;
        }
    }
    if to_stdout {
        std::io::stdout().lock().write_all(stdout.as_bytes())?;
    }
    Ok(())
}

/// The query, or the reason for skipping it, of each region that runs through a changed block,
/// by the label of its cut, with a digest of what it compares.
fn regions(
    pass: &str,
    before: &Module,
    after: &Module,
    evm_version: EvmVersion,
) -> Vec<(u32, Result<Query, String>, u64)> {
    let (old, new) = match (Program::new(before), Program::new(after)) {
        (Ok(old), Ok(new)) => (old, new),
        (Err(reason), _) | (_, Err(reason)) => return vec![(entry_label(after), Err(reason), 0)],
    };
    let mut changed = FxHashSet::default();
    for (&label, &block) in &new.by_label {
        if old.by_label.get(&label).is_none_or(|&other| old.key(other) != new.key(block)) {
            changed.insert(label);
        }
    }
    changed.extend(old.by_label.keys().filter(|label| !new.by_label.contains_key(label)));
    if changed.is_empty() {
        return Vec::new();
    }
    let mut changed_keys: Vec<_> = changed.iter().copied().collect();
    changed_keys.sort_unstable();
    let digest = |extra: u64| {
        let mut hasher = DefaultHasher::new();
        (pass, &changed_keys).hash(&mut hasher);
        for &label in &changed_keys {
            old.by_label.get(&label).map(|&block| old.key(block)).hash(&mut hasher);
            new.by_label.get(&label).map(|&block| new.key(block)).hash(&mut hasher);
        }
        extra.hash(&mut hasher);
        hasher.finish()
    };
    if old.module.blocks.is_empty() || new.module.blocks.is_empty() {
        return vec![(entry_label(after), Err("an empty program".into()), digest(0))];
    }
    let entry = new.label(BlockId::ENTRY);
    if old.label(BlockId::ENTRY) != entry {
        return vec![(entry, Err("the entry block changed".into()), digest(1))];
    }

    // Regions start at the entry, blocks whose address is pushed and blocks a cycle runs through,
    // of the labels both versions have. They stop only at the entry and at cycles: a return
    // continuation is entered from another region, through a return address on its stack, and a
    // region that reaches one directly may run on through it like through any other block.
    let common: FxHashSet<u32> =
        new.by_label.keys().copied().filter(|label| old.by_label.contains_key(label)).collect();
    let mut stops = FxHashSet::default();
    stops.insert(entry);
    let mut starts = FxHashSet::default();
    for program in [&old, &new] {
        for block in &program.module.blocks {
            for inst in &block.instructions {
                if let Some(target) = inst.pushed_block() {
                    starts.insert(program.label(target));
                }
            }
        }
    }
    stops.retain(|label| common.contains(label));
    starts.retain(|label| common.contains(label));
    starts.extend(stops.iter().copied());
    let touches = |program: &Program<'_>, shape: &Shape| {
        shape.nodes.keys().any(|key| changed.contains(&program.label(key.block)))
    };
    // Each region's shape in both versions, or the label of a block a cycle runs through that
    // both versions have, which must be a stop.
    type Regions = (Vec<(u32, Shape, Shape)>, Vec<(u32, String)>);
    let explore = |starts: &FxHashSet<u32>, stops: &FxHashSet<u32>| -> Result<Regions, u32> {
        let mut order: Vec<_> = starts.iter().copied().collect();
        order.sort_unstable();
        let mut shapes = Vec::new();
        let mut skipped = Vec::new();
        for &cut in &order {
            let [(old_result, old_shape), (new_result, new_shape)] =
                [&old, &new].map(|program| program.shape(program.by_label[&cut], stops));
            // Only a region that runs through a changed block, as far as it was explored, counts.
            let touches = touches(&old, &old_shape) || touches(&new, &new_shape);
            match [old_result, new_result] {
                [Ok(()), Ok(())] => shapes.push((cut, old_shape, new_shape)),
                [Err(Explore::Cycle(label)), _] | [_, Err(Explore::Cycle(label))]
                    if common.contains(&label) =>
                {
                    return Err(label);
                }
                _ if !touches => {}
                [Err(Explore::Cycle(label)), _] | [_, Err(Explore::Cycle(label))] => skipped.push(
                    (cut, format!("a cycle runs through bb{label}, which one version lacks")),
                ),
                [Err(Explore::Skip(reason)), _] | [_, Err(Explore::Skip(reason))] => {
                    skipped.push((cut, reason))
                }
            }
        }
        Ok((shapes, skipped))
    };
    let mut cycles = Vec::new();
    let (mut shapes, mut skipped) = loop {
        match explore(&starts, &stops) {
            Ok(regions) => break regions,
            Err(label) => {
                stops.insert(label);
                starts.insert(label);
                cycles.push(label);
            }
        }
    };
    // Fewer stops let a region follow a pass that moves code across one, such as a loop test
    // copied into the latch: drop each stop that the cycles do not need.
    for label in cycles {
        let mut fewer = stops.clone();
        fewer.remove(&label);
        if let Ok((fewer_shapes, fewer_skipped)) = explore(&starts, &fewer)
            && fewer_skipped.len() <= skipped.len()
        {
            (stops, shapes, skipped) = (fewer, fewer_shapes, fewer_skipped);
        }
    }

    let mut out = Vec::new();
    for (cut, reason) in skipped {
        let digest = digest(hash_of((cut, &reason)));
        out.push((cut, Err(reason), digest));
    }
    for (cut, old_shape, new_shape) in &shapes {
        if !touches(&old, old_shape) && !touches(&new, new_shape) {
            continue;
        }
        let fresh = old.fresh_returndata(old.by_label[cut]);
        let result = Validator { evm_version, ..Default::default() }.encode(
            *cut == entry,
            (&old, old_shape),
            (&new, new_shape),
            &stops,
            fresh,
        );
        let region_digest = result.as_ref().map(|query| query.text.clone()).unwrap_or_default();
        out.push((*cut, result, digest(hash_of((cut, region_digest)))));
    }
    out.sort_by_key(|&(cut, ..)| cut);
    out
}

/// A hash of `value`.
fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The label of a program's entry block, or zero for an empty program.
fn entry_label(module: &Module) -> u32 {
    module.blocks.first().map_or(0, |block| block.label)
}

/// One version of the program, with its blocks by label.
struct Program<'a> {
    module: &'a Module,
    by_label: FxHashMap<u32, BlockId>,
}

/// Why a region's exploration stopped.
enum Explore {
    /// A cycle runs through this block, which must be a cut.
    Cycle(u32),
    /// The region is skipped for this reason.
    Skip(String),
}

/// A point of a region: a block and the shape of the stack on entry to it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    block: BlockId,
    /// The depth relative to the region's entry.
    height: isize,
    /// The positions from the top that hold a block's address, with that block's label.
    labels: Vec<(usize, u32)>,
}

/// The points of a region in one version and the edges between them, which form a DAG.
struct Shape {
    /// Each point with the points it continues to; the region's cut first.
    nodes: FxIndexMap<Key, Vec<usize>>,
    /// The points in postorder.
    order: Vec<usize>,
}

/// How an operation moves control.
enum Control<T> {
    /// To the next operation.
    Next,
    /// To this target when the condition, the second word, is not zero, and otherwise on.
    JumpIf(T, T),
    /// To this target.
    Jump(T),
    /// Out of the transaction.
    End(Kind),
}

impl<'a> Program<'a> {
    fn new(module: &'a Module) -> Result<Self, String> {
        let mut by_label = FxHashMap::default();
        for (id, block) in module.blocks.iter_enumerated() {
            if by_label.insert(block.label, id).is_some() {
                return Err(format!("two blocks are labeled bb{}", block.label));
            }
        }
        Ok(Self { module, by_label })
    }

    fn label(&self, block: BlockId) -> u32 {
        self.module.blocks[block].label
    }

    /// The label whose address stands for `block`'s: the block an empty continuation, whose
    /// address only control transfers use, passes control to, and otherwise its own.
    fn address(&self, mut block: BlockId) -> u32 {
        for _ in 0..self.module.blocks.len() {
            let body = &self.module.blocks[block];
            if !body.metadata.is_continuation || !body.instructions.is_empty() {
                break;
            }
            let next = match body.terminator.as_ref().map(|term| &term.kind) {
                Some(&TerminatorKind::Jump(target)) => target,
                None => match self.module.next_block(block) {
                    Some(next) => next,
                    None => break,
                },
                Some(_) => break,
            };
            if next == block {
                break;
            }
            block = next;
        }
        self.label(block)
    }

    /// Whether control falls through from the end of `block` to the next block.
    fn falls_through(&self, block: BlockId) -> bool {
        match self.module.blocks[block].terminator.as_ref().map(|term| &term.kind) {
            None => true,
            Some(TerminatorKind::Op(opcode)) => !op::is_terminal(*opcode),
            Some(_) => false,
        }
    }

    /// A block's code as text, with targets by label and data by contents, metadata aside, and
    /// the block it falls through to.
    fn key(&self, block: BlockId) -> String {
        let body = &self.module.blocks[block];
        let mut text = String::new();
        for inst in &body.instructions {
            match (inst.as_stack_op(), inst.pushed_block(), inst.pushed_data()) {
                (Some(stack_op), ..) => write!(text, "{stack_op:?}"),
                (_, Some(target), _) => write!(text, "push bb{}", self.label(target)),
                (_, _, Some(data)) => {
                    write!(text, "push_data {}+{}", self.data_symbol(data.id), data.offset)
                }
                _ => write!(text, "{}/{:02x} {:?}", inst.mnemonic(), inst.opcode, inst.value),
            }
            .unwrap();
            text.push(';');
        }
        match body.terminator.as_ref().map(|term| &term.kind) {
            Some(TerminatorKind::Jump(target)) => write!(text, "jump bb{}", self.label(*target)),
            Some(TerminatorKind::JumpI { then_block, else_block }) => {
                write!(text, "jumpi bb{}, bb{}", self.label(*then_block), self.label(*else_block))
            }
            Some(TerminatorKind::IndexedJump(targets)) => {
                let targets: Vec<_> = targets.iter().map(|&t| self.label(t).to_string()).collect();
                write!(text, "indexed_jump {}", targets.join(","))
            }
            Some(TerminatorKind::Op(opcode)) => write!(text, "op {opcode:02x}"),
            None => Ok(()),
        }
        .unwrap();
        if self.falls_through(block) {
            match self.module.next_block(block) {
                Some(next) => write!(text, " -> bb{}", self.label(next)).unwrap(),
                None => text.push_str(" -> end"),
            }
        }
        text
    }

    /// Whether no call can run before control reaches `target`: no block on a path from the
    /// entry to it makes one, where a jump through an address may reach every block whose address
    /// the program pushes.
    fn fresh_returndata(&self, target: BlockId) -> bool {
        let blocks = &self.module.blocks;
        let is_jump = |inst: &Instruction| {
            !inst.is_encoded_push()
                && inst.as_stack_op().is_none()
                && matches!(inst.opcode, op::JUMP | op::JUMPI)
        };
        let ends_in_jump = |block: BlockId| {
            matches!(
                blocks[block].terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::Op(op::JUMP | op::JUMPI))
            )
        };
        // A pushed address that a jump right after it takes is that jump's target; any other is a
        // return address, which a jump through a word on the stack may take.
        let mut returns = Vec::new();
        for (id, body) in blocks.iter_enumerated() {
            for (index, inst) in body.instructions.iter().enumerate() {
                let Some(pushed) = inst.pushed_block() else { continue };
                let jumped = match body.instructions.get(index + 1) {
                    Some(next) => is_jump(next),
                    None => ends_in_jump(id),
                };
                if !jumped {
                    returns.push(pushed);
                }
            }
        }
        let successors = |block: BlockId| {
            let body = &blocks[block];
            let mut out = Vec::new();
            let mut dynamic = false;
            let jumps = body.instructions.iter().enumerate().filter(|(_, inst)| is_jump(inst));
            let last = ends_in_jump(block).then_some(body.instructions.len());
            for index in jumps.map(|(index, _)| index).chain(last) {
                match index.checked_sub(1).and_then(|index| body.instructions[index].pushed_block())
                {
                    Some(pushed) => out.push(pushed),
                    None => dynamic = true,
                }
            }
            if dynamic {
                out.extend(returns.iter().copied());
            }
            if let Some(term) = &body.terminator {
                term.kind.visit_targets(|target| out.push(target));
            }
            if self.falls_through(block)
                && let Some(next) = self.module.next_block(block)
            {
                out.push(next);
            }
            out
        };
        let mut predecessors: IndexVec<BlockId, Vec<BlockId>> =
            index_vec![Vec::new(); blocks.len()];
        for block in blocks.indices() {
            for successor in successors(block) {
                predecessors[successor].push(block);
            }
        }
        let reach = |seeds: Vec<BlockId>, next: &dyn Fn(BlockId) -> Vec<BlockId>| {
            let mut seen = DenseBitSet::new_empty(blocks.len());
            let mut work = seeds;
            while let Some(block) = work.pop() {
                if seen.insert(block) {
                    work.extend(next(block));
                }
            }
            seen
        };
        let forward = reach(vec![BlockId::ENTRY], &successors);
        let backward = reach(predecessors[target].clone(), &|block| predecessors[block].clone());
        let calls = |body: &Block| {
            let call = |inst: &Instruction| {
                !inst.is_encoded_push()
                    && inst.as_stack_op().is_none()
                    && op::sets_returndata(inst.opcode)
            };
            let terminator = body.terminator.as_ref().map(|term| &term.kind);
            body.instructions.iter().any(call)
                || matches!(terminator, Some(&TerminatorKind::Op(opcode)) if op::sets_returndata(opcode))
        };
        !blocks
            .iter_enumerated()
            .any(|(id, body)| forward.contains(id) && backward.contains(id) && calls(body))
    }

    /// The symbol of a data entry's address, named by its contents.
    fn data_symbol(&self, id: DataId) -> String {
        let data = &self.module.data[id];
        let mut hasher = DefaultHasher::new();
        format!("{:?}{:?}{:?}", data.bytes, data.library_relocations, data.emit_in_runtime)
            .hash(&mut hasher);
        format!("data{:012x}", hasher.finish() & 0xffff_ffff_ffff)
    }

    /// Explores the region from `start` in this version, stopping at `cuts`: its shape, or why
    /// the exploration stopped with the part it explored.
    fn shape(&self, start: BlockId, cuts: &FxHashSet<u32>) -> (Result<(), Explore>, Shape) {
        let mut shape = Shape { nodes: FxIndexMap::default(), order: Vec::new() };
        let mut path = FxHashSet::default();
        let start = Key { block: start, height: 0, labels: Vec::new() };
        let result = self.visit(start, cuts, &mut shape, &mut path).map(drop);
        (result, shape)
    }

    fn visit(
        &self,
        key: Key,
        cuts: &FxHashSet<u32>,
        shape: &mut Shape,
        path: &mut FxHashSet<usize>,
    ) -> Result<usize, Explore> {
        if let Some(index) = shape.nodes.get_index_of(&key) {
            if path.contains(&index) {
                return Err(Explore::Cycle(self.label(key.block)));
            }
            return Ok(index);
        }
        if shape.nodes.len() >= MAX_STATES {
            return Err(Explore::Skip(format!("more than {MAX_STATES} states")));
        }
        let successors = self.successors(&key, cuts).map_err(Explore::Skip)?;
        let (index, _) = shape.nodes.insert_full(key, Vec::new());
        path.insert(index);
        for successor in successors {
            let next = self.visit(successor, cuts, shape, path)?;
            shape.nodes[index].push(next);
        }
        path.remove(&index);
        shape.order.push(index);
        Ok(index)
    }

    /// The points that the block at `key` continues to within the region, tracking only which
    /// stack words are block addresses.
    fn successors(&self, key: &Key, cuts: &FxHashSet<u32>) -> Result<Vec<Key>, String> {
        let mut stack = Stack::from_key(key);
        let mut out = Vec::new();
        let edge = |target: BlockId, stack: &Stack<Option<u32>>, out: &mut Vec<Key>| {
            if !cuts.contains(&self.label(target)) {
                out.push(stack.key(target, |word| *word));
            }
        };
        let resolve = |word: Option<u32>| word.and_then(|label| self.by_label.get(&label).copied());
        let body = &self.module.blocks[key.block];
        for inst in &body.instructions {
            match self.shape_inst(inst, &mut stack)? {
                Control::Next => {}
                Control::JumpIf(target, _) => {
                    if let Some(target) = resolve(target) {
                        edge(target, &stack, &mut out);
                    }
                }
                Control::Jump(target) => {
                    if let Some(target) = resolve(target) {
                        edge(target, &stack, &mut out);
                    }
                    return Ok(out);
                }
                Control::End(_) => return Ok(out),
            }
        }
        match body.terminator.as_ref().map(|term| &term.kind) {
            Some(TerminatorKind::Jump(target)) => edge(*target, &stack, &mut out),
            Some(TerminatorKind::JumpI { then_block, else_block }) => {
                stack.pop(&mut |_| None);
                edge(*then_block, &stack, &mut out);
                edge(*else_block, &stack, &mut out);
            }
            Some(TerminatorKind::IndexedJump(targets)) => {
                stack.pop(&mut |_| None);
                for &target in targets {
                    edge(target, &stack, &mut out);
                }
            }
            Some(&TerminatorKind::Op(opcode)) => match shape_op(opcode, &mut stack)? {
                Control::Jump(target) => {
                    if let Some(target) = resolve(target) {
                        edge(target, &stack, &mut out);
                    }
                }
                Control::JumpIf(target, _) => {
                    if let Some(target) = resolve(target) {
                        edge(target, &stack, &mut out);
                    }
                    if let Some(next) = self.module.next_block(key.block) {
                        edge(next, &stack, &mut out);
                    }
                }
                Control::Next => {
                    if let Some(next) = self.module.next_block(key.block) {
                        edge(next, &stack, &mut out);
                    }
                }
                Control::End(_) => {}
            },
            None => {
                if let Some(next) = self.module.next_block(key.block) {
                    edge(next, &stack, &mut out);
                }
            }
        }
        Ok(out)
    }

    fn shape_inst(
        &self,
        inst: &Instruction,
        stack: &mut Stack<Option<u32>>,
    ) -> Result<Control<Option<u32>>, String> {
        if let Some(stack_op) = inst.as_stack_op() {
            stack.apply(stack_op, &mut |_| None);
            return Ok(Control::Next);
        }
        if inst.is_encoded_push() {
            stack.push(inst.pushed_block().map(|block| self.address(block)));
            return Ok(Control::Next);
        }
        shape_op(inst.opcode, stack)
    }
}

/// How `opcode` moves control, tracking only which stack words are block addresses.
fn shape_op(opcode: u8, stack: &mut Stack<Option<u32>>) -> Result<Control<Option<u32>>, String> {
    let entry = &mut |_| None;
    Ok(match opcode {
        op::JUMPI => {
            let target = stack.pop(entry);
            let condition = stack.pop(entry);
            Control::JumpIf(target, condition)
        }
        op::JUMP => Control::Jump(stack.pop(entry)),
        _ if op::is_terminal(opcode) || op::definition(opcode).is_none() => {
            Control::End(Kind::Invalid)
        }
        _ => {
            let (inputs, outputs) =
                op::stack_io(opcode).ok_or_else(|| format!("opcode 0x{opcode:02x}"))?;
            for _ in 0..inputs {
                stack.pop(entry);
            }
            for _ in 0..outputs {
                stack.push(None);
            }
            Control::Next
        }
    })
}

/// The stack at a point of a region: explicit words above the entry's words from `floor` down.
#[derive(Clone, Debug, Default)]
struct Stack<T> {
    /// The explicit words, bottom first.
    words: Vec<T>,
    /// The entry's words, counting its top as zero, that the explicit words replaced: the word
    /// below them is the entry's word `floor`.
    floor: usize,
}

impl<T: Clone> Stack<T> {
    /// The depth relative to the region's entry.
    fn height(&self) -> isize {
        self.words.len() as isize - self.floor as isize
    }

    /// Makes the top `depth` words explicit, taking the entry's words from `entry`.
    fn reach(&mut self, depth: usize, entry: &mut impl FnMut(usize) -> T) {
        while self.words.len() < depth {
            self.words.insert(0, entry(self.floor));
            self.floor += 1;
        }
    }

    fn push(&mut self, word: T) {
        self.words.push(word);
    }

    fn pop(&mut self, entry: &mut impl FnMut(usize) -> T) -> T {
        self.reach(1, entry);
        self.words.pop().unwrap()
    }

    fn apply(&mut self, stack_op: StackOp, entry: &mut impl FnMut(usize) -> T) {
        self.reach(stack_op.required_depth(), entry);
        let top = self.words.len() - 1;
        match stack_op {
            // dup n: a copy of the nth word on top.
            StackOp::Dup(n) => self.words.push(self.words[top + 1 - usize::from(n)].clone()),
            // swap n: the top and the word n below it trade places.
            StackOp::Swap(n) => self.words.swap(top, top - usize::from(n)),
            // exchange n, m: the words n and m below the top trade places.
            StackOp::Exchange(n, m) => self.words.swap(top - usize::from(n), top - usize::from(m)),
            StackOp::Pop => {
                self.words.pop();
            }
        }
    }

    /// The point at `block` with this stack, whose words `label` tells the block addresses of.
    fn key(&self, block: BlockId, label: impl Fn(&T) -> Option<u32>) -> Key {
        let labels = self
            .words
            .iter()
            .rev()
            .enumerate()
            .filter_map(|(position, word)| Some((position, label(word)?)))
            .collect();
        Key { block, height: self.height(), labels }
    }
}

impl Stack<Option<u32>> {
    /// The stack a point's key describes, with the words it leaves unknown.
    fn from_key(key: &Key) -> Self {
        let top = key.labels.iter().map(|&(position, _)| position + 1).max().unwrap_or(0);
        let len = top.max(key.height.max(0) as usize);
        let mut words = vec![None; len];
        for &(position, label) in &key.labels {
            words[len - 1 - position] = Some(label);
        }
        Self { words, floor: (len as isize - key.height) as usize }
    }
}

/// A path's guard, stack, and memory, storage and world.
type Point = (String, Stack<String>, Frame);

/// The memory, storage and world of a path, and the size of the return data the last call left.
#[derive(Clone)]
struct Frame {
    state: State,
    returndata: String,
}

/// An end of a region: an outcome, or a continuation whose stack still needs aligning.
enum End {
    Outcome(Outcome),
    Continue { guard: String, target: String, stack: Stack<String>, frame: Frame },
}

/// Encodes one region.
#[derive(Default)]
struct Validator {
    encoder: Encoder,
    /// The labels whose addresses the query names.
    labels: FxIndexSet<u32>,
    /// The deepest entry word a stack reached.
    floor: usize,
    /// The words that address known data, with its bytes from there on.
    data: FxHashMap<String, Vec<u8>>,
    /// The target, whose missing opcodes halt.
    evm_version: EvmVersion,
}

impl Validator {
    /// The query whether the region from the shared cut differs between `before` and `after`, or
    /// the reason it cannot be encoded. The `entry` region starts with zeroed memory.
    fn encode(
        mut self,
        entry: bool,
        before: (&Program<'_>, &Shape),
        after: (&Program<'_>, &Shape),
        cuts: &FxHashSet<u32>,
        fresh: bool,
    ) -> Result<Query, String> {
        let state = State {
            memory: self.encoder.declare("mem0", MEMORY),
            storage: self.encoder.declare("sto0", STORAGE),
            transient: self.encoder.declare("tra0", STORAGE),
            world: "world0".into(),
        };
        // No call ran before the region in the version before the pass, so the return data is
        // empty, as it is at the start of every call.
        let returndata =
            if fresh { word(U256::ZERO) } else { self.encoder.declare("returndatasize0", WORD) };
        let start = Frame { state, returndata };
        self.encoder.zeroed = entry;
        self.encoder.read_merges = true;
        // Elsewhere the free-memory pointer lies above the reserved words, as every entry
        // establishes and the memory model keeps.
        if !entry {
            self.encoder.entry_fmp();
        }
        let old = self.run(before, cuts, &start)?;
        let new = self.run(after, cuts, &start)?;
        // Continuations' stacks made explicit to one depth, so they compare word by word.
        let floor = old
            .iter()
            .chain(&new)
            .filter_map(|end| match end {
                End::Continue { stack, .. } => Some(stack.floor),
                End::Outcome(_) => None,
            })
            .max()
            .unwrap_or(0);
        let old = self.group(old, floor);
        let new = self.group(new, floor);
        for index in 0..self.floor.max(floor) {
            self.encoder.declare(&format!("arg{index}"), WORD);
        }
        let mut differences = Vec::new();
        for (key, a) in &old {
            let Some(b) = new.get(key) else {
                differences.push(a.guard.clone());
                continue;
            };
            differences.push(all(&[a.guard.clone(), format!("(not {})", b.guard)]));
            let differ = self.differ(key, a, b);
            if differ != "false" {
                differences.push(all(&[a.guard.as_str(), b.guard.as_str(), differ.as_str()]));
            }
        }
        if self.labels.len() > 1 {
            let labels: Vec<_> = self.labels.iter().map(|&label| label_term(label)).collect();
            self.encoder.assert(&format!("(distinct {})", labels.join(" ")));
        }
        self.encoder.assert(&any(&differences));
        Ok(self.encoder.finish())
    }

    /// Runs one version of the region from `start` and returns its ends.
    fn run(
        &mut self,
        (program, shape): (&Program<'_>, &Shape),
        cuts: &FxHashSet<u32>,
        start: &Frame,
    ) -> Result<Vec<End>, String> {
        let mut incoming: Vec<Vec<Point>> = vec![Vec::new(); shape.nodes.len()];
        incoming[0].push(("true".into(), Stack::default(), start.clone()));
        let mut ends = Vec::new();
        for &index in shape.order.iter().rev() {
            let arms = std::mem::take(&mut incoming[index]);
            if arms.is_empty() {
                continue;
            }
            let block = shape.nodes.get_index(index).unwrap().0.block;
            let point = self.merge(arms)?;
            let mut walk = Walk { program, shape, cuts, incoming: &mut incoming, ends: &mut ends };
            self.block(&mut walk, block, point)?;
        }
        Ok(ends)
    }

    /// The point the paths `arms` join at: the disjunction of their guards, and their stacks and
    /// states as `ite` chains over them.
    fn merge(&mut self, mut arms: Vec<Point>) -> Result<Point, String> {
        if arms.len() == 1 {
            return Ok(arms.pop().unwrap());
        }
        let floor = arms.iter().map(|(_, stack, _)| stack.floor).max().unwrap();
        for (_, stack, _) in &mut arms {
            let depth = stack.words.len() + floor - stack.floor;
            stack.reach(depth, &mut entry_word);
        }
        let len = arms[0].1.words.len();
        if arms.iter().any(|(_, stack, _)| stack.words.len() != len) {
            return Err("paths join with different stack depths".into());
        }
        let guards: Vec<_> = arms.iter().map(|(guard, ..)| guard.as_str()).collect();
        let guard = self.encoder.define(BOOL, any(&guards));
        let mut words = Vec::with_capacity(len);
        for slot in 0..len {
            let options =
                arms.iter().map(|(guard, stack, _)| (guard.clone(), stack.words[slot].clone()));
            words.push(self.encoder.ite(WORD, options.collect()));
        }
        let mut field = |sort: &'static str, get: fn(&Frame) -> &String| {
            let options = arms.iter().map(|(guard, _, frame)| (guard.clone(), get(frame).clone()));
            self.encoder.ite(sort, options.collect())
        };
        let state = State {
            memory: field(MEMORY, |frame| &frame.state.memory),
            storage: field(STORAGE, |frame| &frame.state.storage),
            transient: field(STORAGE, |frame| &frame.state.transient),
            world: field(WORLD, |frame| &frame.state.world),
        };
        let returndata = field(WORD, |frame| &frame.returndata);
        Ok((guard, Stack { words, floor }, Frame { state, returndata }))
    }

    /// Runs `block` from `point`, passing its paths on to the points it continues to.
    fn block(
        &mut self,
        walk: &mut Walk<'_, '_>,
        block: BlockId,
        point: Point,
    ) -> Result<(), String> {
        let (mut guard, mut stack, mut state) = point;
        let program = walk.program;
        let body = &program.module.blocks[block];
        for inst in &body.instructions {
            let control = if let Some(stack_op) = inst.as_stack_op() {
                stack.apply(stack_op, &mut entry_word);
                Control::Next
            } else if inst.is_encoded_push() {
                let word = self.pushed(program, inst)?;
                stack.push(word);
                Control::Next
            } else {
                self.opcode(inst.opcode, &mut stack, &mut state, &guard)?
            };
            self.note_floor(&stack);
            match control {
                Control::Next => {}
                Control::JumpIf(target, condition) => {
                    let (taken, not_taken) = self.split(&guard, &condition);
                    self.jump(walk, &target, taken, &stack, &state)?;
                    guard = not_taken;
                }
                Control::Jump(target) => return self.jump(walk, &target, guard, &stack, &state),
                Control::End(kind) => {
                    walk.ends.push(End::Outcome(Outcome { guard, kind, state: state.state }));
                    return Ok(());
                }
            }
        }
        let fall_through = |this: &mut Self,
                            walk: &mut Walk<'_, '_>,
                            guard: String,
                            stack: &Stack<String>,
                            state: &Frame| {
            match program.module.next_block(block) {
                Some(next) => this.edge(walk, next, guard, stack, state),
                None => {
                    let zero = word(U256::ZERO);
                    let kind = Kind::ReturnData { offset: zero.clone(), size: zero };
                    let state = state.state.clone();
                    walk.ends.push(End::Outcome(Outcome { guard, kind, state }));
                    Ok(())
                }
            }
        };
        match body.terminator.as_ref().map(|term| &term.kind) {
            None => fall_through(self, walk, guard, &stack, &state),
            Some(&TerminatorKind::Jump(target)) => self.edge(walk, target, guard, &stack, &state),
            Some(&TerminatorKind::JumpI { then_block, else_block }) => {
                let condition = stack.pop(&mut entry_word);
                self.note_floor(&stack);
                if then_block == else_block {
                    return self.edge(walk, then_block, guard, &stack, &state);
                }
                let (taken, not_taken) = self.split(&guard, &condition);
                self.edge(walk, then_block, taken, &stack, &state)?;
                self.edge(walk, else_block, not_taken, &stack, &state)
            }
            Some(TerminatorKind::IndexedJump(targets)) => {
                let index = stack.pop(&mut entry_word);
                self.note_floor(&stack);
                // Lowering checks no bounds: the code that computes the index keeps it in range.
                self.encoder.axioms.insert("indexed-jump");
                let size = word(U256::from(targets.len()));
                let bound = format!("(bvult {index} {size})");
                self.encoder.assert(&implies(&guard, &bound));
                for (position, &target) in targets.iter().enumerate() {
                    let case = format!("(= {index} {})", word(U256::from(position)));
                    let taken = self.encoder.define(BOOL, and(&guard, &case));
                    self.edge(walk, target, taken, &stack, &state)?;
                }
                Ok(())
            }
            Some(&TerminatorKind::Op(opcode)) => {
                let control = self.opcode(opcode, &mut stack, &mut state, &guard)?;
                self.note_floor(&stack);
                match control {
                    Control::Next => fall_through(self, walk, guard, &stack, &state),
                    Control::JumpIf(target, condition) => {
                        let (taken, not_taken) = self.split(&guard, &condition);
                        self.jump(walk, &target, taken, &stack, &state)?;
                        fall_through(self, walk, not_taken, &stack, &state)
                    }
                    Control::Jump(target) => self.jump(walk, &target, guard, &stack, &state),
                    Control::End(kind) => {
                        walk.ends.push(End::Outcome(Outcome { guard, kind, state: state.state }));
                        Ok(())
                    }
                }
            }
        }
    }

    fn note_floor(&mut self, stack: &Stack<String>) {
        self.floor = self.floor.max(stack.floor);
    }

    /// The guards of a branch on `condition` under `guard`: taken, and not taken.
    fn split(&mut self, guard: &str, condition: &str) -> (String, String) {
        let zero = word(U256::ZERO);
        match parse_word(condition) {
            Some(value) if value.is_zero() => ("false".into(), guard.to_owned()),
            Some(_) => (guard.to_owned(), "false".into()),
            None => {
                let taken = format!("(not (= {condition} {zero}))");
                let not_taken = format!("(= {condition} {zero})");
                (
                    self.encoder.define(BOOL, and(guard, &taken)),
                    self.encoder.define(BOOL, and(guard, &not_taken)),
                )
            }
        }
    }

    /// A jump to the address `target`: to its block when it is one the region pushed, and
    /// otherwise a continuation there.
    fn jump(
        &mut self,
        walk: &mut Walk<'_, '_>,
        target: &str,
        guard: String,
        stack: &Stack<String>,
        state: &Frame,
    ) -> Result<(), String> {
        if let Some(label) = target.strip_prefix("lbl").and_then(|label| label.parse::<u32>().ok())
            && let Some(&block) = walk.program.by_label.get(&label)
        {
            return self.edge(walk, block, guard, stack, state);
        }
        if guard != "false" {
            let (target, stack, frame) = (target.to_owned(), stack.clone(), state.clone());
            walk.ends.push(End::Continue { guard, target, stack, frame });
        }
        Ok(())
    }

    /// Control reaching `target` with this path: a continuation at a cut, or an arm of the point
    /// it reaches within the region.
    fn edge(
        &mut self,
        walk: &mut Walk<'_, '_>,
        target: BlockId,
        guard: String,
        stack: &Stack<String>,
        state: &Frame,
    ) -> Result<(), String> {
        if guard == "false" {
            return Ok(());
        }
        let label = walk.program.label(target);
        if walk.cuts.contains(&label) {
            let target = self.label(label);
            let (stack, frame) = (stack.clone(), state.clone());
            walk.ends.push(End::Continue { guard, target, stack, frame });
            return Ok(());
        }
        let key = stack.key(target, |word| {
            word.strip_prefix("lbl").and_then(|label| label.parse::<u32>().ok())
        });
        let index = walk
            .shape
            .nodes
            .get_index_of(&key)
            .ok_or("a path reaches a point the exploration did not")?;
        walk.incoming[index].push((guard, stack.clone(), state.clone()));
        Ok(())
    }

    /// The address of the block labeled `label`.
    fn label(&mut self, label: u32) -> String {
        self.labels.insert(label);
        self.encoder.declare(&label_term(label), WORD)
    }

    /// The word a push instruction pushes.
    fn pushed(&mut self, program: &Program<'_>, inst: &Instruction) -> Result<String, String> {
        if let Some(id) = inst.immutable_push() {
            return Ok(self.encoder.declare(&format!("immutable{}", id.index()), WORD));
        }
        if let Some(library) = inst.pushed_library() {
            return Ok(self.encoder.declare(&format!("library{}", library.index()), WORD));
        }
        if let Some(block) = inst.pushed_block() {
            let address = program.address(block);
            if address != program.label(block) {
                self.encoder.axioms.insert("continuation");
            }
            return Ok(self.label(address));
        }
        if let Some(data) = inst.pushed_data() {
            let base = self.encoder.declare(&program.data_symbol(data.id), WORD);
            let term = if data.offset == 0 {
                base
            } else {
                let offset = word(U256::from(data.offset));
                self.encoder.define(WORD, format!("(bvadd {base} {offset})"))
            };
            let entry = &program.module.data[data.id];
            if let DataBytes::Known(bytes) = &entry.bytes
                && entry.library_relocations.is_empty()
                && let Some(rest) = bytes.get(data.offset as usize..)
            {
                self.data.insert(term.clone(), rest.to_vec());
            }
            return Ok(term);
        }
        if let Some(size) = inst.pushed_data_size() {
            return Ok(match &program.module.data[size.data].bytes {
                DataBytes::Known(bytes) => word(size.value(bytes.len())),
                DataBytes::Deferred(_) => {
                    let base = self.encoder.declare(&program.data_symbol(size.data), WORD);
                    let length = self.encoder.apply("data_size", &[(base, WORD)], WORD);
                    let addend = word(U256::from(size.addend));
                    let size_term = self.encoder.define(WORD, format!("(bvadd {length} {addend})"));
                    if size.aligned {
                        let mask = word(!U256::from(31));
                        self.encoder.define(WORD, format!("(bvand {size_term} {mask})"))
                    } else {
                        size_term
                    }
                }
            });
        }
        match inst.concrete_immediate() {
            Some(value) => Ok(word(value)),
            None => Err("an unresolved relocation".into()),
        }
    }

    /// Runs a non-push opcode on the stack and state.
    fn opcode(
        &mut self,
        opcode: u8,
        stack: &mut Stack<String>,
        frame: &mut Frame,
        guard: &str,
    ) -> Result<Control<String>, String> {
        let Frame { state, returndata } = frame;
        let Some(definition) =
            op::definition(opcode).filter(|definition| definition.is_available(self.evm_version))
        else {
            // An opcode the target lacks halts like `invalid`.
            return Ok(Control::End(Kind::Invalid));
        };
        let mnemonic = definition.mnemonic;
        let Some((inputs, outputs)) = definition.stack_io else {
            return Err(format!("`{mnemonic}` without its immediate"));
        };
        // Operands in pop order: the top of the stack first.
        let operands: Vec<String> =
            (0..inputs).map(|_| stack.pop(&mut entry_word)).collect::<Vec<_>>();
        let zero = word(U256::ZERO);
        let words = |operands: &[String]| -> Vec<(String, &'static str)> {
            operands.iter().map(|operand| (operand.clone(), WORD)).collect()
        };
        let result = match (opcode, &operands[..]) {
            (op::JUMPI, [target, condition]) => {
                return Ok(Control::JumpIf(target.clone(), condition.clone()));
            }
            (op::JUMP, [target]) => return Ok(Control::Jump(target.clone())),
            (op::STOP, []) => {
                return Ok(Control::End(Kind::ReturnData { offset: zero.clone(), size: zero }));
            }
            (op::RETURN, [offset, size]) => {
                let (offset, size) = (offset.clone(), size.clone());
                return Ok(Control::End(Kind::ReturnData { offset, size }));
            }
            (op::REVERT, [offset, size]) => {
                let (offset, size) = (offset.clone(), size.clone());
                return Ok(Control::End(Kind::Revert { offset, size }));
            }
            (op::INVALID, []) => return Ok(Control::End(Kind::Invalid)),
            (op::SELFDESTRUCT, [recipient]) => {
                return Ok(Control::End(Kind::SelfDestruct(recipient.clone())));
            }
            (op::JUMPDEST, []) => return Ok(Control::Next),
            (op::PC, _) => return Err("`pc` is not modeled".into()),
            (op::PUSH0, []) => zero,
            (op::MLOAD, [address]) => {
                self.encoder.access(guard, address, 32);
                self.encoder.load(&state.memory, address)
            }
            (op::MSTORE | op::MSTORE8, [address, value]) => {
                let size = if opcode == op::MSTORE { 32 } else { 1 };
                self.encoder.access(guard, address, size);
                state.memory = self.encoder.store(&state.memory, address, value, size);
                String::new()
            }
            (op::SLOAD, [slot]) => {
                self.encoder.define(WORD, format!("(select {} {slot})", state.storage))
            }
            (op::TLOAD, [slot]) => {
                self.encoder.define(WORD, format!("(select {} {slot})", state.transient))
            }
            (op::SSTORE, [slot, value]) => {
                let storage = format!("(store {} {slot} {value})", state.storage);
                state.storage = self.encoder.define(STORAGE, storage);
                String::new()
            }
            (op::TSTORE, [slot, value]) => {
                let transient = format!("(store {} {slot} {value})", state.transient);
                state.transient = self.encoder.define(STORAGE, transient);
                String::new()
            }
            (op::KECCAK256, [offset, size]) => match parse_word(size) {
                Some(size) if size % U256::from(32) == U256::ZERO && size <= U256::from(256) => {
                    let words: Vec<_> = (0..size.to::<u64>() / 32)
                        .map(|index| {
                            let address =
                                format!("(bvadd {offset} {})", word(U256::from(index * 32)));
                            (self.encoder.load(&state.memory, &address), WORD)
                        })
                        .collect();
                    self.encoder.apply("keccak", &words, WORD)
                }
                _ => {
                    let arguments = [
                        (state.memory.clone(), MEMORY),
                        (offset.clone(), WORD),
                        (size.clone(), WORD),
                    ];
                    self.encoder.apply("keccak_memory", &arguments, WORD)
                }
            },
            (op::CODECOPY, [destination, source, size])
                if let Some(bytes) = self.data.get(source)
                    && let Some(size) = parse_word(size)
                    && size <= U256::from(bytes.len().min(MAX_COPY)) =>
            {
                // A copy of known data writes its bytes.
                let bytes = bytes[..size.to::<usize>()].to_vec();
                self.write_bytes(state, destination, &bytes);
                String::new()
            }
            (op::CALLDATACOPY, [destination, source, size])
                if source == "input_calldatasize_0"
                    && let Some(size) = parse_word(size)
                    && size <= U256::from(MAX_COPY) =>
            {
                // Calldata reads zeros past its end.
                let zeros = vec![0; size.to::<usize>()];
                self.write_bytes(state, destination, &zeros);
                String::new()
            }
            (op::MCOPY, [destination, source, size])
                if let Some(size) = parse_word(size)
                    && size <= U256::from(MAX_COPY) =>
            {
                // Every byte comes from memory before the copy, as for overlapping ranges.
                let size = size.to::<usize>();
                let before = state.memory.clone();
                for start in (0..size).step_by(32) {
                    let from = offset_term(source, start);
                    let from = self.encoder.define(WORD, from);
                    let to = offset_term(destination, start);
                    let to = self.encoder.define(WORD, to);
                    if size - start >= 32 {
                        let value = self.encoder.load(&before, &from);
                        state.memory = self.encoder.store(&state.memory, &to, &value, 32);
                        continue;
                    }
                    for index in start..size {
                        let from = self.encoder.define(WORD, offset_term(source, index));
                        let to = self.encoder.define(WORD, offset_term(destination, index));
                        let byte = format!("((_ zero_extend 248) (select {before} {from}))");
                        let value = self.encoder.define(WORD, byte);
                        state.memory = self.encoder.store(&state.memory, &to, &value, 1);
                    }
                }
                String::new()
            }
            (
                op::CALLDATACOPY | op::CODECOPY | op::RETURNDATACOPY | op::MCOPY | op::EXTCODECOPY,
                _,
            ) => {
                let mut arguments =
                    vec![(state.world.clone(), WORLD), (state.memory.clone(), MEMORY)];
                arguments.extend(words(&operands));
                let memory = state.memory.clone();
                state.memory = self.encoder.apply(&format!("copy_{mnemonic}"), &arguments, MEMORY);
                let destination =
                    if opcode == op::EXTCODECOPY { &operands[1] } else { &operands[0] };
                self.encoder.region(&state.memory, memory, destination);
                String::new()
            }
            (op::LOG0..=op::LOG4, _) => {
                let mut arguments = vec![(state.world.clone(), WORLD)];
                arguments.extend(words(&operands));
                arguments.push((state.memory.clone(), MEMORY));
                state.world = self.encoder.apply(&format!("ev_{mnemonic}"), &arguments, WORLD);
                String::new()
            }
            (op::RETURNDATASIZE, []) => returndata.clone(),
            _ if op::sets_returndata(opcode) => {
                let world = self.encoder.event(&format!("ev_{mnemonic}"), operands.clone(), state);
                let after = [(world, WORLD)];
                *returndata = self.encoder.apply("returndata_size", &after, WORD);
                self.encoder.apply("result", &after, WORD)
            }
            _ if op::is_pure(opcode) => self.encoder.pure(opcode, mnemonic, &operands),
            _ if op::is_tx_input(opcode) => {
                self.encoder.apply(&format!("input_{mnemonic}"), &words(&operands), WORD)
            }
            _ if outputs == 1 => {
                let mut arguments = vec![(state.world.clone(), WORLD)];
                arguments.extend(words(&operands));
                self.encoder.apply(&format!("env_{mnemonic}"), &arguments, WORD)
            }
            _ => return Err(format!("`{mnemonic}` is not modeled")),
        };
        if outputs == 1 {
            if result.is_empty() {
                return Err(format!("`{mnemonic}` without a result"));
            }
            // An uninterpreted result fits the opcode's result width.
            let bits = u32::from(definition.result_bits);
            if bits < 256 && !op::is_pure(opcode) && parse_word(&result).is_none() {
                self.encoder.axioms.insert("result-type");
                let mask = word((U256::ONE << bits) - U256::ONE);
                self.encoder.assert(&format!("(bvule {result} {mask})"));
            }
            stack.push(result);
        }
        Ok(Control::Next)
    }

    /// Writes `bytes` to memory at `destination`: words, then single bytes for the rest.
    fn write_bytes(&mut self, state: &mut State, destination: &str, bytes: &[u8]) {
        for (index, chunk) in bytes.chunks(32).enumerate() {
            if chunk.len() == 32 {
                let address = self.encoder.define(WORD, offset_term(destination, index * 32));
                let value = word(U256::from_be_slice(chunk));
                state.memory = self.encoder.store(&state.memory, &address, &value, 32);
                continue;
            }
            for (byte_index, &byte) in chunk.iter().enumerate() {
                let offset = index * 32 + byte_index;
                let address = self.encoder.define(WORD, offset_term(destination, offset));
                let value = word(U256::from(byte));
                state.memory = self.encoder.store(&state.memory, &address, &value, 1);
            }
        }
    }

    /// Each version's ends merged per kind, and for a continuation per target block and depth.
    fn group(&mut self, ends: Vec<End>, floor: usize) -> FxIndexMap<String, Outcome> {
        let mut groups: FxIndexMap<String, Vec<Outcome>> = FxIndexMap::default();
        for end in ends {
            let outcome = match end {
                End::Outcome(outcome) => outcome,
                End::Continue { guard, target, mut stack, frame } => {
                    let depth = stack.words.len() + floor - stack.floor;
                    stack.reach(depth, &mut entry_word);
                    // The return data's size goes on with the stack.
                    let mut words = vec![frame.returndata];
                    words.extend(stack.words);
                    Outcome {
                        guard,
                        kind: Kind::Continue { target, stack: words },
                        state: frame.state,
                    }
                }
            };
            // Data of a constant size compares word by word, which reads through the stores that
            // wrote it.
            let mut outcome = outcome;
            let data = match &outcome.kind {
                Kind::ReturnData { offset, size } => Some((false, offset.clone(), size.clone())),
                Kind::Revert { offset, size } => Some((true, offset.clone(), size.clone())),
                _ => None,
            };
            if let Some((revert, offset, size)) = data
                && let Some(size) = parse_word(&size).filter(|&size| size <= U256::from(MAX_DATA))
            {
                let words = self.data_words(&outcome.state.memory, &offset, size.to::<u64>());
                outcome.kind = Kind::Return(words);
                let kind = if revert { "revert" } else { "return" };
                groups.entry(format!("{kind} {size}")).or_default().push(outcome);
                continue;
            }
            let key = match &outcome.kind {
                Kind::Continue { target, stack } if target.starts_with("lbl") => {
                    format!("goto {target} {}", stack.len())
                }
                Kind::Continue { stack, .. } => format!("dynamic {}", stack.len()),
                Kind::ReturnData { .. } => "return".into(),
                Kind::Revert { .. } => "revert".into(),
                Kind::SelfDestruct(_) => "selfdestruct".into(),
                Kind::Invalid => "invalid".into(),
                Kind::Return(_) | Kind::RevertReturndata | Kind::Ended => "other".into(),
            };
            groups.entry(key).or_default().push(outcome);
        }
        groups.into_iter().map(|(key, outcomes)| (key, self.join(outcomes))).collect()
    }

    /// The words of the `size` bytes of `memory` at `offset`, the last one with the bytes past
    /// the end cleared.
    fn data_words(&mut self, memory: &str, offset: &str, size: u64) -> Vec<String> {
        (0..size.div_ceil(32))
            .map(|index| {
                let address = match parse_word(offset) {
                    Some(offset) => word(offset.wrapping_add(U256::from(index * 32))),
                    None if index == 0 => offset.to_owned(),
                    None => self
                        .encoder
                        .define(WORD, format!("(bvadd {offset} {})", word(U256::from(index * 32)))),
                };
                let value = self.encoder.load(memory, &address);
                let rest = size - index * 32;
                if rest >= 32 {
                    return value;
                }
                let mask = word(!(U256::MAX >> (8 * rest)));
                self.encoder.define(WORD, format!("(bvand {value} {mask})"))
            })
            .collect()
    }

    /// Whether two merged outcomes of the group `key` differ: word by word for data of a
    /// constant size, with storage, transient storage and the world for returned data.
    fn differ(&mut self, key: &str, a: &Outcome, b: &Outcome) -> String {
        let (Kind::Return(left), Kind::Return(right)) = (&a.kind, &b.kind) else {
            return self.encoder.differ(a, b);
        };
        let mut parts = Vec::new();
        for (left, right) in left.iter().zip(right) {
            if left != right {
                parts.push(format!("(not (= {left} {right}))"));
            }
        }
        if !key.starts_with("revert") {
            let (x, y) = (&a.state, &b.state);
            for (left, right) in
                [(&x.storage, &y.storage), (&x.transient, &y.transient), (&x.world, &y.world)]
            {
                if left != right {
                    parts.push(format!("(not (= {left} {right}))"));
                }
            }
        }
        any(&parts)
    }

    /// Outcomes of one kind and shape as one, under the disjunction of their guards.
    fn join(&mut self, mut outcomes: Vec<Outcome>) -> Outcome {
        if outcomes.len() == 1 {
            return outcomes.pop().unwrap();
        }
        let guards: Vec<_> = outcomes.iter().map(|outcome| outcome.guard.clone()).collect();
        let guard = self.encoder.define(BOOL, any(&guards));
        let mut pick = |sort: &'static str, get: &dyn Fn(&Outcome) -> String| {
            let options = outcomes.iter().map(|outcome| (outcome.guard.clone(), get(outcome)));
            self.encoder.ite(sort, options.collect())
        };
        let state = State {
            memory: pick(MEMORY, &|outcome| outcome.state.memory.clone()),
            storage: pick(STORAGE, &|outcome| outcome.state.storage.clone()),
            transient: pick(STORAGE, &|outcome| outcome.state.transient.clone()),
            world: pick(WORLD, &|outcome| outcome.state.world.clone()),
        };
        let field = |outcome: &Outcome, index: usize| match &outcome.kind {
            Kind::Continue { target, stack } => {
                if index == 0 {
                    target.clone()
                } else {
                    stack[index - 1].clone()
                }
            }
            Kind::ReturnData { offset, size } | Kind::Revert { offset, size } => {
                if index == 0 {
                    offset.clone()
                } else {
                    size.clone()
                }
            }
            Kind::SelfDestruct(recipient) => recipient.clone(),
            Kind::Return(words) => words[index].clone(),
            _ => String::new(),
        };
        let kind = match &outcomes[0].kind {
            Kind::Continue { stack, .. } => {
                let len = stack.len();
                let target = pick(WORD, &|outcome| field(outcome, 0));
                let stack = (1..=len).map(|index| pick(WORD, &|outcome| field(outcome, index)));
                Kind::Continue { target, stack: stack.collect() }
            }
            Kind::ReturnData { .. } => Kind::ReturnData {
                offset: pick(WORD, &|outcome| field(outcome, 0)),
                size: pick(WORD, &|outcome| field(outcome, 1)),
            },
            Kind::Revert { .. } => Kind::Revert {
                offset: pick(WORD, &|outcome| field(outcome, 0)),
                size: pick(WORD, &|outcome| field(outcome, 1)),
            },
            Kind::SelfDestruct(_) => Kind::SelfDestruct(pick(WORD, &|outcome| field(outcome, 0))),
            Kind::Return(words) => {
                let words =
                    (0..words.len()).map(|index| pick(WORD, &|outcome| field(outcome, index)));
                Kind::Return(words.collect())
            }
            _ => Kind::Invalid,
        };
        Outcome { guard, kind, state }
    }
}

/// The region and the paths into its points, while one version runs.
struct Walk<'a, 'p> {
    program: &'a Program<'p>,
    shape: &'a Shape,
    cuts: &'a FxHashSet<u32>,
    incoming: &'a mut Vec<Vec<Point>>,
    ends: &'a mut Vec<End>,
}

/// The address `offset` bytes past `base`, folded when `base` is a constant.
fn offset_term(base: &str, offset: usize) -> String {
    match parse_word(base) {
        Some(base) => word(base.wrapping_add(U256::from(offset))),
        None if offset == 0 => base.to_owned(),
        None => format!("(bvadd {base} {})", word(U256::from(offset))),
    }
}

/// The symbol of the region entry's stack word `index`, counting its top as zero.
fn entry_word(index: usize) -> String {
    format!("arg{index}")
}

/// The symbol of the address of the block labeled `label`.
fn label_term(label: u32) -> String {
    format!("lbl{label}")
}

/// The implication of `b` by `a`.
fn implies(a: &str, b: &str) -> String {
    if a == "true" { b.to_owned() } else { format!("(=> {a} {b})") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{ColorChoice, Session, source_map::FileName};
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    /// Z3's verdict on each region query between two versions of a program, `None` for a
    /// skipped region, or no verdicts without Z3.
    fn verdicts(before: &str, after: &str) -> Option<Vec<Option<String>>> {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let queries = sess.enter_sequential(|| {
            let parse = |name: &str, text: &str| {
                let file =
                    sess.source_map().new_source_file(FileName::Custom(name.into()), text).unwrap();
                Module::parse(&sess, &file).unwrap()
            };
            let (before, after) = (parse("before.evmir", before), parse("after.evmir", after));
            regions("test", &before, &after, EvmVersion::default())
                .into_iter()
                .map(|(_, query, _)| query.ok().map(|query| query.text))
                .collect::<Vec<_>>()
        });
        let z3 = std::env::var_os("SOLAR_PROVE_ANALYSES_Z3").unwrap_or_else(|| "z3".into());
        Command::new(&z3).arg("--version").output().ok()?;
        let solve = |text: String| {
            let mut child = Command::new(&z3)
                .args(["-in", "-smt2", "-memory:1500", "-T:60"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
            let output = child.wait_with_output().unwrap();
            String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or_default().to_owned()
        };
        Some(queries.into_iter().map(|query| query.map(solve)).collect())
    }

    const BEFORE: &str = "@module Pair
bb0:
  push 0
  calldataload
  dup 1
  iszero
  push 1
  mul
  push 0
  mstore
  jump bb1
bb1:
  push 32
  push 0
  return
";

    #[test]
    fn proves_a_rewrite() {
        // push 1; mul => (nothing)
        let after = BEFORE.replace("  push 1\n  mul\n", "");
        if let Some(verdicts) = verdicts(BEFORE, &after) {
            assert_eq!(verdicts, [Some("unsat".to_owned())]);
        }
    }

    #[test]
    fn refutes_a_wrong_rewrite() {
        // iszero; push 1; mul => (nothing): the stored word is no longer the test.
        let after = BEFORE.replace("  iszero\n  push 1\n  mul\n", "");
        if let Some(verdicts) = verdicts(BEFORE, &after) {
            assert_eq!(verdicts, [Some("sat".to_owned())]);
        }
    }

    #[test]
    fn follows_a_merged_tail() {
        // Both paths share the tail that each ran on its own.
        let before = "@module Merged
bb0:
  push 0
  calldataload
  jumpi bb1, bb2
bb1:
  push 1
  push 0
  sstore
  stop
bb2:
  push 1
  push 0
  sstore
  stop
";
        let after =
            before.replace("bb2:\n  push 1\n  push 0\n  sstore\n  stop\n", "bb2:\n  jump bb1\n");
        if let Some(verdicts) = verdicts(before, &after) {
            assert_eq!(verdicts, [Some("unsat".to_owned())]);
        }
    }
}
