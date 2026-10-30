//! EVM IR verifier.
//!
//! Two checks run over a module. The shape check is local: it validates labels, push encodings,
//! terminators, and that every instruction and terminator has a known stack effect. The
//! stack-operation check is global: it walks the direct control-flow edges from the entry block,
//! and from any block that declares an entry depth, and models the physical stack height so that
//! per-block imbalances, operand underflows, and depth violations are caught before assembly.
//!
//! EVM IR is one flat CFG of blocks with no function boundaries: an internal call is a block
//! that pushes a return address and jumps to the callee's first block, and a return is a dynamic
//! `jump` through that address, which carries no static target. The walk therefore sees a
//! recursive function as an ordinary cycle whose modeled stack height grows by the return address
//! on every round, and a concrete-depth walk that follows it never converges. The continuation a
//! call returns to is reached only through that dynamic jump, so it gets an edge of its own. A
//! block that ends with a jump to code declaring a [`CallEffect`] has pushed the continuation's
//! address in the slot that code returns to, which the walk finds by following the block's pushed
//! addresses through its stack operations. It enters the continuation at the stack the block ends
//! with, less the words the call consumes and plus the words it leaves. A block that returns
//! through a jump to an address it pushed itself, as a function inlined into its caller does,
//! enters that address's block at the stack after the jump. A recursive call closes a cycle
//! through the callee, which is cut as any other, and the continuation edge is an ordinary
//! forward edge. An address that is pushed in any other way, or copied, has no modeled entry
//! depth, and only the shape check covers its block.
//!
//! [`Verifier::stack_entry_upper_bounds`] bounds those blocks as well, for passes that need to
//! know how deep a block can start without the verifier's per-edge precision. A pushed address
//! can only be entered by a jump that consumes it, and no jump can be modeled from the address's
//! block alone when a branch carries it into other code, so the bound traces the word itself. It
//! follows the word's distance from the top over every stack operation and direct edge to the
//! jumps that consume it, and enters its block at the depth of the deepest of them. A word that
//! is copied, read, passed to a callee, or left on a path that can still reach a dynamic jump
//! through a word the trace cannot name has no bound.
//!
//! The walk instead propagates a range of entry depths per block and checks each bound where it
//! is the worst case. Operand availability (`dup`, `swap`, `pop`, and every instruction's inputs)
//! is monotone in the entry depth, so it is checked at the block's minimum; the 1024-word stack
//! limit is checked at the maximum.
//!
//! What makes recursion compile is that both bounds are split by how many cycle-closing edges the
//! walk that carried them has crossed, where the cycle-closing edges are those of one depth-first
//! walk from the entry, so removing them leaves a DAG. A block keeps an ingress bound, over walks
//! that cross none of them, and a post-cycle bound, over walks that cross exactly one. Crossing a
//! cycle-closing edge promotes each ingress bound to the target's post-cycle bound and drops the
//! post-cycle bound. So every new ingress bound is still propagated through one complete
//! traversal of the cycle and checked there, and only a second traversal is cut. Both halves are
//! least fixed points over the same DAG, the second seeded from the first, so the walk terminates
//! structurally rather than by a depth or iteration bound, and its result does not depend on the
//! order the worklist happens to run in.
//!
//! Cutting the second traversal is the only sound choice available statically, because how many
//! rounds a cycle runs is a runtime property; exceeding the stack limit through it is a runtime
//! failure, exactly as it is for solc. The two bounds are cut alike, so a cycle that drops a word
//! every round is no more rejected than one that leaves a word behind: both are drift the walk
//! declines to accumulate.
//!
//! What this guarantees is one complete traversal of every cycle per new ingress bound: any
//! violation a single pass produces is reported, including a pass whose high-depth ingress arrives
//! at a cycle over an edge the depth-first walk happened to classify as cycle-closing. What it
//! does not guarantee is anything about a walk that goes around a cycle twice. That is not
//! confined to going around one cycle twice, because the propagated bounds are per block and not
//! per path, and a simple path through an irreducible CFG can cross two cycle-closing edges: the
//! excess left unchecked at a block is bounded by the difference between its post-cycle and its
//! ingress bound at the second-to-last cycle-closing edge on the walk, which for a composition of
//! separate drifting cycles is the drift the earlier ones accumulated.
//! `two_cycle_stack_overflow.evmir` is the residual case, two individually safe drifting cycles
//! composed into one finite path that overflows: catching it needs the bounds carried across more
//! than one cycle-closing edge, which cannot be done without also accumulating recursion depth
//! and rejecting valid programs.
//!
//! Because one block is walked once per bound it receives, and a block with a broken stack
//! operation is normally broken at every one of them, stack-operation diagnostics are reported
//! once per block and message rather than once per entry depth.

use super::*;
use crate::op::{self, MAX_STACK_DEPTH};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::index_vec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::diagnostics::{DiagCtxt, ErrorGuaranteed};
use std::{collections::hash_map::Entry, fmt};

/// Stack-operation diagnostics already reported, keyed by block and message.
type ReportedErrors = FxHashMap<(BlockId, String), ErrorGuaranteed>;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TargetRequirement {
    StackOp(op::StackOp),
    Opcode(u8),
}

/// Target requirements already present before an EVM IR pass runs.
#[derive(Default)]
pub struct TargetSupportSnapshot(FxHashMap<TargetRequirement, usize>);

/// EVM IR verifier.
pub struct Verifier<'a> {
    dcx: &'a DiagCtxt,
    evm_version: EvmVersion,
}

impl<'a> Verifier<'a> {
    /// Creates a verifier for modules targeting `evm_version`, which reports to `dcx`.
    pub const fn for_evm_version(dcx: &'a DiagCtxt, evm_version: EvmVersion) -> Self {
        Self { dcx, evm_version }
    }

    /// Checks the target requirements EVM IR passes rely on.
    pub fn verify_before_pipeline(&self, module: &Module) {
        self.verify_stack_ops_for_evm_version(module);
    }

    /// Checks structural invariants and target support after an EVM IR pass.
    ///
    /// Pre-Byzantium reverts remain temporarily legal because the final legalization
    /// step replaces them after the optimization pipeline.
    pub fn verify_between_passes(&self, module: &Module, before: &TargetSupportSnapshot) {
        if self.verify_module_shape(module) {
            self.verify_new_target_requirements(module, before);
        }
    }

    /// Records target requirements that an input module already contains.
    pub fn target_support_snapshot(&self, module: &Module) -> TargetSupportSnapshot {
        let mut requirements = FxHashMap::default();
        for block in &module.blocks {
            for inst in &block.instructions {
                if let Some(requirement) = self.unsupported_instruction(inst) {
                    *requirements.entry(requirement).or_default() += 1;
                }
            }
            if let Some(Terminator { kind: TerminatorKind::Op(opcode), .. }) = &block.terminator
                && let Some(requirement) = self.unsupported_opcode(*opcode)
            {
                *requirements.entry(requirement).or_default() += 1;
            }
        }
        TargetSupportSnapshot(requirements)
    }

    /// Checks the module and its output target support after target legalization.
    ///
    /// This is the only stack-operation check that reports in release builds, so it runs for
    /// every EVM version: which module the check sees must not depend on whether legalization
    /// had a revert to rewrite.
    pub fn verify_after_legalization(&self, module: &Module) {
        self.verify_module(module);
        self.verify_target_support(module);
    }

    #[track_caller]
    fn error(&self, msg: impl fmt::Display) -> ErrorGuaranteed {
        // TODO: Use EVM IR debug-info spans when emitting verifier diagnostics.
        let msg = fmt::from_fn(|f| write!(f, "EVM IR verification failed: {msg}"));
        self.dcx.err(msg.to_string()).emit()
    }

    #[track_caller]
    fn error_in_block(&self, block: BlockId, msg: impl fmt::Display) -> ErrorGuaranteed {
        self.error(format_args!("block {}: {msg}", block.index()))
    }

    /// Reports `msg` for `block` unless the same message was already reported for it.
    ///
    /// The stack-operation walk enters a block once per entry depth it propagates, and a block
    /// whose stack operations are wrong is normally wrong at every one of them. Keying on the
    /// message keeps two genuinely different problems in one block visible while reporting each
    /// of them once, and the recorded guarantee is the one the first report produced.
    #[track_caller]
    fn error_in_block_once(
        &self,
        reported: &mut ReportedErrors,
        block: BlockId,
        msg: impl fmt::Display,
    ) -> ErrorGuaranteed {
        match reported.entry((block, msg.to_string())) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let guar = self.error(format_args!("block {}: {}", block.index(), entry.key().1));
                *entry.insert(guar)
            }
        }
    }

    pub fn verify_module(&self, module: &Module) {
        if self.verify_module_shape(module) {
            self.verify_stack_ops(module, &label_flows(module));
        }
    }

    pub fn verify_module_shape(&self, module: &Module) -> bool {
        let errors_before = self.dcx.err_count();
        if module.blocks.is_empty() {
            self.error("program has no blocks");
            return false;
        }
        let mut labels = FxHashSet::default();
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !labels.insert(block.label) {
                self.error_in_block(
                    block_id,
                    format_args!("duplicate block label `bb{}`", block.label),
                );
            }
            for inst in &block.instructions {
                self.verify_instruction_shape(block_id, module, inst);
            }
            self.verify_keep_with_next(block_id, block);
            let Some(term) = &block.terminator else {
                self.error_in_block(block_id, "missing terminator");
                continue;
            };
            self.verify_terminator_shape(block_id, term);
            term.kind.visit_targets(|target| {
                if !self.block_exists(module, target) {
                    self.error_in_block(
                        block_id,
                        format_args!("target block `{}` is out of range", target.index()),
                    );
                }
            });
        }

        self.dcx.err_count() == errors_before
    }

    /// Verifies every sequence `keep_with_next` glues together.
    ///
    /// The flag forbids both a block boundary and an inserted instruction after the instruction
    /// carrying it, and neither is any use unless the successor is still the instruction the
    /// sequence needs. The only sequence lowering glues is a pre-EIP-150 call's `GAS`-relative
    /// gas reserve, `push <reserve>; gas; sub; <call>`, whose margin pays for the `sub` and the
    /// call setup alone, so the pair is checked by shape: a glued `gas` is followed by a glued
    /// `sub`, and a glued `sub` by the call opcode itself. Anything else carrying the flag has no
    /// rule to check it and is rejected, which keeps a new use of the flag from passing
    /// verification unverified.
    fn verify_keep_with_next(&self, block_id: BlockId, block: &Block) {
        const CALLS: [u8; 4] = [op::CALL, op::CALLCODE, op::DELEGATECALL, op::STATICCALL];
        for (index, inst) in block.instructions.iter().enumerate() {
            if !inst.keeps_with_next() {
                continue;
            }
            let Some(next) = block.instructions.get(index + 1) else {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "`{}` must be followed by the instruction it is kept with",
                        inst.mnemonic()
                    ),
                );
                continue;
            };
            let expected = match inst.opcode {
                op::GAS if next.opcode == op::SUB && next.keeps_with_next() => continue,
                op::GAS => "a `sub` kept with the call",
                op::SUB if CALLS.contains(&next.opcode) => continue,
                op::SUB => "a call",
                _ => {
                    self.error_in_block(
                        block_id,
                        format_args!(
                            "`{}` cannot be kept with a next instruction",
                            inst.mnemonic()
                        ),
                    );
                    continue;
                }
            };
            self.error_in_block(
                block_id,
                format_args!(
                    "`{}` is kept with the next instruction, which must be {expected}, not `{}`",
                    inst.mnemonic(),
                    next.mnemonic()
                ),
            );
        }
    }

    fn verify_instruction_shape(&self, block_id: BlockId, module: &Module, inst: &Instruction) {
        if inst.is_encoded_push() {
            let Some(value) = &inst.value else {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must carry a value", inst.mnemonic()),
                );
                return;
            };
            if inst.encoding & Instruction::IMMUTABLE != 0
                && !(op::PUSH1..=op::PUSH32).contains(&inst.opcode)
            {
                self.error_in_block(
                    block_id,
                    "encoded immutable push must use a `PUSH1` through `PUSH32` opcode",
                );
            } else if inst.encoding & Instruction::IMMUTABLE == 0 && inst.opcode != op::PUSH32 {
                self.error_in_block(block_id, "encoded push must use the `PUSH32` opcode");
            }
            match inst.encoding {
                Instruction::ENCODED_PUSH => {
                    if let PushValue::Library(id) = value
                        && module.libraries.get(*id).is_none()
                    {
                        self.error_in_block(
                            block_id,
                            format_args!("library `{}` is out of range", id.index()),
                        );
                    }
                }
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::IMMUTABLE => {
                    self.verify_immutable_id(block_id, inst, value);
                }
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::DATA_SIZE => {
                    let PushValue::DataSize(size) = value else {
                        self.error_in_block(block_id, "`push_data_size` must carry a data size");
                        return;
                    };
                    match module.data.get(size.data) {
                        None => {
                            self.error_in_block(
                                block_id,
                                format_args!(
                                    "program data `{}` is out of range",
                                    size.data.index()
                                ),
                            );
                        }
                        Some(data) if data.bytes.known().is_some() => {
                            self.error_in_block(
                                block_id,
                                "`push_data_size` requires deferred data",
                            );
                        }
                        Some(_) => {}
                    }
                }
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::DATA => {
                    let PushValue::Data(data) = value else {
                        self.error_in_block(block_id, "`push_data` must carry a data ID");
                        return;
                    };
                    if data.id.index() >= module.data.len() {
                        self.error_in_block(
                            block_id,
                            format_args!("program data `{}` is out of range", data.id.index()),
                        );
                    } else if let len =
                        module.data[data.id].bytes.known().map_or(0, |bytes| bytes.len())
                        && data.offset as usize > len
                    {
                        self.error_in_block(
                            block_id,
                            format_args!(
                                "program data offset `{}` exceeds data size `{len}`",
                                data.offset
                            ),
                        );
                    }
                }
                _ => {
                    self.error_in_block(block_id, "invalid encoded push kind");
                }
            };
            if let PushValue::Block(target) = value
                && !self.block_exists(module, *target)
            {
                self.error_in_block(
                    block_id,
                    format_args!("push target block `{}` is out of range", target.index()),
                );
            }
        } else {
            if inst.value.is_some() {
                self.error_in_block(block_id, "only `push` instructions can carry a value");
            }
            if let Some(stack_op) = inst.as_stack_op() {
                if inst.opcode != stack_op.ir_opcode() {
                    self.error_in_block(block_id, "logical stack operation has the wrong opcode");
                }
                if !stack_op.is_valid() {
                    self.error_in_block(block_id, "logical stack operation has invalid depths");
                }
            } else if op::StackOp::from_single_byte_evm_opcode(inst.opcode).is_some()
                || matches!(inst.opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
            {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must use the logical stack-op form", inst.mnemonic()),
                );
            } else if inst.opcode == op::PUSH0 {
                self.error_in_block(block_id, "`push0` must use the logical push form");
            }
            if (op::PUSH1..=op::PUSH32).contains(&inst.opcode) {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must carry an encoded push value", inst.mnemonic()),
                );
            }
        }

        // Stack operation depths are checked above.
        if inst.as_stack_op().is_none()
            && !inst.is_encoded_push()
            && op::stack_io(inst.opcode).is_none()
        {
            self.error_in_block(
                block_id,
                format_args!("instruction `{}` has no known stack effect", inst.mnemonic()),
            );
        }
    }

    fn verify_immutable_id(&self, block_id: BlockId, inst: &Instruction, value: &PushValue) {
        let PushValue::Immediate(value) = value else {
            self.error_in_block(
                block_id,
                format_args!("`{}` must carry an immediate immutable ID", inst.mnemonic()),
            );
            return;
        };
        if u32::try_from(*value).ok().is_none_or(|value| value == u32::MAX) {
            self.error_in_block(block_id, "immutable ID exceeds the index limit");
        }
    }

    fn verify_terminator_shape(&self, block_id: BlockId, term: &Terminator) {
        if matches!(&term.kind, TerminatorKind::IndexedJump(targets) if targets.is_empty()) {
            self.error_in_block(block_id, "`indexed_jump` must have at least one target");
        }
        if term.metadata.keep_with_next {
            self.error_in_block(
                block_id,
                format_args!("terminator `{}` cannot be kept with a next instruction", term.kind),
            );
        }
        if let TerminatorKind::Op(opcode) = &term.kind
            && !op::is_terminal(*opcode)
        {
            self.error_in_block(
                block_id,
                format_args!("terminator opcode `0x{opcode:02x}` is not terminal"),
            );
        }
        if default_terminator_stack_effect(&term.kind).is_none() {
            self.error_in_block(
                block_id,
                format_args!("terminator `{}` has no known stack effect", term.kind),
            );
        }
    }

    /// Checks physical stack operations along generated direct control-flow edges.
    ///
    /// See the module documentation for the bounds the walk propagates and for why only the
    /// post-cycle maximum stops at a cycle-closing edge.
    fn verify_stack_ops(
        &self,
        module: &Module,
        flows: &IndexVec<BlockId, LabelFlow>,
    ) -> IndexVec<BlockId, EntryDepths> {
        let mut reported = ReportedErrors::default();
        // The entry block starts with an empty stack unless it declares a depth, and any other
        // block that declares one is also entered at it.
        let roots = module
            .blocks
            .iter_enumerated()
            .filter_map(|(block_id, block)| {
                let depth = block.metadata.entry_depth.map(usize::from);
                depth.or((block_id == BlockId::ENTRY).then_some(0)).map(|depth| (block_id, depth))
            })
            .collect::<Vec<_>>();
        let cycle_edges = cycle_edges(module, flows, roots.iter().map(|&(block_id, _)| block_id));
        let mut entry_depths = index_vec![EntryDepths::default(); module.blocks.len()];
        let mut pending = Vec::with_capacity(roots.len());
        for (block_id, depth) in roots {
            entry_depths[block_id].merge(depth, Bounds::ENTRY);
            pending.push((block_id, depth, Bounds::ENTRY));
        }
        while let Some((block_id, mut stack, bounds)) = pending.pop() {
            let block = &module.blocks[block_id];
            let term =
                block.terminator.as_ref().expect("terminator must exist after shape validation");
            let mut physical_targets = Vec::new();
            let mut valid = true;
            for (index, inst) in block.instructions.iter().enumerate() {
                if inst.is_physical_stack_op() {
                    if self
                        .apply_physical_stack_op(&mut reported, block_id, inst, &mut stack)
                        .is_err()
                    {
                        valid = false;
                        break;
                    }
                } else {
                    let effect = inst.stack_effect();
                    if self
                        .apply_effect(&mut reported, block_id, inst.mnemonic(), effect, &mut stack)
                        .is_err()
                    {
                        valid = false;
                        break;
                    }
                }
                if inst.opcode == op::JUMPI
                    && let Some(target) = branch_target(block, index)
                {
                    physical_targets.push((target, stack));
                }
            }
            if valid {
                let lowering_growth = term.kind.lowering_stack_growth(module.next_block(block_id));
                if lowering_growth != 0
                    && self
                        .ensure_stack_limit(
                            &mut reported,
                            block_id,
                            &term.kind,
                            stack + lowering_growth,
                        )
                        .is_err()
                {
                    valid = false;
                } else {
                    let effect = default_terminator_stack_effect(&term.kind)
                        .expect("terminator stack effect must be known after shape validation");
                    valid = self
                        .apply_effect(&mut reported, block_id, &term.kind, effect, &mut stack)
                        .is_ok();
                }
            }
            if !valid {
                continue;
            }
            term.kind.visit_targets(|target| physical_targets.push((target, stack)));
            for &(continuation, effect) in &flows[block_id].returns {
                if let Ok(returned) = self.apply_call_return(&mut reported, block_id, effect, stack)
                {
                    physical_targets.push((continuation, returned));
                }
            }
            for (target, depth) in physical_targets {
                let out = bounds.across(cycle_edges.contains(&(block_id, target)));
                let raised = entry_depths[target].merge(depth, out);
                if !raised.is_empty() {
                    pending.push((target, depth, raised));
                }
            }
        }
        entry_depths
    }

    fn apply_effect(
        &self,
        reported: &mut ReportedErrors,
        block_id: BlockId,
        name: impl fmt::Display,
        effect: StackEffect,
        stack: &mut usize,
    ) -> Result<(), ErrorGuaranteed> {
        let inputs = usize::from(effect.inputs);
        if *stack < inputs {
            return Err(self.error_in_block_once(
                reported,
                block_id,
                format_args!(
                    "`{name}` consumes {} stack words but only {} are available",
                    effect.inputs, *stack
                ),
            ));
        }
        *stack = *stack - inputs + usize::from(effect.outputs);
        self.ensure_stack_limit(reported, block_id, name, *stack)
    }

    /// Returns the stack depth a call returns at, `stack` being the depth at its entry.
    fn apply_call_return(
        &self,
        reported: &mut ReportedErrors,
        block_id: BlockId,
        effect: StackEffect,
        stack: usize,
    ) -> Result<usize, ErrorGuaranteed> {
        let (inputs, outputs) = (usize::from(effect.inputs), usize::from(effect.outputs));
        let Some(returned) = (stack + outputs).checked_sub(inputs) else {
            return Err(self.error_in_block_once(
                reported,
                block_id,
                format_args!(
                    "`call return` drops {} stack words but only {stack} are available",
                    inputs - outputs
                ),
            ));
        };
        self.ensure_stack_limit(reported, block_id, "call return", returned)?;
        Ok(returned)
    }

    fn apply_physical_stack_op(
        &self,
        reported: &mut ReportedErrors,
        block_id: BlockId,
        inst: &Instruction,
        stack: &mut usize,
    ) -> Result<(), ErrorGuaranteed> {
        let stack_op = inst.as_stack_op().expect("checked physical stack operation");
        let name = match stack_op {
            op::StackOp::Dup(n) => {
                if *stack < usize::from(n) {
                    return Err(self.error_in_block_once(
                        reported,
                        block_id,
                        format_args!("`dup {n}` reaches depth {n} but the stack has {}", *stack),
                    ));
                }
                *stack += 1;
                "dup"
            }
            op::StackOp::Swap(n) => {
                if *stack < usize::from(n) + 1 {
                    return Err(self.error_in_block_once(
                        reported,
                        block_id,
                        format_args!("`swap {n}` reaches depth {n} but the stack has {}", *stack),
                    ));
                }
                "swap"
            }
            op::StackOp::Exchange(_, m) => {
                if *stack < usize::from(m) + 1 {
                    return Err(self.error_in_block_once(
                        reported,
                        block_id,
                        format_args!("`exchange` reaches depth {m} but the stack has {}", *stack),
                    ));
                }
                "exchange"
            }
            op::StackOp::Pop => {
                if *stack == 0 {
                    return Err(self.error_in_block_once(
                        reported,
                        block_id,
                        "`pop` on an empty stack",
                    ));
                }
                *stack -= 1;
                "pop"
            }
        };
        self.ensure_stack_limit(reported, block_id, name, *stack)
    }

    fn ensure_stack_limit(
        &self,
        reported: &mut ReportedErrors,
        block_id: BlockId,
        name: impl fmt::Display,
        depth: usize,
    ) -> Result<(), ErrorGuaranteed> {
        if depth > MAX_STACK_DEPTH {
            Err(self.error_in_block_once(
                reported,
                block_id,
                format_args!(
                    "`{name}` grows the stack to {depth} words, exceeding the limit of {MAX_STACK_DEPTH}"
                ),
            ))
        } else {
            Ok(())
        }
    }

    fn verify_target_support(&self, module: &Module) {
        for (block_id, block) in module.blocks.iter_enumerated() {
            for inst in &block.instructions {
                if let Some(stack_op) = inst.as_stack_op() {
                    if stack_op.lowering(self.evm_version).is_none() {
                        self.error_in_block(
                            block_id,
                            format_args!("`{}` requires Amsterdam-compatible EVM", inst.mnemonic()),
                        );
                    }
                } else {
                    self.verify_opcode(block_id, inst.opcode);
                }
            }
            if let Some(Terminator { kind: TerminatorKind::Op(opcode), .. }) = &block.terminator {
                self.verify_opcode(block_id, *opcode);
            }
        }
    }

    fn opcode_awaits_legalization(&self, opcode: u8) -> bool {
        !self.evm_version.supports_returndata() && opcode == op::REVERT
    }

    fn unsupported_instruction(&self, inst: &Instruction) -> Option<TargetRequirement> {
        if let Some(stack_op) = inst.as_stack_op() {
            stack_op
                .lowering(self.evm_version)
                .is_none()
                .then_some(TargetRequirement::StackOp(stack_op))
        } else {
            self.unsupported_opcode(inst.opcode)
        }
    }

    fn unsupported_opcode(&self, opcode: u8) -> Option<TargetRequirement> {
        (!self.opcode_awaits_legalization(opcode) && !op::is_available(opcode, self.evm_version))
            .then_some(TargetRequirement::Opcode(opcode))
    }

    fn verify_new_target_requirements(&self, module: &Module, before: &TargetSupportSnapshot) {
        let mut remaining = before.0.clone();
        let mut consume = |requirement| {
            let Some(count) = remaining.get_mut(&requirement) else { return false };
            if *count == 0 {
                false
            } else {
                *count -= 1;
                true
            }
        };
        for (block_id, block) in module.blocks.iter_enumerated() {
            for inst in &block.instructions {
                let Some(requirement) = self.unsupported_instruction(inst) else { continue };
                if consume(requirement) {
                    continue;
                }
                match requirement {
                    TargetRequirement::StackOp(_) => {
                        self.error_in_block(
                            block_id,
                            format_args!("`{}` requires Amsterdam-compatible EVM", inst.mnemonic()),
                        );
                    }
                    TargetRequirement::Opcode(opcode) => self.verify_opcode(block_id, opcode),
                }
            }
            if let Some(Terminator { kind: TerminatorKind::Op(opcode), .. }) = &block.terminator
                && let Some(requirement) = self.unsupported_opcode(*opcode)
                && !consume(requirement)
            {
                self.verify_opcode(block_id, *opcode);
            }
        }
    }

    fn verify_stack_ops_for_evm_version(&self, module: &Module) {
        for (block_id, block) in module.blocks.iter_enumerated() {
            for inst in &block.instructions {
                if let Some(stack_op) = inst.as_stack_op()
                    && stack_op.lowering(self.evm_version).is_none()
                {
                    self.error_in_block(
                        block_id,
                        format_args!("`{}` requires Amsterdam-compatible EVM", inst.mnemonic()),
                    );
                }
            }
        }
    }

    fn verify_opcode(&self, block: BlockId, opcode: u8) {
        if !op::is_available(opcode, self.evm_version) {
            let name = op::mnemonic(opcode).unwrap_or("unknown");
            self.error_in_block(
                block,
                format_args!("opcode `{name}` is unavailable for `{}` EVM", self.evm_version),
            );
        }
    }

    fn block_exists(&self, module: &Module, block: BlockId) -> bool {
        block.index() < module.blocks.len()
    }

    pub fn is_valid(module: &Module) -> bool {
        let dcx = DiagCtxt::with_silent_emitter(None);
        Verifier::for_evm_version(&dcx, EvmVersion::Osaka).verify_module(module);
        dcx.has_errors().is_ok()
    }

    /// Returns upper entry depths proved by the direct-edge stack walk.
    ///
    /// The walk's finite cycle traversal is a candidate bound, not a proof: check that every
    /// direct edge preserves it, including backedges. Growing cycles, which include every
    /// recursive call and the continuations after it, remain unknown, together with their
    /// successors. Unknown bounds must not justify increasing a block's stack peak.
    ///
    /// A pushed label that the direct edges do not model, such as a return address that a branch
    /// carries into other code, has no walked depth. `bound_pushed_labels` bounds it by
    /// following the word to the jumps that consume it, and the blocks it reaches from there.
    pub fn stack_entry_upper_bounds(
        module: &Module,
        evm_version: EvmVersion,
    ) -> IndexVec<BlockId, Option<usize>> {
        let dcx = DiagCtxt::with_silent_emitter(None);
        let flows = label_flows(module);
        let depths = Verifier::for_evm_version(&dcx, evm_version).verify_stack_ops(module, &flows);
        if dcx.has_errors().is_err() {
            return index_vec![None; module.blocks.len()];
        }
        let mut upper = depths
            .iter()
            .map(|depth| depth.ingress_max.max(depth.cycle_max))
            .collect::<IndexVec<BlockId, _>>();
        let pushes = unmodeled_pushes(module, &flows);
        let entered = reachable(module, &flows, pushes.iter().map(|&(_, _, label)| label));
        let mut pending = Vec::new();
        for (block_id, block) in module.blocks.iter_enumerated() {
            let Some(stack) = upper[block_id] else {
                // The blocks that pushed labels reach are bounded below.
                if !entered.contains(block_id) {
                    pending.push(block_id);
                }
                continue;
            };
            bound_edges(block, &flows[block_id], stack, |target, stack| {
                if upper[target].is_some_and(|bound| stack > bound) {
                    pending.push(target);
                }
            });
        }
        let mut unknown = DenseBitSet::new_empty(module.blocks.len());
        forget_reached(module, &flows, &mut upper, &mut unknown, pending);
        bound_pushed_labels(module, &flows, &mut upper, &mut unknown, &pushes);
        upper
    }

    /// Returns the blocks that can be entered after a call or create has run in the same frame.
    ///
    /// Every other block runs with the empty return data buffer the frame started with. A call
    /// or create taints the successors of its block, and a continuation is tainted when the code
    /// it returns from can run one. Blocks entered through unmodeled dynamic jumps, and blocks
    /// that declare an entry of their own, are tainted along with everything they reach. When
    /// that code can run a call or create, any dynamic return may follow one.
    pub fn blocks_after_returndata_set(module: &Module) -> DenseBitSet<BlockId> {
        let flows = label_flows(module);
        let mut predecessors = index_vec![Vec::new(); module.blocks.len()];
        // The first call or create in each block.
        let sets = module
            .blocks
            .iter()
            .map(|block| {
                block
                    .instructions
                    .iter()
                    .position(|inst| inst.as_evm_opcode().is_some_and(op::sets_returndata))
            })
            .collect::<IndexVec<BlockId, _>>();
        let mut successors = Vec::new();
        for (block_id, block) in module.blocks.iter_enumerated() {
            successors.clear();
            direct_successors(&flows[block_id], block, &mut successors);
            for &target in &successors {
                predecessors[target].push(block_id);
            }
        }

        // The blocks entered without a modeled edge, and everything they reach.
        let mut pending = Vec::new();
        pending.extend(unmodeled_pushes(module, &flows).into_iter().map(|(_, _, label)| label));
        pending.extend(module.blocks.iter_enumerated().filter_map(|(block_id, block)| {
            (block_id != BlockId::ENTRY && block.metadata.entry_depth.is_some()).then_some(block_id)
        }));
        let mut after = DenseBitSet::new_empty(module.blocks.len());
        while let Some(block_id) = pending.pop() {
            if after.insert(block_id) {
                direct_successors(&flows[block_id], &module.blocks[block_id], &mut pending);
            }
        }

        // The blocks that can reach a call or create, taking an unmodeled entry to run one when
        // any does, which every dynamic jump may then reach.
        let dynamic_sets = after.iter().any(|block_id| sets[block_id].is_some());
        let mut runs = DenseBitSet::new_empty(module.blocks.len());
        pending.extend(sets.indices().filter(|&block_id| sets[block_id].is_some()));
        if dynamic_sets {
            pending.extend(
                module.blocks.indices().filter(|&id| jumps_dynamically(&module.blocks[id])),
            );
        }
        while let Some(block_id) = pending.pop() {
            if runs.insert(block_id) {
                pending.extend(predecessors[block_id].iter().copied());
            }
        }

        for (block_id, block) in module.blocks.iter_enumerated() {
            if let Some(at) = sets[block_id] {
                successors_after(&flows[block_id], block, at, &mut pending);
            } else if let Some(TerminatorKind::Jump(callee)) =
                block.terminator.as_ref().map(|term| &term.kind)
                && runs.contains(*callee)
            {
                pending.extend(flows[block_id].returns.iter().map(|&(next, _)| next));
            }
        }
        while let Some(block_id) = pending.pop() {
            if after.insert(block_id) {
                direct_successors(&flows[block_id], &module.blocks[block_id], &mut pending);
            }
        }
        after
    }
}

/// The entry depths the stack-operation walk has propagated into a block.
///
/// Both bounds are split by how many cycle-closing edges the walk reached the block over, so that
/// drift carried around a cycle can be cut without also discarding a first arrival that merely
/// happens to come in over a cycle-closing edge. See the module documentation.
#[derive(Clone, Copy, Debug, Default)]
struct EntryDepths {
    /// Lowest entry depth over paths that cross no cycle-closing edge.
    ingress_min: Option<usize>,
    /// Lowest entry depth over paths that cross exactly one cycle-closing edge.
    cycle_min: Option<usize>,
    /// Highest entry depth over paths that cross no cycle-closing edge.
    ingress_max: Option<usize>,
    /// Highest entry depth over paths that cross exactly one cycle-closing edge.
    cycle_max: Option<usize>,
}

impl EntryDepths {
    /// Merges `depth` into the bounds named by `bounds`, returning the ones it moved.
    fn merge(&mut self, depth: usize, bounds: Bounds) -> Bounds {
        let mut moved = Bounds::default();
        if bounds.ingress_min && self.ingress_min.is_none_or(|min| depth < min) {
            self.ingress_min = Some(depth);
            moved.ingress_min = true;
        }
        if bounds.cycle_min && self.cycle_min.is_none_or(|min| depth < min) {
            self.cycle_min = Some(depth);
            moved.cycle_min = true;
        }
        if bounds.ingress_max && self.ingress_max.is_none_or(|max| depth > max) {
            self.ingress_max = Some(depth);
            moved.ingress_max = true;
        }
        if bounds.cycle_max && self.cycle_max.is_none_or(|max| depth > max) {
            self.cycle_max = Some(depth);
            moved.cycle_max = true;
        }
        moved
    }
}

/// Which of a block's [`EntryDepths`] a walk item stands for.
///
/// One item can stand for several at once, which is the common case: straight-line code enters
/// every block at one depth, so all four bounds travel together and each block is walked once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Bounds {
    ingress_min: bool,
    cycle_min: bool,
    ingress_max: bool,
    cycle_max: bool,
}

impl Bounds {
    /// The bounds a block the walk starts at begins with.
    const ENTRY: Self =
        Self { ingress_min: true, cycle_min: false, ingress_max: true, cycle_max: false };

    fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// The bounds this item still stands for after crossing one control-flow edge.
    ///
    /// A cycle-closing edge promotes each ingress bound to the matching post-cycle bound and
    /// drops the post-cycle bounds: one traversal of the cycle is checked, a second is not. The
    /// two bounds are treated alike so that a cycle which drops a word every round is no more
    /// rejected than one which leaves a word behind.
    const fn across(self, closes_cycle: bool) -> Self {
        if closes_cycle {
            Self {
                ingress_min: false,
                cycle_min: self.ingress_min,
                ingress_max: false,
                cycle_max: self.ingress_max,
            }
        } else {
            self
        }
    }
}

/// The addresses of blocks that a block pushes and the calls that consume them.
///
/// A call pushes the address of the block it returns to, the continuation, and then jumps into
/// code whose [`CallEffect`] says how it returns: through a dynamic jump to the word at a fixed
/// position, at a known stack depth. That jump carries no static target, so the continuation
/// would have no edge and no entry depth. This follows the pushed addresses through the stack
/// operations of the block instead, so that where the block ends with a jump to such code the
/// word at the return position names the continuation.
#[derive(Default)]
struct LabelFlow {
    /// The continuations the block's terminator calls into, with the effect of that call.
    returns: SmallVec<[(BlockId, StackEffect); 1]>,
    /// The pushes, by instruction index, of addresses that a call consumes.
    ///
    /// Any other pushed address may be copied, stored, or carried into a successor, and so can
    /// enter its block at a depth the walk does not model.
    accounted: SmallVec<[usize; 2]>,
}

/// Follows the addresses a block pushes to the call that consumes them.
fn label_flow(module: &Module, block: &Block, may_return: &DenseBitSet<BlockId>) -> LabelFlow {
    /// A pushed address on the stack: its position from the block's entry height, the block it
    /// names, and the index of the instruction that pushed it.
    struct Word {
        slot: isize,
        block: BlockId,
        push: usize,
    }

    fn swap_slots(words: &mut [Word], a: isize, b: isize) {
        for word in words {
            if word.slot == a {
                word.slot = b;
            } else if word.slot == b {
                word.slot = a;
            }
        }
    }

    let mut flow = LabelFlow::default();
    let mut words = SmallVec::<[Word; 4]>::new();
    let mut height = 0isize;
    for (at, inst) in block.instructions.iter().enumerate() {
        if let Some(label) = inst.pushed_block() {
            // More addresses than the stack holds is rejected by the walk; do not scan them.
            if words.len() >= MAX_STACK_DEPTH {
                return LabelFlow::default();
            }
            words.push(Word { slot: height, block: label, push: at });
            height += 1;
            continue;
        }
        // A branch that can reach a dynamic jump may take the addresses below its operands there,
        // where they are entered at another depth.
        if inst.opcode == op::JUMPI
            && branch_target(block, at).is_none_or(|taken| may_return.contains(taken))
        {
            words.clear();
        }
        let slot_of = |words: &[Word], slot: isize| words.iter().position(|w| w.slot == slot);
        match inst.as_stack_op() {
            Some(op::StackOp::Dup(n)) => {
                // A copy can reach anywhere, so the address is no longer followed.
                if let Some(index) = slot_of(&words, height - isize::from(n)) {
                    words.swap_remove(index);
                }
                height += 1;
            }
            Some(op::StackOp::Swap(n)) => {
                swap_slots(&mut words, height - 1, height - 1 - isize::from(n));
            }
            Some(op::StackOp::Exchange(n, m)) => {
                swap_slots(&mut words, height - 1 - isize::from(n), height - 1 - isize::from(m));
            }
            Some(op::StackOp::Pop) => {
                // A branch earlier in the block may have returned through the word.
                if let Some(index) = slot_of(&words, height - 1) {
                    words.swap_remove(index);
                }
                height -= 1;
            }
            None => {
                let effect = inst.stack_effect();
                let floor = height - isize::from(effect.inputs);
                words.retain(|word| word.slot < floor);
                height = floor + isize::from(effect.outputs);
            }
        }
    }
    match block.terminator.as_ref().map(|term| &term.kind) {
        Some(TerminatorKind::Jump(target))
            if let Some(call) = module.blocks[*target].metadata.call_effect
                && let Some(index) =
                    words.iter().position(|w| w.slot == height - 1 - isize::from(call.ret)) =>
        {
            let word = words.swap_remove(index);
            flow.returns.push((word.block, call.effect));
            flow.accounted.push(word.push);
        }
        // A function inlined into its caller returns through a jump to the address the caller
        // pushed. A push directly before the jump is left to the shape check, as it is a branch.
        Some(TerminatorKind::Op(op::JUMP))
            if let Some(index) = words.iter().position(|w| w.slot == height - 1)
                && words[index].push + 1 != block.instructions.len() =>
        {
            let word = words.swap_remove(index);
            flow.returns.push((word.block, StackEffect::new(0, 0)));
            flow.accounted.push(word.push);
        }
        _ => {}
    }
    flow
}

fn label_flows(module: &Module) -> IndexVec<BlockId, LabelFlow> {
    let may_return = reach_dynamic_jumps(module);
    module.blocks.iter().map(|block| label_flow(module, block, &may_return)).collect()
}

/// Returns the blocks from which a dynamic jump or branch is reachable. A call reaches its
/// callee, whose return is one, so the calls' own return edges are not needed.
fn reach_dynamic_jumps(module: &Module) -> DenseBitSet<BlockId> {
    let mut predecessors = index_vec![Vec::new(); module.blocks.len()];
    let mut reaching = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = Vec::new();
    let mut successors = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if jumps_dynamically(block) {
            pending.push(block_id);
        }
        successors.clear();
        direct_successors(&LabelFlow::default(), block, &mut successors);
        for &target in &successors {
            predecessors[target].push(block_id);
        }
    }
    while let Some(block_id) = pending.pop() {
        if reaching.insert(block_id) {
            pending.extend(predecessors[block_id].iter().copied());
        }
    }
    reaching
}

/// Returns the block that the `JUMPI` at `at` branches to, when the instruction before it pushes
/// one.
fn branch_target(block: &Block, at: usize) -> Option<BlockId> {
    at.checked_sub(1).and_then(|previous| block.instructions[previous].pushed_block())
}

/// Returns whether `block` can end in a jump or a branch through a stack value.
fn jumps_dynamically(block: &Block) -> bool {
    let dynamic_branch = block
        .instructions
        .iter()
        .enumerate()
        .any(|(at, inst)| inst.opcode == op::JUMPI && branch_target(block, at).is_none());
    dynamic_branch
        || matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::JUMP))
        )
}

/// Returns the pushes of a block's address, with the block and index of each, other than to
/// branch to it or to return through it as a call's continuation. Such an address may be copied,
/// stored, or carried to a dynamic jump, so its block can be entered at a depth, and in a state,
/// the direct edges do not model.
fn unmodeled_pushes(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
) -> Vec<(BlockId, usize, BlockId)> {
    let mut pushes = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for (at, inst) in block.instructions.iter().enumerate() {
            if let Some(target) = inst.pushed_block()
                && !block
                    .instructions
                    .get(at + 1)
                    .is_some_and(|next| matches!(next.as_evm_opcode(), Some(op::JUMP | op::JUMPI)))
                && !flows[block_id].accounted.contains(&at)
            {
                pushes.push((block_id, at, target));
            }
        }
    }
    pushes
}

/// Appends every direct control-flow successor of `block`, in walk order.
///
/// The `push bbN; jumpi` pair is a physical conditional branch inside a block, so its target is
/// an edge as well, and so is the continuation of a call. Dynamic jumps through a stack value,
/// which is how an internal function returns, carry no target and therefore no edge.
fn direct_successors(flow: &LabelFlow, block: &Block, out: &mut Vec<BlockId>) {
    successors_after(flow, block, 0, out);
}

/// Appends the direct successors of `block` that execution reaches after the instruction at
/// `start`: the targets of its later branches, and those of its terminator.
fn successors_after(flow: &LabelFlow, block: &Block, start: usize, out: &mut Vec<BlockId>) {
    for (index, inst) in block.instructions.iter().enumerate().skip(start) {
        if inst.opcode == op::JUMPI
            && let Some(target) = branch_target(block, index)
        {
            out.push(target);
        }
    }
    if let Some(term) = &block.terminator {
        term.kind.visit_targets(|target| out.push(target));
    }
    out.extend(flow.returns.iter().map(|&(continuation, _)| continuation));
}

/// Returns the blocks `roots` reach over direct edges, themselves included.
fn reachable(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
    roots: impl IntoIterator<Item = BlockId>,
) -> DenseBitSet<BlockId> {
    let mut reached = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(block_id) = pending.pop() {
        if reached.insert(block_id) {
            direct_successors(&flows[block_id], &module.blocks[block_id], &mut pending);
        }
    }
    reached
}

/// Marks `roots` and the blocks they reach as unknown, and drops their bounds.
fn forget_reached(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
    upper: &mut IndexVec<BlockId, Option<usize>>,
    unknown: &mut DenseBitSet<BlockId>,
    mut pending: Vec<BlockId>,
) {
    while let Some(block_id) = pending.pop() {
        if unknown.insert(block_id) {
            upper[block_id] = None;
            direct_successors(&flows[block_id], &module.blocks[block_id], &mut pending);
        }
    }
}

/// Returns the stack depth after an operation with `effect`, which stops at an empty stack where
/// the walk would report an underflow.
const fn depth_after(stack: usize, effect: StackEffect) -> usize {
    stack.saturating_sub(effect.inputs as usize) + effect.outputs as usize
}

/// Returns the depth a block entered at `entry` ends at, after its terminator.
fn exit_depth(block: &Block, entry: usize) -> usize {
    let terminator =
        block.terminator.iter().filter_map(|term| default_terminator_stack_effect(&term.kind));
    block
        .instructions
        .iter()
        .map(Instruction::stack_effect)
        .chain(terminator)
        .fold(entry, depth_after)
}

/// Calls `edge` with each direct successor of `block` and the depth it is entered at when the
/// block is entered at `stack`, as the walk propagates it.
fn bound_edges(
    block: &Block,
    flow: &LabelFlow,
    mut stack: usize,
    mut edge: impl FnMut(BlockId, usize),
) {
    for (at, inst) in block.instructions.iter().enumerate() {
        stack = depth_after(stack, inst.stack_effect());
        if inst.as_evm_opcode() == Some(op::JUMPI)
            && let Some(target) = branch_target(block, at)
        {
            edge(target, stack);
        }
    }
    if let Some(term) = &block.terminator
        && let Some(effect) = default_terminator_stack_effect(&term.kind)
    {
        stack = depth_after(stack, effect);
        term.kind.visit_targets(|target| edge(target, stack));
        for &(continuation, effect) in &flow.returns {
            edge(continuation, depth_after(stack, effect));
        }
    }
}

/// The block that ends with the jump that consumes a followed word, and the effect the jump has
/// on the stack it leaves for the word's block: a call's, or none for a dynamic jump.
type Consumer = (BlockId, StackEffect);

/// Returns the position `word` has after the words at the positions `a` and `b` swap.
const fn swapped(word: usize, a: usize, b: usize) -> usize {
    if word == a {
        b
    } else if word == b {
        a
    } else {
        word
    }
}

/// The words a trace follows, by distance from the top of the stack.
#[derive(Clone, PartialEq, Eq)]
struct Words {
    /// The word whose consumers the trace looks for.
    word: usize,
    /// The labels the trace has seen pushed after it, sorted by distance. A jump through one goes
    /// to the block it names.
    labels: SmallVec<[(usize, BlockId); 4]>,
}

impl Words {
    /// Applies an operation. Returns `None` when it consumes the followed word.
    fn apply(&mut self, effect: StackEffect) -> Option<()> {
        let (inputs, outputs) = (usize::from(effect.inputs), usize::from(effect.outputs));
        self.word = self.word.checked_sub(inputs)? + outputs;
        self.labels.retain(|(distance, _)| *distance >= inputs);
        for (distance, _) in &mut self.labels {
            *distance = *distance - inputs + outputs;
        }
        Some(())
    }

    /// Applies a stack operation. Returns `None` when it copies the followed word, and
    /// `Some(false)` when it discards it.
    fn apply_stack_op(&mut self, stack_op: op::StackOp) -> Option<bool> {
        match stack_op {
            op::StackOp::Dup(n) => {
                if self.word + 1 == usize::from(n) {
                    return None;
                }
                self.word += 1;
                for (distance, _) in &mut self.labels {
                    *distance += 1;
                }
            }
            op::StackOp::Swap(n) => self.swap(0, usize::from(n)),
            op::StackOp::Exchange(n, m) => self.swap(usize::from(n), usize::from(m)),
            op::StackOp::Pop => return Some(self.apply(StackEffect::new(1, 0)).is_some()),
        }
        Some(true)
    }

    fn swap(&mut self, a: usize, b: usize) {
        self.word = swapped(self.word, a, b);
        for (distance, _) in &mut self.labels {
            *distance = swapped(*distance, a, b);
        }
        self.labels.sort_unstable();
    }

    /// Returns the label at `distance`, if the trace saw it pushed.
    fn label_at(&self, distance: usize) -> Option<BlockId> {
        self.labels.iter().find(|(at, _)| *at == distance).map(|&(_, label)| label)
    }

    /// Keeps the labels that `other` holds as well.
    fn intersect(&mut self, other: &Self) {
        self.labels.retain(|label| other.labels.contains(label));
    }
}

/// Follows the word pushed by the instruction at `push` in `start` over the direct edges, and
/// returns the jumps that consume it, or `None` when it can leave the followed paths: it is
/// copied, read by another instruction, passed to a callee, carried to a dynamic jump through a
/// word the trace cannot name, or it sits at different distances from the top at the entry of one
/// block. A path where the word leaves the trace but no dynamic jump can follow is dropped, as
/// nothing is left to consume it.
///
/// A jump through a label that the trace saw pushed after the word goes to the block it names,
/// where the trace continues. A callee leaves the words below its arguments as they are, as the
/// call effect that models it assumes, so the word follows the call's continuation.
fn trace_pushed_word(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
    may_jump: &DenseBitSet<BlockId>,
    start: BlockId,
    push: usize,
) -> Option<Vec<Consumer>> {
    let mut consumers = Vec::new();
    // The words at the entry of each block reached.
    let mut entries = FxHashMap::<BlockId, Words>::default();
    // The block to run, the first instruction to run in it, and the words.
    let mut pending = vec![(start, push + 1, Words { word: 0, labels: SmallVec::new() })];
    'blocks: while let Some((block_id, from, mut words)) = pending.pop() {
        // Gives up on the path unless no dynamic jump is left on it.
        macro_rules! lost {
            () => {
                if may_jump.contains(block_id) {
                    return None;
                } else {
                    continue 'blocks;
                }
            };
        }
        if from == 0 {
            match entries.entry(block_id) {
                Entry::Occupied(mut entry) => {
                    let known = entry.get_mut();
                    if known.word != words.word {
                        lost!();
                    }
                    let before = known.labels.len();
                    known.intersect(&words);
                    if known.labels.len() == before {
                        continue;
                    }
                    words = known.clone();
                }
                Entry::Vacant(entry) => {
                    entry.insert(words.clone());
                }
            }
        }
        let block = &module.blocks[block_id];
        for (at, inst) in block.instructions.iter().enumerate().skip(from) {
            if let Some(stack_op) = inst.as_stack_op() {
                match words.apply_stack_op(stack_op) {
                    Some(true) => continue,
                    Some(false) => continue 'blocks,
                    None => lost!(),
                }
            }
            if words.apply(inst.stack_effect()).is_none() {
                lost!();
            }
            match inst.as_evm_opcode() {
                Some(op::JUMPI) => {
                    let Some(target) = branch_target(block, at) else {
                        lost!();
                    };
                    pending.push((target, 0, words.clone()));
                }
                Some(op::JUMP) => return None,
                _ if inst.is_encoded_push() => {
                    if let Some(label) = inst.pushed_block() {
                        words.labels.insert(0, (0, label));
                    }
                }
                _ => {}
            }
        }
        match &block.terminator.as_ref()?.kind {
            TerminatorKind::Op(op::JUMP) => {
                if words.word == 0 {
                    consumers.push((block_id, StackEffect::new(0, 0)));
                    continue;
                }
                let continuation = words
                    .label_at(0)
                    .or_else(|| flows[block_id].returns.first().map(|&(next, _)| next))?;
                words.apply(StackEffect::new(1, 0)).expect("the word is below the top");
                pending.push((continuation, 0, words));
            }
            TerminatorKind::Jump(target)
                if let Some(call) = module.blocks[*target].metadata.call_effect =>
            {
                if words.word == usize::from(call.ret) {
                    consumers.push((block_id, call.effect));
                    continue;
                }
                let Some(continuation) = words
                    .label_at(usize::from(call.ret))
                    .or_else(|| flows[block_id].returns.first().map(|&(next, _)| next))
                else {
                    lost!();
                };
                if words.apply(call.effect).is_none() {
                    lost!();
                }
                pending.push((continuation, 0, words));
            }
            kind => {
                let effect = default_terminator_stack_effect(kind)?;
                if words.apply(effect).is_none() {
                    lost!();
                }
                kind.visit_targets(|target| pending.push((target, 0, words.clone())));
            }
        }
    }
    Some(consumers)
}

/// Bounds the entry depth of every pushed label that no direct edge enters, by following each
/// pushed word to the jumps that consume it, and propagates the bound to the blocks it reaches.
///
/// A jump through the word enters the label's block at the depth the jump leaves its own block,
/// which is the bound of the walk's depth at that block, less the address the jump pops. A label
/// whose word escapes the trace has no bound, and neither has what it reaches. A raised block
/// can raise the jumps in it, and a jump can sit in a block that another label bounds, so the
/// bounds are repeated until none changes.
fn bound_pushed_labels(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
    upper: &mut IndexVec<BlockId, Option<usize>>,
    unknown: &mut DenseBitSet<BlockId>,
    pushes: &[(BlockId, usize, BlockId)],
) {
    if pushes.is_empty() {
        return;
    }
    let may_jump = reach_dynamic_jumps(module);
    let mut consumers = FxHashMap::<BlockId, Vec<Consumer>>::default();
    let mut escaped = Vec::new();
    for &(block_id, at, label) in pushes {
        match trace_pushed_word(module, flows, &may_jump, block_id, at) {
            Some(found) => consumers.entry(label).or_default().extend(found),
            None => escaped.push(label),
        }
    }
    forget_reached(module, flows, upper, unknown, escaped);
    let mut changed = true;
    while std::mem::take(&mut changed) {
        // The labels with a consumer in a block that has no bound yet.
        let mut waiting = Vec::new();
        'labels: for (&label, found) in &consumers {
            if unknown.contains(label) {
                continue;
            }
            let mut bound = 0;
            for &(block_id, effect) in found {
                let block = &module.blocks[block_id];
                let Some(entry) = upper[block_id] else {
                    waiting.push(label);
                    continue 'labels;
                };
                bound = bound.max(depth_after(exit_depth(block, entry), effect));
            }
            if !found.is_empty() && upper[label].is_none_or(|known| known < bound) {
                upper[label] = Some(bound);
                changed = true;
                raise_reached(module, flows, upper, unknown, label);
            }
        }
        // A consumer may get its bound from another label's, so a label gives up only when the
        // others have nothing left to raise.
        if !changed && !waiting.is_empty() {
            forget_reached(module, flows, upper, unknown, waiting);
            changed = true;
        }
    }
}

/// Raises the bounds of the blocks `start` reaches to the depths its bound gives them. A block
/// the stack limit stops it from raising lies on a growing cycle, which has no bound, and so
/// does what it reaches.
fn raise_reached(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
    upper: &mut IndexVec<BlockId, Option<usize>>,
    unknown: &mut DenseBitSet<BlockId>,
    start: BlockId,
) {
    let mut pending = vec![start];
    let mut growing = Vec::new();
    while let Some(block_id) = pending.pop() {
        let Some(stack) = upper[block_id] else { continue };
        bound_edges(&module.blocks[block_id], &flows[block_id], stack, |target, stack| {
            if stack > MAX_STACK_DEPTH {
                growing.push(target);
            } else if !unknown.contains(target) && upper[target].is_none_or(|bound| stack > bound) {
                upper[target] = Some(stack);
                pending.push(target);
            }
        });
        forget_reached(module, flows, upper, unknown, std::mem::take(&mut growing));
    }
}

/// Returns the edges that close a cycle in a depth-first walk from the entry block.
///
/// Removing them leaves an acyclic graph, which is what bounds the stack-depth walk. Only edges
/// reachable from the entry are classified; the walk never leaves that region either.
fn cycle_edges(
    module: &Module,
    flows: &IndexVec<BlockId, LabelFlow>,
    roots: impl IntoIterator<Item = BlockId>,
) -> FxHashSet<(BlockId, BlockId)> {
    /// Depth-first states: not yet reached, on the current path, and fully walked.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Unseen,
        OnPath,
        Done,
    }

    let mut edges = FxHashSet::default();
    let mut states = index_vec![State::Unseen; module.blocks.len()];
    // Successors of every block on the current path, each frame owning the tail of the buffer
    // from its recorded start.
    let mut successors = Vec::new();
    let mut path = Vec::new();
    for root in roots {
        if states[root] != State::Unseen {
            continue;
        }
        path.push((root, 0));
        states[root] = State::OnPath;
        direct_successors(&flows[root], &module.blocks[root], &mut successors);
        while let Some(&(block_id, start)) = path.last() {
            if successors.len() == start {
                states[block_id] = State::Done;
                path.pop();
                continue;
            }
            let target = successors.pop().expect("frame owns the buffer tail");
            match states[target] {
                State::Unseen => {
                    states[target] = State::OnPath;
                    path.push((target, successors.len()));
                    direct_successors(&flows[target], &module.blocks[target], &mut successors);
                }
                State::OnPath => {
                    edges.insert((block_id, target));
                }
                State::Done => {}
            }
        }
    }
    edges
}
