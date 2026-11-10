//! AFL++-style edge coverage and comparison operand logging (CmpLog) instrumentation for EVM IR,
//! as a native pass plugin.
//!
//! Build with `cargo build -p solar-edgecov-plugin` in the compiler's profile and load it with
//! `solar -Zload-pass-plugin=target/<profile>/libsolar_edgecov_plugin.so`. Its
//! `edgecov::instrument` pass is enabled by default, so every EVM IR module of the compilation,
//! deployment and runtime code alike, is instrumented at the plugin extension point, before the EVM
//! IR optimization pipeline.
//!
//! The algorithm is LLVM SanitizerCoverage's edge level (AFL++ PCGUARD) on a CFG whose critical
//! edges are split. EVM IR blocks may branch in their middle, as `push <target>; jumpi`, so the
//! CFG's basic blocks are the IR blocks cut after each `jumpi`. An edge is critical when its
//! source has several successors and its target several predecessors; the pass gives each such
//! edge its own block, `probe; jump target`, and redirects the branch there, by its pushed address
//! or its terminator. It then probes every conditional edge: after each `jumpi` that more
//! instructions follow, at the start of each single-predecessor branch target, and in each split
//! edge, plus the module entry. Every unconditional edge is implied by the probed edge or entry
//! that leads to it, as with SanitizerCoverage's block pruning, so the probes hit identify the
//! edges taken, without the collisions of AFL's hashed `prev ^ cur` edge IDs.
//! `SOLAR_EDGECOV_MODE=blocks` also probes every block start, the unpruned edge level.
//!
//! A probe is `push <id>; edgecov`, where `edgecov` is the instrumentation opcode `0xc0`: it pops
//! the edge ID and records a hit in an instrumented interpreter, which also refunds the push, so
//! the probe is gas-neutral there. A normal EVM rejects it as an undefined opcode, so instrumented
//! bytecode is for fuzzing and coverage only. The host glues each push to its `edgecov`, so
//! optimizations keep the pair intact, and distinct edge IDs keep probed blocks distinct, so tail
//! merging, block deduplication and outlining cannot merge two probed paths. Later passes may
//! still merge a block into its only predecessor, leaving two probes in a row.
//!
//! The same pass also logs comparison operands, as AFL++'s CMPLOG does: directly before every
//! `eq`, `lt`, `gt`, `slt` and `sgt` (not `iszero`, which compares with a constant) it inserts
//! `push <word>; cmplog`, where `cmplog` is the instrumentation opcode `0xc1` and the word is
//! `(site ID << 8) | comparison opcode`, so the runtime knows the comparison's kind even when later
//! passes move code between the probe and the comparison. `cmplog` pops the word and leaves the two
//! words below it in place, so an instrumented interpreter records the comparison's operands in the
//! order it pops them, the top one first, and the comparison runs on the same words. A comparison
//! the host protects, glued to a neighbour, is skipped. Like `edgecov`, the pair is glued and
//! opaque to the optimizer, which therefore no longer folds or fuses an instrumented comparison
//! with its operands. Set `SOLAR_EDGECOV_CMPLOG=0` to emit edge probes only.
//!
//! Edge and comparison site IDs are dense, start at 1 and are unique across every module one
//! compiler process instruments: they come from one process-global counter, each module taking its
//! edge probes, then its split edges, then its comparison sites. ID 0 is never emitted. Set
//! `SOLAR_EDGECOV_MAP=<path>` to append one line per ID, `<id> <module> <block label> <kind>`,
//! where kind is `block`, `after-jumpi:<instruction index>`, `edge:<target label>` for a split
//! critical edge, or `cmp:<opcode name>:<instruction index>` for a comparison site, with labels and
//! indices of the uninstrumented module.
//!
//! Dynamic jumps (returns, jumps to computed addresses) and fallthroughs out of non-halting
//! terminal opcodes cannot be redirected, so their targets' own probes stand for all of them; a
//! branch into such a target is still split. Modules compiled in parallel draw from the shared
//! counter in scheduling order, so IDs are unique but reproducible only with `-j1`.

use solar_pass_plugin::{
    API_VERSION, Edit, EvmBlock, EvmTerminator, Ir, Manifest, NewEvmBlock, NewEvmInstruction, Pass,
    Plugin, Request, Response, Snapshot,
};
use std::{
    collections::HashSet,
    fmt::Write as _,
    fs::OpenOptions,
    io::Write as _,
    sync::atomic::{AtomicU32, Ordering},
};

/// The instrumentation opcode that records an edge hit.
pub const EDGECOV: u8 = 0xc0;

/// The instrumentation opcode that records a comparison's operands.
pub const CMPLOG: u8 = 0xc1;

/// The comparisons whose operands are logged, by opcode: `eq`, `lt`, `gt`, `slt` and `sgt`.
pub const COMPARISONS: [(u8, &str); 5] =
    [(0x14, "eq"), (0x10, "lt"), (0x11, "gt"), (0x12, "slt"), (0x13, "sgt")];

/// Environment variable naming a file to which each edge ID's origin is appended.
pub const MAP_ENV: &str = "SOLAR_EDGECOV_MAP";

/// Environment variable selecting the probe placement: `branches` (default) or `blocks`.
pub const MODE_ENV: &str = "SOLAR_EDGECOV_MODE";

/// Environment variable that disables comparison operand logging when set to `0`.
pub const CMPLOG_ENV: &str = "SOLAR_EDGECOV_CMPLOG";

const JUMPI: u8 = 0x57;

/// The next edge or comparison site ID to hand out. ID 0 is reserved.
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

struct EdgeCoverage;

impl Plugin for EdgeCoverage {
    fn manifest(&self) -> Manifest {
        Manifest {
            api_version: API_VERSION,
            name: "edgecov".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            passes: vec![Pass {
                name: "instrument".into(),
                ir: Ir::EvmIr,
                phase: None,
                enabled_by_default: true,
            }],
        }
    }

    fn run(&self, request: &Request) -> Result<Response, String> {
        let Snapshot::EvmIr { name, blocks, .. } = &request.module else {
            return Err("edge coverage instruments EVM IR only".into());
        };
        let plan = Plan::new(blocks);
        let first = take_ids(plan.len())?;
        if let Ok(path) = std::env::var(MAP_ENV) {
            write_map(&path, name, blocks, &plan, first)?;
        }
        Ok(Response { edits: plan.edits(blocks, first) })
    }
}

solar_pass_plugin::export_plugin!(EdgeCoverage);

/// A probe inside an existing block, before the instruction at `index`: 0 for the block's entry,
/// or right after a `jumpi`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub block: u32,
    pub index: u32,
}

/// How a split edge leaves its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// The `push <target>` at this instruction index, consumed by the next `jumpi`.
    Push(u32),
    /// This index among the terminator's targets.
    Terminator(usize),
}

/// A critical edge, which gets its own block and probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    pub source: u32,
    pub via: Via,
    pub target: u32,
}

/// A comparison whose operands are logged, at instruction `index` of `block`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cmp {
    pub block: u32,
    pub index: u32,
    pub opcode: u8,
}

/// Which control-flow points get probes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Only conditional edges, plus the module entry: each `jumpi`, branch or indexed jump edge
    /// gets its own probe, either at its single-predecessor target or in a split critical edge.
    /// Unconditional edges are implied by the conditional edge or entry that leads to them, as in
    /// SanitizerCoverage's pruning, so this records the same edges with fewer probes.
    #[default]
    Branches,
    /// Every block start and every point after a `jumpi` (SanitizerCoverage edge level
    /// without pruning).
    Blocks,
}

impl Mode {
    /// Reads [`MODE_ENV`]: `blocks` selects [`Mode::Blocks`], anything else the default.
    pub fn from_env() -> Self {
        match std::env::var(MODE_ENV).as_deref() {
            Ok("blocks") => Self::Blocks,
            _ => Self::Branches,
        }
    }
}

/// Reads [`CMPLOG_ENV`]: comparison operand logging is on unless it is `0`.
pub fn cmplog_from_env() -> bool {
    std::env::var(CMPLOG_ENV).as_deref() != Ok("0")
}

/// The instrumentation of one module. Probes take IDs in order, then split edges, then comparison
/// sites.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub probes: Vec<Probe>,
    pub splits: Vec<Split>,
    pub cmps: Vec<Cmp>,
}

impl Plan {
    /// Plans the instrumentation of one module.
    pub fn new(blocks: &[EvmBlock]) -> Self {
        Self::with_mode(blocks, Mode::from_env(), cmplog_from_env())
    }

    /// Plans the instrumentation of one module in the given mode, logging comparison operands when
    /// `cmplog` is set.
    pub fn with_mode(blocks: &[EvmBlock], mode: Mode, cmplog: bool) -> Self {
        let predecessors = predecessor_counts(blocks);
        let mut plan = Self::default();
        // The module entry, and in branch mode the blocks entered only through one conditional
        // edge, get an entry probe. In blocks mode every block does.
        let mut entry_probe = HashSet::new();
        if !blocks.is_empty() {
            entry_probe.insert(0);
        }
        for block in blocks {
            let instructions = &block.instructions;
            for (index, inst) in instructions.iter().enumerate() {
                if cmplog && !inst.protected && comparison_name(inst.opcode).is_some() {
                    plan.cmps.push(Cmp {
                        block: block.id,
                        index: index as u32,
                        opcode: inst.opcode,
                    });
                }
                if inst.opcode != JUMPI {
                    continue;
                }
                if index + 1 < instructions.len() && !inst.protected {
                    plan.probes.push(Probe { block: block.id, index: index as u32 + 1 });
                }
                // A branch has two successors: its target and the code after it.
                if let Some(push) = index.checked_sub(1).map(|push| &instructions[push])
                    && let Some(target) = push.block
                    && !push.protected
                {
                    if predecessors[target as usize] > 1 {
                        plan.splits.push(Split {
                            source: block.id,
                            via: Via::Push(push.index),
                            target,
                        });
                    } else {
                        entry_probe.insert(target);
                    }
                }
            }
            let targets = targets(&block.terminator);
            // The terminator also receives the fallthrough edge of a final `jumpi`.
            let ends_in_branch = instructions.last().is_some_and(|inst| inst.opcode == JUMPI);
            if targets.len() + usize::from(ends_in_branch) < 2 {
                continue;
            }
            for (slot, &target) in targets.iter().enumerate() {
                if predecessors[target as usize] > 1 {
                    plan.splits.push(Split {
                        source: block.id,
                        via: Via::Terminator(slot),
                        target,
                    });
                } else {
                    entry_probe.insert(target);
                }
            }
        }
        let entries = blocks
            .iter()
            .filter(|block| mode == Mode::Blocks || entry_probe.contains(&block.id))
            .map(|block| Probe { block: block.id, index: 0 });
        plan.probes.splice(0..0, entries);
        plan
    }

    /// Returns the number of edge and comparison site IDs this plan uses.
    pub fn len(&self) -> u32 {
        (self.probes.len() + self.splits.len() + self.cmps.len()) as u32
    }

    /// Returns whether this plan instruments nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the edits that apply this plan, numbering probes from `first`.
    pub fn edits(&self, blocks: &[EvmBlock], first: u32) -> Vec<Edit> {
        let mut edits = Vec::new();
        let split_first = first + self.probes.len() as u32;
        let new_first = blocks.len() as u32;
        if !self.splits.is_empty() {
            // edge_k: push <id>; edgecov; jump target
            edits.push(Edit::AddEvmBlocks {
                blocks: self
                    .splits
                    .iter()
                    .zip(split_first..)
                    .map(|(split, id)| NewEvmBlock {
                        instructions: probe(id),
                        terminator: EvmTerminator::Jump { target: split.target },
                        call_effect: None,
                    })
                    .collect(),
            });
        }
        // Redirections keep instruction indices, so they precede the probe insertions.
        let mut retargeted = Vec::<(u32, Vec<u32>)>::new();
        for (index, split) in self.splits.iter().enumerate() {
            let edge = new_first + index as u32;
            match split.via {
                // push edge_k (was push target)
                // jumpi
                Via::Push(push) => edits.push(Edit::SpliceEvmInstructions {
                    block: split.source,
                    start: push,
                    remove: 1,
                    instructions: vec![NewEvmInstruction::BlockAddress { block: edge }],
                }),
                Via::Terminator(slot) => {
                    if retargeted.last().is_none_or(|(source, _)| *source != split.source) {
                        let terminator = &blocks[split.source as usize].terminator;
                        retargeted.push((split.source, targets(terminator)));
                    }
                    retargeted.last_mut().unwrap().1[slot] = edge;
                }
            }
        }
        for (source, targets) in retargeted {
            // source: ...; jump|jumpi|indexed_jump edge_k... (critical targets redirected)
            edits.push(Edit::SetEvmTerminator {
                block: source,
                terminator: retarget(&blocks[source as usize].terminator, targets),
            });
        }
        // (block, index, rank, instructions): at one index, the edge probe comes before the
        // comparison operand log.
        let mut insertions = Vec::new();
        for (probe_at, id) in self.probes.iter().zip(first..) {
            insertions.push((probe_at.block, probe_at.index, 1, probe(id)));
        }
        let cmp_first = split_first + self.splits.len() as u32;
        for (cmp, id) in self.cmps.iter().zip(cmp_first..) {
            insertions.push((cmp.block, cmp.index, 0, cmp_probe(id, cmp.opcode)));
        }
        // Insert from the end of each block so earlier indices stay valid.
        insertions.sort_by_key(|&(block, index, rank, _)| (block, std::cmp::Reverse(index), rank));
        for (block, index, _, instructions) in insertions {
            // bb: ...; push <id>; edgecov; ...
            // bb: ...; push <id << 8 | cmp>; cmplog; <cmp>; ...
            edits.push(Edit::SpliceEvmInstructions {
                block,
                start: index,
                remove: 0,
                instructions,
            });
        }
        edits
    }
}

/// Returns the terminator's block targets in order, one per edge.
pub fn targets(terminator: &EvmTerminator) -> Vec<u32> {
    match terminator {
        EvmTerminator::Jump { target } => vec![*target],
        EvmTerminator::Branch { then_block, else_block } => vec![*then_block, *else_block],
        EvmTerminator::IndexedJump { targets } => targets.clone(),
        EvmTerminator::Opcode { .. } => Vec::new(),
    }
}

/// Counts each block's incoming edges: terminator and `push <target>; jumpi` edges, plus one for
/// the module entry, for a block whose address is otherwise pushed, so a dynamic jump can enter
/// it, and for the block after one whose terminal opcode may fall through.
pub fn predecessor_counts(blocks: &[EvmBlock]) -> Vec<u32> {
    let mut counts = vec![0_u32; blocks.len()];
    let mut dynamic = HashSet::new();
    if let Some(entry) = counts.first_mut() {
        *entry += 1;
    }
    for (index, block) in blocks.iter().enumerate() {
        for target in targets(&block.terminator) {
            counts[target as usize] += 1;
        }
        let instructions = &block.instructions;
        for (at, inst) in instructions.iter().enumerate() {
            let Some(target) = inst.block else { continue };
            if instructions.get(at + 1).is_some_and(|next| next.opcode == JUMPI) {
                counts[target as usize] += 1;
            } else {
                dynamic.insert(target);
            }
        }
        if let EvmTerminator::Opcode { opcode } = block.terminator
            && !is_halting(opcode)
            && let Some(next) = counts.get_mut(index + 1)
        {
            *next += 1;
        }
    }
    for target in dynamic {
        counts[target as usize] += 1;
    }
    counts
}

/// Rebuilds a terminator of the same kind with new targets.
fn retarget(terminator: &EvmTerminator, targets: Vec<u32>) -> EvmTerminator {
    match terminator {
        EvmTerminator::Jump { .. } => EvmTerminator::Jump { target: targets[0] },
        EvmTerminator::Branch { .. } => {
            EvmTerminator::Branch { then_block: targets[0], else_block: targets[1] }
        }
        EvmTerminator::IndexedJump { .. } => EvmTerminator::IndexedJump { targets },
        EvmTerminator::Opcode { opcode } => EvmTerminator::Opcode { opcode: *opcode },
    }
}

/// Returns whether a terminal opcode never falls through to the next block.
fn is_halting(opcode: u8) -> bool {
    // stop | jump | return | revert | invalid | selfdestruct
    matches!(opcode, 0x00 | 0x56 | 0xf3 | 0xfd | 0xfe | 0xff)
}

/// Returns `push <id>; edgecov`.
fn probe(id: u32) -> Vec<NewEvmInstruction> {
    // push <id>
    // edgecov
    vec![
        NewEvmInstruction::Push { value: id.to_string(), source: None },
        NewEvmInstruction::Opcode { opcode: EDGECOV, source: None },
    ]
}

/// Returns `push <(id << 8) | opcode>; cmplog`, logging the operands of comparison `opcode`.
fn cmp_probe(id: u32, opcode: u8) -> Vec<NewEvmInstruction> {
    // push <(id << 8) | opcode>
    // cmplog
    vec![
        NewEvmInstruction::Push { value: cmplog_word(id, opcode).to_string(), source: None },
        NewEvmInstruction::Opcode { opcode: CMPLOG, source: None },
    ]
}

/// Returns the word a `cmplog` probe pops: `(site ID << 8) | comparison opcode`.
pub fn cmplog_word(id: u32, opcode: u8) -> u64 {
    u64::from(id) << 8 | u64::from(opcode)
}

/// Returns the name of a comparison whose operands are logged.
pub fn comparison_name(opcode: u8) -> Option<&'static str> {
    COMPARISONS.iter().find(|&&(comparison, _)| comparison == opcode).map(|&(_, name)| name)
}

/// Reserves `count` consecutive edge and comparison site IDs and returns the first.
fn take_ids(count: u32) -> Result<u32, String> {
    let mut first = NEXT_ID.load(Ordering::Relaxed);
    loop {
        let end = first.checked_add(count).ok_or("edge coverage IDs exhausted")?;
        match NEXT_ID.compare_exchange_weak(first, end, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(first),
            Err(current) => first = current,
        }
    }
}

fn write_map(
    path: &str,
    module: &str,
    blocks: &[EvmBlock],
    plan: &Plan,
    first: u32,
) -> Result<(), String> {
    let label = |block: u32| blocks[block as usize].label;
    let mut out = String::new();
    for (probe, id) in plan.probes.iter().zip(first..) {
        if probe.index == 0 {
            writeln!(out, "{id} {module} {} block", label(probe.block)).unwrap();
        } else {
            let jumpi = probe.index - 1;
            writeln!(out, "{id} {module} {} after-jumpi:{jumpi}", label(probe.block)).unwrap();
        }
    }
    for (split, id) in plan.splits.iter().zip(first + plan.probes.len() as u32..) {
        writeln!(out, "{id} {module} {} edge:{}", label(split.source), label(split.target))
            .unwrap();
    }
    let cmp_first = first + plan.probes.len() as u32 + plan.splits.len() as u32;
    for (cmp, id) in plan.cmps.iter().zip(cmp_first..) {
        let name = comparison_name(cmp.opcode).unwrap();
        writeln!(out, "{id} {module} {} cmp:{name}:{}", label(cmp.block), cmp.index).unwrap();
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(out.as_bytes()))
        .map_err(|error| format!("cannot write the edge map: {error}"))
}
