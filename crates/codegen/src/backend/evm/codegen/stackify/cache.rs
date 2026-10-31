//! Reuse of function plans across the contracts of a compilation.
//!
//! Contracts copy the same library and internal functions, so many bodies reach planning
//! byte-identical in several modules. Planning one is a pure function of what [`PlanKey`] holds:
//! the body with its value, instruction and block numbering and the hotness of its blocks, the
//! facts of [`ModuleInfo`] that the planner reads for the function and for each function it calls,
//! the writes that may reach the spill area, the cost model and the call-spill level. Equal keys
//! give equal plans, so a plan is stored once per key on the global context and later modules take
//! a copy.
//!
//! Callee identities are numbered per module, so the key names each callee by the order of its
//! first appearance in the body and the stored plan does the same; a hit maps those numbers to
//! the callees of its own module. A key is the exact bytes of its inputs and the table compares
//! them, so a hash collision cannot return another function's plan. Only successful plans are
//! kept. Debug metadata and names are left out: the planner does not read them, and emission
//! reads them from the module being emitted.

use super::{BlockPlan, CallSpill, Edge, Exit, FunctionPlan, ModuleInfo, Step};
use crate::{
    mir::{Callee, Function, FunctionId, InstId, InstKind, Module, Terminator, Value, ValueId},
    target::Target,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxBuildHasher, FxHashMap, FxHashSet},
    sync::RwLock,
};
use std::{
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};

/// Bits of a key's hash that pick its shard.
const SHARD_BITS: u32 = 6;

/// Shards of the plan cache.
const SHARDS: usize = 1 << SHARD_BITS;

/// The plans of a compilation by key, kept on its global context. Threads may race to fill an
/// entry: equal keys give equal plans.
pub(in crate::backend::evm) struct PlanCache([Shard; SHARDS]);

/// One shard of the cache. Each has cache lines of its own, so threads on different shards do
/// not contend.
#[derive(Default)]
#[repr(align(128))]
struct Shard(RwLock<FxHashMap<Box<[u8]>, Arc<FunctionPlan>>>);

impl Default for PlanCache {
    fn default() -> Self {
        Self(std::array::from_fn(|_| Shard::default()))
    }
}

impl PlanCache {
    /// The plan stored for `key`, with its callees mapped to those of `key`'s module.
    pub(super) fn get(&self, key: &PlanKey) -> Option<FunctionPlan> {
        let plan = self.shard(key).0.read().get(&key.bytes[..])?.clone();
        plan.remap(|number| key.callees.get(number.index()).copied())
    }

    /// Stores `plan` under `key`, with its callees numbered by their first appearance in the
    /// body.
    pub(super) fn insert(&self, key: PlanKey, plan: &FunctionPlan) {
        let Some(plan) = plan.remap(|callee| key.callee_number(callee)) else { return };
        self.shard(&key).0.write().entry(key.bytes.into()).or_insert_with(|| Arc::new(plan));
    }

    /// Frees every stored plan. Later lookups miss and plan again.
    pub(in crate::backend::evm) fn clear(&self) {
        for shard in &self.0 {
            *shard.0.write() = FxHashMap::default();
        }
    }

    fn shard(&self, key: &PlanKey) -> &Shard {
        // The shard takes the hash's high bits, as the table indexes by the low ones.
        let hash = FxBuildHasher.hash_one(&key.bytes[..]);
        &self.0[(hash >> (u64::BITS - SHARD_BITS)) as usize]
    }
}

/// Everything planning a function reads, as bytes, with the callees in order of appearance.
pub(super) struct PlanKey {
    bytes: Vec<u8>,
    callees: Vec<FunctionId>,
    numbers: FxHashMap<FunctionId, FunctionId>,
}

impl PlanKey {
    pub(super) fn new(
        module: &Module,
        info: &ModuleInfo,
        func_id: FunctionId,
        hazards: &FxHashSet<InstId>,
        writers: &DenseBitSet<FunctionId>,
        target: &Target,
        call_spill: CallSpill,
    ) -> Self {
        let func = &module.functions[func_id];
        let mut key = Self {
            bytes: Vec::with_capacity(16 * func.num_insts() + 64),
            callees: Vec::new(),
            numbers: FxHashMap::default(),
        };

        (target, call_spill).hash(&mut key);

        // The function's own facts.
        let component = info.components[func_id];
        key.word(
            u64::from(info.internal.contains(func_id))
                | u64::from(info.returning.contains(func_id)) << 1
                | u64::from(component.is_some()) << 2
                | u64::from(func_id == info.entry && info.emitting_entry) << 3,
        );
        key.word(info.returns[func_id] as u64);
        key.word(info.params[func_id] as u64);

        // The writes that may reach the spill area, in order.
        let mut hazards = hazards.iter().map(|inst| inst.index()).collect::<Vec<_>>();
        hazards.sort_unstable();
        key.word(hazards.len() as u64);
        for inst in hazards {
            key.word(inst as u64);
        }

        key.function(func);

        // The facts of each callee, in order of appearance.
        for index in 0..key.callees.len() {
            let callee = key.callees[index];
            let callee_component = info.components[callee];
            key.word(
                u64::from(info.internal.contains(callee))
                    | u64::from(info.returning.contains(callee)) << 1
                    | u64::from(callee_component.is_some()) << 2
                    | u64::from(component.is_some() && callee_component == component) << 3
                    | u64::from(writers.contains(callee)) << 4,
            );
            key.word(info.returns[callee] as u64);
            key.word(info.params[callee] as u64);
        }
        key
    }

    /// The number of `callee` among the callees of the key's body.
    fn callee_number(&self, callee: FunctionId) -> Option<FunctionId> {
        self.numbers.get(&callee).copied()
    }

    /// Numbers `callee` in order of first appearance.
    fn callee(&mut self, callee: FunctionId) {
        let next = FunctionId::from_usize(self.callees.len());
        let number = *self.numbers.entry(callee).or_insert(next);
        if number == next {
            self.callees.push(callee);
        }
        self.word(number.index() as u64);
    }

    /// Appends `word` in seven-bit groups, so the small numbers that fill a key take one byte.
    fn word(&mut self, mut word: u64) {
        while word >= 0x80 {
            self.bytes.push(word as u8 | 0x80);
            word >>= 7;
        }
        self.bytes.push(word as u8);
    }

    /// Writes the body: every allocated value, then the instructions of each block.
    fn function(&mut self, func: &Function) {
        self.word(func.num_values() as u64);
        self.word(func.num_insts() as u64);
        self.word(func.params.len() as u64);
        for ty in &func.params {
            ty.hash(self);
        }
        for value in (0..func.num_values()).map(ValueId::from_usize) {
            match func.value(value) {
                Value::Inst(inst) => {
                    self.word(0);
                    self.word(inst.index() as u64);
                }
                Value::Arg(index) => {
                    self.word(1);
                    self.word(index.index() as u64);
                    func.arg_ty(*index).hash(self);
                }
                Value::Immediate(immediate) => {
                    self.word(2);
                    immediate.hash(self);
                }
                Value::Undef(ty) => {
                    self.word(3);
                    ty.hash(self);
                }
                Value::Error(_) => self.word(4),
            }
        }
        self.word(func.blocks.len() as u64);
        for block in &func.blocks {
            self.word(block.instructions.len() as u64);
            for &id in &block.instructions {
                id.hash(self);
                func.inst_result_value(id).hash(self);
                let inst = func.inst(id);
                inst.result_ty.hash(self);
                match &inst.kind {
                    InstKind::ICall { function: Callee::Function(callee), args } => {
                        self.word(1);
                        self.callee(*callee);
                        args.hash(self);
                    }
                    kind => {
                        self.word(0);
                        kind.hash(self);
                    }
                }
            }
            block.predecessors.hash(self);
            block.hotness.hash(self);
            match &block.terminator {
                Some(Terminator::TailCall { function, args }) => {
                    self.word(1);
                    self.callee(*function);
                    args.hash(self);
                }
                terminator => {
                    self.word(0);
                    terminator.hash(self);
                }
            }
        }
    }
}

/// Appends everything hashed into it: integers as words, other bytes as they are.
impl Hasher for PlanKey {
    fn write(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn write_u16(&mut self, word: u16) {
        self.word(word.into());
    }

    fn write_u32(&mut self, word: u32) {
        self.word(word.into());
    }

    fn write_u64(&mut self, word: u64) {
        self.word(word);
    }

    fn write_usize(&mut self, word: usize) {
        self.word(word as u64);
    }

    fn write_isize(&mut self, word: isize) {
        self.word(word as u64);
    }

    fn finish(&self) -> u64 {
        unreachable!("a plan key keeps its bytes")
    }
}

impl FunctionPlan {
    /// A copy with every callee mapped by `map`, or `None` when `map` has no callee for one, or
    /// when the plan is for a rebuilt copy of the function, whose own calls name its callees.
    fn remap(&self, map: impl Fn(FunctionId) -> Option<FunctionId>) -> Option<Self> {
        if self.rebuilt.is_some() {
            return None;
        }
        let mut blocks = IndexVec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            blocks.push(match block {
                Some(block) => Some(block.remap(&map)?),
                None => None,
            });
        }
        Some(Self {
            blocks,
            spilled: self.spilled.clone(),
            ret_spilled: self.ret_spilled,
            peak: self.peak,
            calls: self
                .calls
                .iter()
                .map(|&(callee, base)| Some((map(callee)?, base)))
                .collect::<Option<_>>()?,
            cost: self.cost,
            non_loop_gas: self.non_loop_gas,
            rebuilt: None,
        })
    }
}

impl BlockPlan {
    fn remap(&self, map: &impl Fn(FunctionId) -> Option<FunctionId>) -> Option<Self> {
        Some(Self {
            steps: remap_steps(&self.steps, map)?,
            terminator_start: self.terminator_start,
            exit: match &self.exit {
                Exit::Branch { then_edge, else_edge } => Exit::Branch {
                    then_edge: remap_edge(then_edge, map)?,
                    else_edge: remap_edge(else_edge, map)?,
                },
                Exit::Switch { default, cases, trampolines } => Exit::Switch {
                    default: *default,
                    cases: cases.clone(),
                    trampolines: trampolines
                        .iter()
                        .map(|(block, steps)| Some((*block, remap_steps(steps, map)?)))
                        .collect::<Option<_>>()?,
                },
                Exit::TailCall(callee) => Exit::TailCall(map(*callee)?),
                exit @ (Exit::Jump(_) | Exit::Return | Exit::Terminal) => exit.clone(),
            },
        })
    }
}

fn remap_edge(edge: &Edge, map: &impl Fn(FunctionId) -> Option<FunctionId>) -> Option<Edge> {
    Some(Edge {
        target: edge.target,
        trampoline: match &edge.trampoline {
            Some(steps) => Some(remap_steps(steps, map)?),
            None => None,
        },
    })
}

fn remap_steps(
    steps: &[Step],
    map: &impl Fn(FunctionId) -> Option<FunctionId>,
) -> Option<Vec<Step>> {
    steps
        .iter()
        .map(|step| {
            Some(match step {
                Step::Call(callee, height) => Step::Call(map(*callee)?, *height),
                &Step::Publish { callee, arity, params } => {
                    Step::Publish { callee: map(callee)?, arity, params }
                }
                step => step.clone(),
            })
        })
        .collect()
}
