//! Value numbering, rewrite rules, and cost-based extraction in one pass.
//!
//! This is an acyclic e-graph in the style of Cranelift's mid-end. Pure
//! instructions become nodes over the canonical values of their operands.
//! Nodes are hash-consed within dominator scopes, so an expression that already
//! has a dominating definition reuses it. The instruction simplification rules
//! in `isle/inst_simplify.isle` run on every new node: `simplify` merges the
//! node's class into an existing value and `rewrite` adds an equivalent node to
//! the class. New nodes only reference classes that already exist, so no
//! rebuild or fixpoint is needed.
//!
//! After the walk, every surviving class keeps its cheapest node under a static
//! gas cost model. Instructions are rewritten in place at their original
//! position; merged instructions are deleted and their uses redirected.
//!
//! Instructions with effects, phis, and terminators form the skeleton: they stay
//! in place and only see their operands canonicalized. Placement never changes,
//! so the pass cannot hoist or sink and never grows code. It subsumes one round
//! of `inst-simplify` followed by scoped value numbering, with the rules
//! applied to operands that are already numbered.

use crate::{
    analysis::CfgInfo,
    mir::{
        ArgIdx, BlockId, EffectKind, Function, Immediate, InstId, InstKind, MirType, Module, Op,
        Value, ValueId, utils as mir_utils,
    },
    pass::{MirPass, run_function_pass},
    transform::inst_simplify::{InstSimplifier, isle::RuleContext},
};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::rc::Rc;

/// Function pass for e-graph based value numbering and simplification.
pub(crate) struct Egraph;

impl MirPass for Egraph {
    fn name(&self) -> &'static str {
        "egraph"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            Builder::new(func, gcx.sess.opts.evm_version, Rc::clone(&analyses.cfg)).run() != 0
        })
    }
}

/// Bound on chained `rewrite` applications per class.
const MAX_REWRITES: usize = 8;

/// A hash-consing key: a node over canonical operands and its result type.
type NodeKey = (Op, Option<MirType>);

/// A class of equal values rooted at one instruction.
struct Class {
    /// Equivalent nodes; the first is the instruction as written.
    nodes: Vec<Op>,
    /// The instruction defining the class representative.
    home: InstId,
}

struct Builder<'a> {
    func: &'a mut Function,
    evm_version: EvmVersion,
    cfg: Rc<CfgInfo>,
    /// One identity for equal immediates and for each function argument, so
    /// nodes over them compare equal. Only instruction results are ever
    /// rewritten, so these never reach the replacement map.
    leaves: FxHashMap<ValueId, ValueId>,
    /// Representative for every value merged into another class.
    merged: FxHashMap<ValueId, ValueId>,
    /// Classes keyed by representative value.
    classes: FxHashMap<ValueId, Class>,
    /// Hash-consing table scoped by the dominator tree.
    memo: FxHashMap<NodeKey, ValueId>,
    /// Undo log restoring `memo` when a dominator scope closes.
    undo: Vec<(NodeKey, Option<ValueId>)>,
    /// Instructions merged into another class.
    dead: DenseBitSet<InstId>,
    changed: usize,
}

impl<'a> Builder<'a> {
    fn new(func: &'a mut Function, evm_version: EvmVersion, cfg: Rc<CfgInfo>) -> Self {
        let dead = DenseBitSet::new_empty(func.num_insts());
        let leaves = canonical_leaves(func);
        Self {
            func,
            evm_version,
            cfg,
            leaves,
            merged: FxHashMap::default(),
            classes: FxHashMap::default(),
            memo: FxHashMap::default(),
            undo: Vec::new(),
            dead,
            changed: 0,
        }
    }

    fn run(mut self) -> usize {
        self.visit(BlockId::ENTRY);
        self.materialize();
        self.changed
    }

    /// Returns the class representative of `value`.
    fn resolve(&self, value: ValueId) -> ValueId {
        let mut value = self.leaves.get(&value).copied().unwrap_or(value);
        while let Some(&next) = self.merged.get(&value) {
            value = next;
        }
        value
    }

    /// Numbers a block, then its dominator-tree children, within one scope.
    fn visit(&mut self, block: BlockId) {
        let mark = self.undo.len();
        for index in 0..self.func.blocks[block].instructions.len() {
            let inst_id = self.func.blocks[block].instructions[index];
            self.visit_inst(inst_id);
        }
        let children = self.cfg.dominators().children(block).to_vec();
        for child in children {
            self.visit(child);
        }
        while self.undo.len() > mark {
            let (key, previous) = self.undo.pop().expect("undo log entry");
            match previous {
                Some(value) => self.memo.insert(key, value),
                None => self.memo.remove(&key),
            };
        }
    }

    fn visit_inst(&mut self, inst_id: InstId) {
        let Some(result) = self.func.inst_result_value(inst_id) else { return };
        let inst = self.func.inst(inst_id);
        if !is_node(&inst.kind) {
            return;
        }
        let ty = inst.result_ty;
        let op = inst.kind.op().map_values(|value| self.resolve(value));

        // An equal expression with a dominating definition: reuse it.
        let key = (canonical(op), ty);
        if let Some(&leader) = self.memo.get(&key) {
            self.merge(result, leader, inst_id);
            return;
        }
        let previous = self.memo.insert(key, result);
        self.undo.push((key, previous));

        // Grow the class with equivalent nodes, then look for a value it already equals.
        let mut nodes = vec![op];
        let mut current = op;
        for _ in 0..MAX_REWRITES {
            let Some(next) = RuleContext::new(self.func, self.evm_version).rewrite(&current) else {
                break;
            };
            if nodes.contains(&next) {
                break;
            }
            nodes.push(next);
            current = next;
        }
        for node in &nodes {
            let kind = node.into_kind().expect("nodes are complete instructions");
            let equal = InstSimplifier::const_fold_inst(self.func, &kind, &FxHashMap::default())
                .or_else(|| RuleContext::new(self.func, self.evm_version).simplify(node));
            if let Some(equal) = equal {
                let equal = self.resolve(equal);
                if equal != result {
                    self.memo.insert(key, equal);
                    self.merge(result, equal, inst_id);
                    return;
                }
            }
        }

        self.classes.insert(result, Class { nodes, home: inst_id });
    }

    fn merge(&mut self, result: ValueId, into: ValueId, inst_id: InstId) {
        self.merged.insert(result, into);
        self.dead.insert(inst_id);
        self.changed += 1;
    }

    /// Rewrites every surviving class to its cheapest node, deletes merged
    /// instructions, and redirects their uses.
    fn materialize(&mut self) {
        let mut costs = FxHashMap::default();
        let mut cheapest = Vec::with_capacity(self.classes.len());
        for class in self.classes.values() {
            let best = class
                .nodes
                .iter()
                .copied()
                .min_by_key(|node| node_cost(&self.classes, &mut costs, node))
                .expect("a class holds its own node");
            cheapest.push((class.home, best));
        }
        // %r = <cheapest node over canonical operands>
        for (home, best) in cheapest {
            let kind = best.into_kind().expect("nodes are complete instructions");
            let inst = self.func.inst_mut(home);
            if inst.kind != kind {
                inst.kind = kind;
                self.changed += 1;
            }
        }

        if !self.dead.is_empty() {
            for block in self.func.blocks.iter_mut() {
                block.instructions.retain(|&id| !self.dead.contains(id));
            }
        }
        if self.merged.is_empty() {
            return;
        }
        // Skeleton instructions, phis, and terminators see canonical operands.
        let merged = &self.merged;
        self.func.for_each_instruction_mut(|_, inst| {
            if mir_utils::replace_inst_uses_canonicalized(&mut inst.kind, merged) != 0 {
                if mir_utils::is_memory_inst(&inst.kind) {
                    inst.metadata.set_memory_region(None);
                }
                if matches!(
                    inst.kind,
                    InstKind::SLoad(_)
                        | InstKind::SStore(_, _)
                        | InstKind::TLoad(_)
                        | InstKind::TStore(_, _)
                ) {
                    inst.metadata.set_storage_alias(None);
                }
            }
        });
        for block in self.func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                mir_utils::replace_terminator_uses_canonicalized(term, merged);
            }
        }
    }
}

/// Maps every immediate to the first equal immediate and every argument value
/// to the first value of that argument.
fn canonical_leaves(func: &Function) -> FxHashMap<ValueId, ValueId> {
    let mut immediates = FxHashMap::<Immediate, ValueId>::default();
    let mut args = FxHashMap::<ArgIdx, ValueId>::default();
    let mut leaves = FxHashMap::default();
    for value in func.live_values() {
        let canonical = match func.value(value) {
            Value::Immediate(immediate) => *immediates.entry(immediate.clone()).or_insert(value),
            Value::Arg(index) => *args.entry(*index).or_insert(value),
            _ => continue,
        };
        if canonical != value {
            leaves.insert(value, canonical);
        }
    }
    leaves
}

/// Returns whether an instruction is a pure expression the e-graph may number.
///
/// `calldataload`, `blockhash`, and `blobhash` are stable within one execution
/// and join the pure operations, as in value numbering. Reads that need
/// clobber tracking stay with CSE.
fn is_node(kind: &InstKind) -> bool {
    let definition = kind.op_def();
    let pure = definition.effect == EffectKind::Pure
        && !definition.has_side_effects
        && !matches!(kind, InstKind::Phi(_));
    pure || matches!(
        kind,
        InstKind::CalldataLoad(_) | InstKind::BlockHash(_) | InstKind::BlobHash(_)
    )
}

/// Orders commutative operands and flips reversed comparisons so equal
/// expressions share one key. The surviving instruction keeps its own form.
fn canonical(op: Op) -> Op {
    let sorted = |a: ValueId, b: ValueId| if b.index() < a.index() { (b, a) } else { (a, b) };
    match op {
        Op::Add { a, b } => {
            let (a, b) = sorted(a, b);
            Op::Add { a, b }
        }
        Op::Mul { a, b } => {
            let (a, b) = sorted(a, b);
            Op::Mul { a, b }
        }
        Op::And { a, b } => {
            let (a, b) = sorted(a, b);
            Op::And { a, b }
        }
        Op::Or { a, b } => {
            let (a, b) = sorted(a, b);
            Op::Or { a, b }
        }
        Op::Xor { a, b } => {
            let (a, b) = sorted(a, b);
            Op::Xor { a, b }
        }
        Op::Eq { a, b } => {
            let (a, b) = sorted(a, b);
            Op::Eq { a, b }
        }
        Op::AddMod { a, b, n } => {
            let (a, b) = sorted(a, b);
            Op::AddMod { a, b, n }
        }
        Op::MulMod { a, b, n } => {
            let (a, b) = sorted(a, b);
            Op::MulMod { a, b, n }
        }
        Op::Gt { a, b } => Op::Lt { a: b, b: a },
        Op::SGt { a, b } => Op::SLt { a: b, b: a },
        other => other,
    }
}

/// Static gas of one node, before its operands.
fn base_cost(op: &Op) -> u32 {
    match op {
        Op::Mul { .. }
        | Op::Div { .. }
        | Op::SDiv { .. }
        | Op::Mod { .. }
        | Op::SMod { .. }
        | Op::SignExtend { .. }
        | Op::Clz { .. } => 5,
        Op::AddMod { .. } | Op::MulMod { .. } => 8,
        Op::Exp { .. } => 60,
        Op::BlockHash { .. } | Op::BlobHash { .. } => 20,
        _ => 3,
    }
}

/// Cost of the cheapest node of a class; values outside the e-graph are free.
fn class_cost(
    classes: &FxHashMap<ValueId, Class>,
    costs: &mut FxHashMap<ValueId, u32>,
    value: ValueId,
) -> u32 {
    if let Some(&cost) = costs.get(&value) {
        return cost;
    }
    let Some(class) = classes.get(&value) else { return 0 };
    let cost = class
        .nodes
        .iter()
        .map(|node| node_cost(classes, costs, node))
        .min()
        .expect("a class holds its own node");
    costs.insert(value, cost);
    cost
}

fn node_cost(
    classes: &FxHashMap<ValueId, Class>,
    costs: &mut FxHashMap<ValueId, u32>,
    node: &Op,
) -> u32 {
    let mut operands = SmallVec::<[ValueId; 8]>::new();
    // Only the visit matters; the mapped copy is discarded.
    let _ = node.map_values(|value| {
        operands.push(value);
        value
    });
    base_cost(node) + operands.iter().map(|&value| class_cost(classes, costs, value)).sum::<u32>()
}
