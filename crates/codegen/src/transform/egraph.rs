//! Instruction simplification and value numbering in one e-graph pass.
//!
//! This is an acyclic e-graph in the style of Cranelift's mid-end. Pure
//! instructions become nodes over the canonical values of their operands.
//! Nodes are hash-consed within dominator scopes, so an expression that already
//! has a dominating definition reuses it. The rules in `isle/egraph.isle` run
//! on every new node: `simplify` merges the node's class into an existing
//! value and `rewrite` adds an equivalent node to the class. New nodes only
//! reference classes that already exist, so no rebuild or fixpoint is needed.
//!
//! After the walk, every surviving class keeps its cheapest node. The cost
//! model is static gas plus the stack traffic a node implies: an operand's
//! computation is charged only when this node is its sole user, and a node that
//! reaches for a value the instruction did not need before pays for the copy
//! that keeps that value alive, unless the operand it displaces dies here.
//! Instructions are rewritten in place at their original position; merged
//! instructions are deleted and their uses redirected. Placement never
//! changes, so the pass cannot hoist or sink and never grows code.
//!
//! Instructions with effects, phis, and terminators form the skeleton. They
//! stay in place and see their operands canonicalized; `rewrite` rules still
//! apply to them in place. Phis over one value merge into it, phis of one
//! block with equal incoming values merge into one, copies of zero bytes are
//! deleted, and branches on `iszero` or a nonzero test branch on the tested
//! value directly.
//!
//! Safety contract:
//! - do not remove or reorder side effects
//! - replace an instruction with a value only when the equality is exact for all 256-bit EVM words
//! - preserve boolean-only rewrites behind explicit MIR boolean type checks

use crate::{
    analysis::CfgInfo,
    mir::{
        ArgIdx, BlockId, EffectKind, Function, Immediate, InstId, InstKind, MirType, Module, Op,
        Terminator, Value, ValueId, utils as mir_utils,
    },
    pass::{MirPass, run_function_pass},
    target::{Cost, Target},
    utils::eval,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, StdEntry},
};
use std::rc::Rc;

mod isle;

const TRACE_TARGET: &str = "solar::codegen::mir::egraph";

/// Function pass for e-graph based simplification and value numbering.
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
            Builder::new(func, Target::new(gcx), Rc::clone(&analyses.cfg)).run() != 0
        })
    }
}

/// Bound on the nodes of one class, counting the instruction as written.
const MAX_NODES: usize = 12;

/// A hash-consing key: a node over canonical operands and its result type.
type NodeKey = (Op, Option<MirType>);

/// A hash-consing key for phis: the block and the canonical incoming values.
type PhiKey = (Vec<(BlockId, ValueId)>, Option<MirType>);

/// A class of equal values rooted at one instruction.
struct Class {
    /// Equivalent nodes; the first is the instruction as written.
    nodes: Vec<Op>,
    /// The instruction defining the class representative.
    home: InstId,
}

struct Builder<'a> {
    func: &'a mut Function,
    target: Target,
    cfg: Rc<CfgInfo>,
    /// One identity for equal immediates, so nodes over them compare equal.
    immediates: FxHashMap<Immediate, ValueId>,
    /// One identity for equal immediates and for each function argument.
    /// Only instruction results are ever rewritten, so these never reach the
    /// replacement map.
    leaves: FxHashMap<ValueId, ValueId>,
    /// Representative for every value merged into another class.
    merged: FxHashMap<ValueId, ValueId>,
    /// Classes keyed by representative value.
    classes: FxHashMap<ValueId, Class>,
    /// Hash-consing table scoped by the dominator tree.
    memo: FxHashMap<NodeKey, ValueId>,
    /// Undo log restoring `memo` when a dominator scope closes.
    undo: Vec<(NodeKey, Option<ValueId>)>,
    /// Phis congruent through loop-carried operands, mapped to the leader of
    /// their class: a leaf, an earlier phi of the block, or a dominating value.
    optimistic: FxHashMap<ValueId, ValueId>,
    /// Hash-consing table for the phis of the current block.
    phis: FxHashMap<PhiKey, ValueId>,
    /// Number of uses of every value before the pass.
    uses: FxHashMap<ValueId, u32>,
    /// Instructions merged into another class or deleted as no-ops.
    dead: DenseBitSet<InstId>,
    changed: usize,
}

impl<'a> Builder<'a> {
    fn new(func: &'a mut Function, target: Target, cfg: Rc<CfgInfo>) -> Self {
        let dead = DenseBitSet::new_empty(func.num_insts());
        let (immediates, leaves) = canonical_leaves(func);
        let uses = use_counts(func);
        let optimistic = optimistic_phi_leaders(func, &cfg, &leaves);
        Self {
            func,
            target,
            cfg,
            immediates,
            leaves,
            optimistic,
            merged: FxHashMap::default(),
            classes: FxHashMap::default(),
            memo: FxHashMap::default(),
            undo: Vec::new(),
            phis: FxHashMap::default(),
            uses,
            dead,
            changed: 0,
        }
    }

    fn run(mut self) -> usize {
        self.visit(BlockId::ENTRY);
        self.materialize();
        self.rewrite_terminators();
        self.changed
    }

    /// Returns the class representative of `value`.
    fn resolve(&mut self, value: ValueId) -> ValueId {
        let mut value = self.leaf(value);
        while let Some(&next) = self.merged.get(&value) {
            value = next;
        }
        value
    }

    /// Returns the canonical identity of an immediate or argument, or `value`.
    fn leaf(&mut self, value: ValueId) -> ValueId {
        if let Some(&leaf) = self.leaves.get(&value) {
            return leaf;
        }
        // Immediates created by rules while the pass runs join the canonical set.
        if let Value::Immediate(immediate) = self.func.value(value) {
            let leaf = *self.immediates.entry(immediate.clone()).or_insert(value);
            if leaf != value {
                self.leaves.insert(value, leaf);
            }
            return leaf;
        }
        value
    }

    /// Numbers a block, then its dominator-tree children, within one scope.
    fn visit(&mut self, block: BlockId) {
        let mark = self.undo.len();
        self.phis.clear();
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
        if self.is_dead_noop(inst_id) {
            self.dead.insert(inst_id);
            self.changed += 1;
            return;
        }
        let Some(result) = self.func.inst_result_value(inst_id) else { return };
        let inst = self.func.inst(inst_id);
        let ty = inst.result_ty;
        if let InstKind::Phi(incoming) = &inst.kind {
            let incoming = incoming.clone();
            self.visit_phi(inst_id, result, incoming, ty);
            return;
        }
        if !is_node(&inst.kind) {
            self.rewrite_in_place(inst_id);
            return;
        }
        let kind = inst.kind.op();
        let op = kind.map_values(|value| self.resolve(value));

        // An equal expression with a dominating definition: reuse it.
        let key = (canonical(op), ty);
        if let Some(&leader) = self.memo.get(&key) {
            self.merge(result, leader, inst_id);
            return;
        }
        let previous = self.memo.insert(key, result);
        self.undo.push((key, previous));

        // Grow the class with every equivalent node the rules reach, breadth
        // first, then look for a value it already equals.
        let mut nodes = vec![op];
        let mut frontier = 0;
        let mut alternatives = Vec::new();
        while frontier < nodes.len() && nodes.len() < MAX_NODES {
            let current = nodes[frontier];
            frontier += 1;
            alternatives.clear();
            isle::RuleContext::new(self.func, self.target.evm_version())
                .rewrite(&current, &mut alternatives);
            for next in alternatives.drain(..) {
                let next = next.map_values(|value| self.resolve(value));
                if !nodes.contains(&next) && nodes.len() < MAX_NODES {
                    nodes.push(next);
                }
            }
        }
        for node in &nodes {
            let kind = node.into_kind().expect("nodes are complete instructions");
            let equal = const_fold(self.func, &kind).or_else(|| {
                isle::RuleContext::new(self.func, self.target.evm_version()).simplify(node)
            });
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

    /// Merges a phi over one value into that value, and phis of one block with
    /// equal incoming values into one.
    fn visit_phi(
        &mut self,
        inst_id: InstId,
        result: ValueId,
        incoming: Vec<(BlockId, ValueId)>,
        ty: Option<MirType>,
    ) {
        let incoming: Vec<(BlockId, ValueId)> =
            incoming.into_iter().map(|(block, value)| (block, self.resolve(value))).collect();
        if let Some(&(_, first)) = incoming.first()
            && incoming.iter().all(|&(_, value)| same_value(self.func, value, first))
        {
            self.merge(result, first, inst_id);
            return;
        }
        let key = (incoming, ty);
        if let Some(&leader) = self.phis.get(&key) {
            self.merge(result, leader, inst_id);
            return;
        }
        // A phi congruent through the loop to an earlier phi of this block or
        // to a dominating value:
        // %leader = phi [..], [latch: %x]
        // %result = phi [..], [latch: %y]   ; x ≡ y given leader ≡ result
        if let Some(&leader) = self.optimistic.get(&result) {
            let leader = self.resolve(leader);
            if leader != result {
                self.phis.insert(key, leader);
                self.merge(result, leader, inst_id);
                return;
            }
        }
        self.phis.insert(key, result);
    }

    /// Applies the rewrite rules to an instruction outside the e-graph.
    fn rewrite_in_place(&mut self, inst_id: InstId) {
        let op = self.func.inst(inst_id).kind.op();
        if op.into_kind().is_none() {
            return;
        }
        let mut current = op.map_values(|value| self.resolve(value));
        let mut rewritten = false;
        let mut alternatives = Vec::new();
        for _ in 0..MAX_NODES {
            alternatives.clear();
            isle::RuleContext::new(self.func, self.target.evm_version())
                .rewrite(&current, &mut alternatives);
            let Some(&next) = alternatives.first() else { break };
            current = next.map_values(|value| self.resolve(value));
            rewritten = true;
        }
        if !rewritten {
            return;
        }
        // %r = <rewritten instruction over canonical operands>
        let kind = current.into_kind().expect("rewrite rules produce complete instructions");
        let inst = self.func.inst_mut(inst_id);
        tracing::trace!(
            target: TRACE_TARGET,
            action = "rewrite",
            input = %inst.kind,
            output = %kind,
            "mir_egraph"
        );
        inst.kind = kind;
        if mir_utils::is_memory_inst(&inst.kind) {
            inst.metadata.set_memory_region(None);
        }
        self.changed += 1;
    }

    /// Returns whether an instruction copies zero bytes and can be deleted.
    fn is_dead_noop(&mut self, inst_id: InstId) -> bool {
        let (offset, size) = match self.func.inst(inst_id).kind {
            InstKind::MCopy(_, _, size)
            | InstKind::CalldataCopy(_, _, size)
            | InstKind::DataCopy(_, _, size)
            | InstKind::CodeCopy(_, _, size) => (None, size),
            InstKind::ReturnDataCopy(_, offset, size) => (Some(offset), size),
            _ => return false,
        };
        let size = self.resolve(size);
        if !is_zero(self.func, size) {
            return false;
        }
        match offset {
            Some(offset) => {
                let offset = self.resolve(offset);
                is_zero(self.func, offset)
            }
            None => true,
        }
    }

    fn merge(&mut self, result: ValueId, into: ValueId, inst_id: InstId) {
        tracing::trace!(
            target: TRACE_TARGET,
            function = %self.func.name,
            action = "merge",
            ?result,
            ?into,
            "mir_egraph"
        );
        self.merged.insert(result, into);
        self.dead.insert(inst_id);
        self.changed += 1;
    }

    /// Rewrites every surviving class to its cheapest node, deletes merged
    /// instructions, and redirects their uses.
    fn materialize(&mut self) {
        let mut costs = Costs {
            func: self.func,
            target: self.target,
            classes: &self.classes,
            uses: &self.uses,
            cache: FxHashMap::default(),
        };
        let mut cheapest = Vec::with_capacity(self.classes.len());
        for class in self.classes.values() {
            // Rules only produce canonical forms, so the latest node wins ties.
            let target = self.target;
            let best = class
                .nodes
                .iter()
                .rev()
                .copied()
                .min_by_key(|node| CostKey(target, costs.node(class, node)))
                .expect("a class holds its own node");
            cheapest.push((class.home, best));
        }
        // %r = <cheapest node over canonical operands>
        let name = self.func.name;
        for (home, best) in cheapest {
            let kind = best.into_kind().expect("nodes are complete instructions");
            let inst = self.func.inst_mut(home);
            if inst.kind != kind {
                tracing::trace!(
                    target: TRACE_TARGET,
                    function = %name,
                    action = "rewrite",
                    input = %inst.kind,
                    output = %kind,
                    "mir_egraph"
                );
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

    /// Branches on `iszero(x)` swap their targets and branch on `x`, branches
    /// on a nonzero test branch on the tested value, and an external return
    /// of zero bytes stops.
    fn rewrite_terminators(&mut self) {
        let func = &mut *self.func;
        let externally_terminating =
            func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback;
        for block_id in func.blocks.indices() {
            loop {
                let Some(Terminator::Branch { condition, .. }) = func.blocks[block_id].terminator
                else {
                    break;
                };
                let (inner, swap) = if let Some(inner) = iszero_operand(func, condition) {
                    (inner, true)
                } else if let Some(inner) = nonzero_test_operand(func, condition) {
                    // `branch gt(x, 0)` / `branch lt(0, x)` test exactly `x != 0`,
                    // which is what `branch x` already does.
                    (inner, false)
                } else {
                    break;
                };
                let inner = mir_utils::resolve_replacement(inner, &self.merged);
                let Some(Terminator::Branch { condition, then_block, else_block }) =
                    &mut func.blocks[block_id].terminator
                else {
                    unreachable!()
                };
                *condition = inner;
                if swap {
                    std::mem::swap(then_block, else_block);
                }
                self.changed += 1;
                tracing::trace!(
                    target: TRACE_TARGET,
                    function = %func.name,
                    action = "rewrite_terminator",
                    ?block_id,
                    swap,
                    "mir_egraph"
                );
            }

            if externally_terminating
                && let Some(Terminator::ReturnData { size, .. }) = func.blocks[block_id].terminator
                && is_zero(func, mir_utils::resolve_replacement(size, &self.merged))
            {
                func.blocks[block_id].terminator = Some(Terminator::Stop);
                self.changed += 1;
            }
        }
    }
}

/// Bound on the refinement rounds of the optimistic numbering.
const MAX_OPTIMISTIC_ROUNDS: usize = 16;

/// The optimistic class every value starts in.
const TOP: u32 = 0;

/// A key of the optimistic numbering. Node operands are class numbers cast to
/// value ids and never dereferenced.
#[derive(Clone, PartialEq, Eq, Hash)]
enum OptimisticKey {
    /// A phi by block, incoming class numbers, and type.
    Phi(BlockId, SmallVec<[(BlockId, u32); 4]>, Option<MirType>),
    /// A pure node over class numbers.
    Node(Op, Option<MirType>),
    /// A value equal only to itself.
    Unique(ValueId),
}

/// One round of optimistic numbering.
struct OptimisticNumbering<'a> {
    func: &'a Function,
    leaves: &'a FxHashMap<ValueId, ValueId>,
    /// Class number of every key numbered this round.
    table: FxHashMap<OptimisticKey, u32>,
    /// Class numbers assigned this round.
    fresh: FxHashMap<ValueId, u32>,
    /// Class numbers of the previous round, used across back edges.
    numbers: FxHashMap<ValueId, u32>,
    next: u32,
}

impl OptimisticNumbering<'_> {
    /// Class number of an operand: this round's number, the previous round's
    /// across a back edge, or `TOP` in the first round.
    fn number(&mut self, value: ValueId) -> u32 {
        let value = self.leaves.get(&value).copied().unwrap_or(value);
        if let Some(&number) = self.fresh.get(&value) {
            return number;
        }
        if matches!(self.func.value(value), Value::Inst(_)) {
            return self.numbers.get(&value).copied().unwrap_or(TOP);
        }
        self.assign(value, OptimisticKey::Unique(value))
    }

    fn assign(&mut self, value: ValueId, key: OptimisticKey) -> u32 {
        let next = &mut self.next;
        let number = *self.table.entry(key).or_insert_with(|| {
            *next += 1;
            *next
        });
        self.fresh.insert(value, number);
        number
    }
}

/// Optimistic value numbering after Simpson: every value starts in one class
/// and reverse-postorder numbering splits classes until a fixed point, so a
/// phi is congruent through loop-carried operands the dominator walk cannot
/// see through. Returns each such phi mapped to the leader of its class that
/// is available at the phi, or nothing when the numbering does not converge.
fn optimistic_phi_leaders(
    func: &Function,
    cfg: &CfgInfo,
    leaves: &FxHashMap<ValueId, ValueId>,
) -> FxHashMap<ValueId, ValueId> {
    let mut leaders = FxHashMap::default();
    // Only a cycle carries a value back into its own phi; elsewhere the
    // dominator walk numbers every phi over already numbered operands.
    let has_loop_phi = cfg.cyclic_blocks().iter().any(|block| {
        func.blocks[block]
            .instructions
            .iter()
            .any(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
    });
    if !has_loop_phi {
        return leaders;
    }
    let mut numbering = OptimisticNumbering {
        func,
        leaves,
        table: FxHashMap::default(),
        fresh: FxHashMap::default(),
        numbers: FxHashMap::default(),
        next: TOP,
    };
    let mut converged = false;
    for _ in 0..MAX_OPTIMISTIC_ROUNDS {
        numbering.table.clear();
        numbering.fresh.clear();
        numbering.next = TOP;
        for &block in cfg.rpo() {
            for &inst_id in &func.blocks[block].instructions {
                let Some(result) = func.inst_result_value(inst_id) else { continue };
                let inst = func.inst(inst_id);
                let key = match &inst.kind {
                    InstKind::Phi(incoming) => {
                        let incoming: SmallVec<[(BlockId, u32); 4]> = incoming
                            .iter()
                            .map(|&(pred, value)| (pred, numbering.number(value)))
                            .collect();
                        // A phi over one class is that class.
                        if let Some(&(_, first)) = incoming.first()
                            && first != TOP
                            && incoming.iter().all(|&(_, number)| number == first)
                        {
                            numbering.fresh.insert(result, first);
                            continue;
                        }
                        OptimisticKey::Phi(block, incoming, inst.result_ty)
                    }
                    kind if is_node(kind) => {
                        let op = kind.op().map_values(|value| {
                            ValueId::from_usize(numbering.number(value) as usize)
                        });
                        OptimisticKey::Node(canonical(op), inst.result_ty)
                    }
                    _ => OptimisticKey::Unique(result),
                };
                numbering.assign(result, key);
            }
        }
        if numbering.fresh == numbering.numbers {
            converged = true;
            break;
        }
        std::mem::swap(&mut numbering.numbers, &mut numbering.fresh);
    }
    if !converged {
        return leaders;
    }
    // The first value of each class in reverse postorder leads it; a phi
    // merges into its leader when the leader is a leaf, an earlier phi or
    // instruction of the same block, or defined in a dominating block.
    let mut first = FxHashMap::<u32, (ValueId, Option<BlockId>)>::default();
    for (&value, &number) in &numbering.numbers {
        if !matches!(func.value(value), Value::Inst(_)) {
            first.insert(number, (value, None));
        }
    }
    for &block in cfg.rpo() {
        for &inst_id in &func.blocks[block].instructions {
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            let number = numbering.numbers[&result];
            match first.entry(number) {
                StdEntry::Occupied(entry) => {
                    let (leader, leader_block) = *entry.get();
                    if matches!(func.inst(inst_id).kind, InstKind::Phi(_))
                        && leader_block.is_none_or(|leader_block| {
                            leader_block == block || cfg.dominators().dominates(leader_block, block)
                        })
                    {
                        leaders.insert(result, leader);
                    }
                }
                StdEntry::Vacant(entry) => {
                    entry.insert((result, Some(block)));
                }
            }
        }
    }
    leaders
}

/// Maps every immediate to the first equal immediate and every argument value
/// to the first value of that argument.
fn canonical_leaves(
    func: &Function,
) -> (FxHashMap<Immediate, ValueId>, FxHashMap<ValueId, ValueId>) {
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
    (immediates, leaves)
}

/// Counts the uses of every value in instructions and terminators.
fn use_counts(func: &Function) -> FxHashMap<ValueId, u32> {
    let mut uses = FxHashMap::<ValueId, u32>::default();
    for inst_id in func.instructions() {
        for operand in func.inst(inst_id).operands() {
            *uses.entry(operand).or_default() += 1;
        }
    }
    for block in func.blocks.iter() {
        if let Some(term) = &block.terminator {
            term.for_each_operand(|operand| *uses.entry(operand).or_default() += 1);
        }
    }
    uses
}

/// Returns whether an instruction is a pure expression the e-graph may number.
///
/// `calldataload`, `blockhash`, and `blobhash` are stable within one execution
/// and join the pure operations. Reads that need clobber tracking stay with
/// CSE.
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

/// Folds an instruction over immediate operands to an immediate result.
fn const_fold(func: &mut Function, kind: &InstKind) -> Option<ValueId> {
    if let InstKind::Select(condition, then_value, else_value) = *kind {
        let condition = func.value_u256(condition)?;
        return Some(if condition.is_zero() { else_value } else { then_value });
    }
    let value = eval::eval_inst(kind, |value| func.value_u256(value).ok_or(())).ok().flatten()?;
    let immediate = match kind {
        InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::IsZero(..) => Immediate::bool(!value.is_zero()),
        _ => Immediate::uint256(value),
    };
    Some(func.alloc_value(Value::Immediate(immediate)))
}

fn is_zero(func: &Function, value: ValueId) -> bool {
    func.value_u256(value) == Some(U256::ZERO)
}

/// Returns whether two values are the same value or equal immediates.
fn same_value(func: &Function, a: ValueId, b: ValueId) -> bool {
    a == b
        || match (func.value(a), func.value(b)) {
            (Value::Immediate(a), Value::Immediate(b)) => a == b,
            _ => false,
        }
}

fn defining_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst_id) => Some(&func.inst(*inst_id).kind),
        _ => None,
    }
}

/// Returns `x` when `value` computes `gt(x, 0)` or `lt(0, x)`, both of which
/// are the unsigned nonzero test.
fn nonzero_test_operand(func: &Function, value: ValueId) -> Option<ValueId> {
    match *defining_kind(func, value)? {
        InstKind::Gt(a, b) if is_zero(func, b) => Some(a),
        InstKind::Lt(a, b) if is_zero(func, a) => Some(b),
        _ => None,
    }
}

fn iszero_operand(func: &Function, value: ValueId) -> Option<ValueId> {
    match *defining_kind(func, value)? {
        InstKind::IsZero(inner) => Some(inner),
        _ => None,
    }
}

/// Cost evaluation over the classes of one function under the target model.
struct Costs<'a> {
    func: &'a Function,
    target: Target,
    classes: &'a FxHashMap<ValueId, Class>,
    uses: &'a FxHashMap<ValueId, u32>,
    /// Cheapest cost per class, memoized.
    cache: FxHashMap<ValueId, Cost>,
}

impl Costs<'_> {
    fn uses(&self, value: ValueId) -> u32 {
        self.uses.get(&value).copied().unwrap_or(0)
    }

    /// Cost of the cheapest node of a class; values outside the e-graph are
    /// free, except immediates, which are pushed at every use.
    fn class(&mut self, value: ValueId) -> Cost {
        if let Some(&cost) = self.cache.get(&value) {
            return cost;
        }
        let Some(class) = self.classes.get(&value) else {
            return match self.func.value(value) {
                Value::Immediate(immediate) => {
                    immediate.as_u256().map_or(Cost::ZERO, |value| self.target.push(value))
                }
                _ => Cost::ZERO,
            };
        };
        let target = self.target;
        let cost = class
            .nodes
            .iter()
            .map(|node| self.node(class, node))
            .min_by(|a, b| target.cmp(*a, *b))
            .expect("a class holds its own node");
        self.cache.insert(value, cost);
        cost
    }

    /// Cost of computing `node` in place of the instruction that roots `class`.
    fn node(&mut self, class: &Class, node: &Op) -> Cost {
        let operands = operands_of(node);
        // An operand shared with other users is computed regardless of this
        // choice; an immediate is pushed at every use.
        let func = self.func;
        let mut cost = self.target.op(node, |value| match func.value(value) {
            Value::Immediate(immediate) => immediate.as_u256(),
            _ => None,
        });
        for &operand in &operands {
            if self.uses(operand) <= 1 || matches!(self.func.value(operand), Value::Immediate(_)) {
                cost += self.class(operand);
            }
        }
        // A rewrite that reaches for values the instruction did not need keeps
        // them alive up to here, unless every operand it stops needing dies here.
        // Immediates are pushed fresh and cost nothing to keep.
        let original = operands_of(&class.nodes[0]);
        if node != &class.nodes[0] {
            let displaced_survive =
                original.iter().any(|value| !operands.contains(value) && self.uses(*value) > 1);
            if displaced_survive {
                let reached = operands
                    .iter()
                    .filter(|&&value| {
                        !original.contains(&value)
                            && !matches!(self.func.value(value), Value::Immediate(_))
                    })
                    .count();
                cost += self.target.dup().times(reached as u32);
            }
        }
        cost
    }
}

/// A cost ordered under the target objective.
#[derive(Clone, Copy)]
struct CostKey(Target, Cost);

impl PartialEq for CostKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for CostKey {}

impl PartialOrd for CostKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CostKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(self.1, other.1)
    }
}

/// Returns the value operands of a node.
fn operands_of(node: &Op) -> SmallVec<[ValueId; 8]> {
    let mut operands = SmallVec::new();
    // Only the visit matters; the mapped copy is discarded.
    let _ = node.map_values(|value| {
        operands.push(value);
        value
    });
    operands
}
