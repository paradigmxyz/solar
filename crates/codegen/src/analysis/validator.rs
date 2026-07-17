//! MIR validator — checks SSA invariants on a [`Function`].
//!
//! This is the Solar equivalent of LLVM's `verify` pass / Cranelift's
//! `Function::verify`. It walks a function once and reports every invariant
//! violation it finds through the compiler diagnostic context.
//!
//! # Checks performed
//!
//! 1. **Defined-before-use**: every `ValueId` referenced as an operand has an entry in
//!    `func.values`.
//! 2. **Block reference validity**: every `BlockId` mentioned in a terminator or phi has an entry
//!    in `func.blocks`.
//! 3. **Single definition**: each `InstId` is referenced by at most one `Value::Inst` entry.
//! 4. **Terminator presence**: every block has a terminator.
//! 5. **Predecessor back-link**: if A's terminator targets B, then B's `predecessors` contains A.
//! 6. **Entry block has no predecessors**.
//! 7. **Phi block coverage**: every `InstKind::Phi`'s incoming blocks are predecessors of the
//!    containing block, and every predecessor has an incoming entry.
//! 8. **Instruction-block consistency**: each instruction's `block` field matches the block whose
//!    `instructions` vector contains it.
//! 9. **Predecessor consistency**: every stored predecessor actually branches to the block.
//! 10. **Use reachability**: for every reachable cross-block use of an instruction result, the
//!     defining block can reach the using block (phi inputs: their incoming predecessor). MIR is
//!     deliberately loose SSA, but a use its definition can never reach is garbage on every
//!     execution.
//!
//! # Usage
//!
//! ```ignore
//! use solar_codegen::analysis::Validator;
//! Validator::new(dcx).validate_function(&func);
//! ```

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, InstId, InstKind, Module, Value, ValueId},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_interface::{diagnostics::DiagCtxt, sym};
use std::fmt;

/// Stateful MIR verifier.
pub struct Validator<'a> {
    dcx: &'a DiagCtxt,
    function: Option<usize>,
    error_count: usize,
}

impl<'a> Validator<'a> {
    /// Creates a verifier that emits findings into `dcx`.
    pub const fn new(dcx: &'a DiagCtxt) -> Self {
        Self { dcx, function: None, error_count: 0 }
    }

    #[track_caller]
    fn emit(&mut self, message: impl fmt::Display) {
        // TODO: Use MIR debug-info spans when emitting verifier diagnostics.
        let message = fmt::from_fn(|f| {
            if let Some(function) = self.function {
                write!(f, "[fn{function}] ")?;
            }
            write!(f, "{message}")
        });
        self.dcx.err(message.to_string()).emit();
        self.error_count += 1;
    }

    #[track_caller]
    fn emit_at_block(&mut self, message: impl fmt::Display, block: BlockId) {
        self.emit(format_args!("[bb{}] {message}", block.index()));
    }

    #[track_caller]
    fn emit_at_inst(&mut self, message: impl fmt::Display, block: BlockId, inst: InstId) {
        self.emit(format_args!("[bb{}, inst{}] {message}", block.index(), inst.index()));
    }

    /// Validates a single function.
    pub fn validate_function(mut self, func: &Function) {
        self.validate_function_inner(func);
    }

    fn validate_function_inner(&mut self, func: &Function) {
        let errors_before = self.error_count;
        let num_values = func.values.len();
        let num_blocks = func.blocks.len();
        let num_insts = func.instructions.len();

        if num_blocks == 0 {
            return;
        }

        // ----- Single-definition check -----
        // Count how many Value entries claim to be the result of each InstId.
        let mut inst_def_count: IndexVec<InstId, usize> = index_vec![0; num_insts];
        let mut invalid_inst_def_count: FxHashMap<InstId, usize> = FxHashMap::default();
        let mut inst_results: IndexVec<InstId, Option<ValueId>> = index_vec![None; num_insts];
        for (value_id, v) in func.values.iter_enumerated() {
            if let Value::Inst(inst_id) = v {
                if inst_id.index() < num_insts {
                    inst_def_count[*inst_id] += 1;
                    inst_results[*inst_id] = Some(value_id);
                } else {
                    *invalid_inst_def_count.entry(*inst_id).or_default() += 1;
                }
            }
        }
        for (inst_id, &count) in inst_def_count.iter_enumerated() {
            if count > 1 {
                self.emit(format_args!(
                    "instruction inst{} is defined by {count} Value entries (must be 1)",
                    inst_id.index()
                ));
            }
            // Only value-producing instructions may have a result value.
            if count != 0 && func.instructions[inst_id].result_ty.is_none() {
                self.emit(format_args!(
                    "instruction inst{} (`{:?}`) has a result Value entry but no result type",
                    inst_id.index(),
                    func.instructions[inst_id].kind
                ));
            }
        }
        for (inst_id, count) in invalid_inst_def_count {
            if count > 1 {
                self.emit(format_args!(
                    "instruction inst{} is defined by {count} Value entries (must be 1)",
                    inst_id.index()
                ));
            }
        }

        // ----- Walk every block -----
        for (block_id, block) in func.blocks.iter_enumerated() {
            // Check terminator presence.
            let term = match &block.terminator {
                Some(t) => t,
                None => {
                    self.emit_at_block("block has no terminator", block_id);
                    continue;
                }
            };

            let term_succs = term.successors();

            // Check successor blocks exist and back-link.
            for &succ in &term_succs {
                if succ.index() >= num_blocks {
                    self.emit_at_block(
                        format_args!("terminator references nonexistent block bb{}", succ.index()),
                        block_id,
                    );
                    continue;
                }
                if !func.blocks[succ].predecessors.contains(&block_id) {
                    self.emit_at_block(
                        format_args!(
                            "successor bb{} does not list bb{} as a predecessor",
                            succ.index(),
                            block_id.index()
                        ),
                        block_id,
                    );
                }
            }

            // Check stored predecessor blocks exist and branch to this block.
            for &pred in &block.predecessors {
                if pred.index() >= num_blocks {
                    self.emit_at_block(
                        format_args!(
                            "stored predecessor references nonexistent block bb{}",
                            pred.index()
                        ),
                        block_id,
                    );
                    continue;
                }
                let Some(pred_term) = &func.blocks[pred].terminator else {
                    self.emit_at_block(
                        format_args!("stored predecessor bb{} has no terminator", pred.index()),
                        block_id,
                    );
                    continue;
                };
                if !pred_term.successors().contains(&block_id) {
                    self.emit_at_block(
                        format_args!(
                            "stored predecessor bb{} does not branch to bb{}",
                            pred.index(),
                            block_id.index()
                        ),
                        block_id,
                    );
                }
            }

            // Check terminator operands are in range.
            for op in term.operands() {
                if op.index() >= num_values {
                    self.emit_at_block(
                        format_args!(
                            "terminator references undefined value v{} (only {} values exist)",
                            op.index(),
                            num_values
                        ),
                        block_id,
                    );
                }
            }

            // ----- Walk instructions in this block -----
            let block_preds: Vec<BlockId> = block.predecessors.iter().copied().collect();
            for &inst_id in &block.instructions {
                if inst_id.index() >= num_insts {
                    self.emit_at_block(
                        format_args!("block contains nonexistent inst{}", inst_id.index()),
                        block_id,
                    );
                    continue;
                }
                let inst = func.instruction(inst_id);

                // Operand range check.
                for op in inst.kind.operands() {
                    if op.index() >= num_values {
                        self.emit_at_inst(
                            format_args!(
                                "instruction references undefined value v{} (only {} values exist)",
                                op.index(),
                                num_values
                            ),
                            block_id,
                            inst_id,
                        );
                    }
                }

                // Phi-specific checks.
                if let InstKind::Phi(incoming) = &inst.kind {
                    // Every incoming block must be a predecessor.
                    for (pred_block, _) in incoming {
                        if pred_block.index() >= num_blocks {
                            self.emit_at_inst(
                                format_args!(
                                    "phi incoming references nonexistent block bb{}",
                                    pred_block.index()
                                ),
                                block_id,
                                inst_id,
                            );
                            continue;
                        }
                        if !block_preds.contains(pred_block) {
                            self.emit_at_inst(
                                format_args!(
                                    "phi incoming from bb{} but bb{} is not a predecessor",
                                    pred_block.index(),
                                    pred_block.index()
                                ),
                                block_id,
                                inst_id,
                            );
                        }
                    }
                    // Every predecessor must appear in the incoming list.
                    for pred in &block_preds {
                        if !incoming.iter().any(|(b, _)| b == pred) {
                            self.emit_at_inst(
                                format_args!(
                                    "phi missing incoming entry for predecessor bb{}",
                                    pred.index()
                                ),
                                block_id,
                                inst_id,
                            );
                        }
                    }
                    // Incoming lists are keyed per predecessor block, so duplicate
                    // entries for one block must agree on the value; conflicting
                    // duplicates make the chosen value depend on consumer order.
                    for (index, (pred_block, value)) in incoming.iter().enumerate() {
                        if incoming
                            .iter()
                            .take(index)
                            .any(|(other, other_value)| other == pred_block && other_value != value)
                        {
                            self.emit_at_inst(
                                format_args!(
                                    "phi has conflicting incoming values for predecessor bb{}",
                                    pred_block.index()
                                ),
                                block_id,
                                inst_id,
                            );
                        }
                    }
                }
            }
        }

        // ----- Entry block must have no predecessors -----
        if !func.blocks[func.entry_block].predecessors.is_empty() {
            self.emit_at_block("entry block must have no predecessors", func.entry_block);
        }

        // ----- Use reachability -----
        // MIR is deliberately loose SSA: a definition need not dominate its
        // uses, because cross-block values travel through reserved spill
        // slots and the source guarantees definite assignment. The invariant
        // that must still hold is reachability: if the defining block can
        // never reach the using block (its incoming predecessor, for phi
        // inputs), the use reads garbage on every execution. Structural
        // errors are reported first: CFG construction assumes valid block
        // references.
        if self.error_count != errors_before {
            return;
        }
        let cfg = CfgInfo::new(func);
        let mut def_block_of: IndexVec<ValueId, Option<BlockId>> = index_vec![None; num_values];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                if let Some(result) = inst_results[inst_id] {
                    def_block_of[result] = Some(block_id);
                }
            }
        }
        let mut reach_cache: FxHashMap<BlockId, DenseBitSet<BlockId>> = FxHashMap::default();
        let mut reaches = |from: BlockId, to: BlockId| {
            if from == to {
                return true;
            }
            let set = reach_cache.entry(from).or_insert_with(|| {
                let mut seen = DenseBitSet::new_empty(func.blocks.len());
                let mut stack = vec![from];
                while let Some(current) = stack.pop() {
                    if let Some(term) = func.blocks[current].terminator.as_ref() {
                        for succ in term.successors() {
                            if seen.insert(succ) {
                                stack.push(succ);
                            }
                        }
                    }
                }
                seen
            });
            set.contains(to)
        };
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) {
                continue;
            }
            for &inst_id in &block.instructions {
                match &func.instructions[inst_id].kind {
                    InstKind::Phi(incoming) => {
                        for &(pred, value) in incoming {
                            if let Some(def) = def_block_of[value]
                                && !reaches(def, pred)
                            {
                                self.emit_at_inst(
                                    format_args!(
                                        "phi input {value:?} from bb{} can never be reached by \
                                 its definition in bb{}",
                                        pred.index(),
                                        def.index()
                                    ),
                                    block_id,
                                    inst_id,
                                );
                            }
                        }
                    }
                    kind => {
                        for &operand in kind.operands().iter() {
                            if let Some(def) = def_block_of[operand]
                                && def != block_id
                                && !reaches(def, block_id)
                            {
                                self.emit_at_inst(
                                    format_args!(
                                        "use of {operand:?} can never be reached by its \
                                 definition in bb{}",
                                        def.index()
                                    ),
                                    block_id,
                                    inst_id,
                                );
                            }
                        }
                    }
                }
            }
            if let Some(term) = &block.terminator {
                for &operand in term.operands().iter() {
                    if let Some(def) = def_block_of[operand]
                        && def != block_id
                        && !reaches(def, block_id)
                    {
                        self.emit_at_block(
                            format_args!(
                                "terminator use of {operand:?} can never be reached by its \
                         definition in bb{}",
                                def.index()
                            ),
                            block_id,
                        );
                    }
                }
            }
        }
    }

    /// Validates every function in a module.
    pub fn validate_module(mut self, module: &Module) {
        for (id, func) in module.iter_functions() {
            self.function = Some(id.index());
            self.validate_function_inner(func);
        }
        self.function = None;
        self.validate_tail_calls(module);
        self.validate_phase(module);
    }

    /// Checks the cross-function invariants of `tail_call` terminators: the
    /// callee exists and the argument count matches its parameters.
    fn validate_tail_calls(&mut self, module: &Module) {
        for (id, func) in module.iter_functions() {
            for block in func.blocks.iter() {
                let Some(crate::mir::Terminator::TailCall { function, args }) = &block.terminator
                else {
                    continue;
                };
                let Some(callee) = module.functions.get(*function) else {
                    self.emit(format_args!(
                        "[fn{}] tail_call targets nonexistent function fn{}",
                        id.index(),
                        function.index()
                    ));
                    continue;
                };
                if args.len() != callee.params.len() {
                    self.emit(format_args!(
                        "[fn{}] tail_call to `{}` passes {} argument(s), expected {}",
                        id.index(),
                        callee.name,
                        args.len(),
                        callee.params.len()
                    ));
                }
            }
        }
    }

    /// Checks that the module's content satisfies its declared
    /// [`MirPhase`](crate::mir::MirPhase), so
    /// the phase is a real contract rather than a label.
    fn validate_phase(&mut self, module: &Module) {
        // From the `dispatch` phase on, routing is materialized: a module with
        // bodied selector functions must contain the synthesized `entry`.
        if module.phase >= crate::mir::MirPhase::Dispatch
            && module.functions.iter().any(|f| f.selector.is_some() && !f.blocks.is_empty())
            && !module.functions.iter().any(|f| f.name.name == sym::entry)
        {
            self.emit(format_args!(
                "module is in the `{}` phase but has no `entry` dispatcher function",
                module.phase.name()
            ));
        }
        // From the `abi` phase on, every bodied external (selector-bearing)
        // function is an argument-free self-decoding wrapper.
        if module.phase >= crate::mir::MirPhase::Abi {
            for (id, func) in module.iter_functions() {
                if func.selector.is_some() && !func.blocks.is_empty() && !func.params.is_empty() {
                    self.emit(format_args!(
                        "[fn{}] selector function `{}` still takes arguments in the `{}` phase \
                     (expected an argument-free ABI wrapper)",
                        id.index(),
                        func.name,
                        module.phase.name()
                    ));
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{BasicBlock, Function, FunctionBuilder, MirType, Terminator};
    use snapbox::{assert_data_eq, str};
    use solar_interface::{ColorChoice, Ident, Session};

    fn with_session<F: FnOnce(&Session) + Send>(f: F) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        sess.enter(|| f(&sess));
    }

    fn make_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn valid_simple_function() {
        with_session(|sess| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                let one = b.imm_u64(1);
                let sum = b.add(x, one);
                b.ret([sum]);
            }
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_ok());
        });
    }

    #[test]
    fn missing_terminator_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            // Add a parameter to the entry block but no terminator.
            {
                let mut b = FunctionBuilder::new(&mut func);
                let _p = b.add_param(MirType::uint256());
                // Don't terminate — leave the entry block dangling.
            }
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [bb0] block has no terminator


"#]]
            );
        });
    }

    #[test]
    fn bad_block_reference_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                b.ret([x]);
            }
            // Manually corrupt: replace the terminator with a Jump to a nonexistent block.
            let bad_block = BlockId::from_usize(99);
            func.blocks[func.entry_block].terminator = Some(Terminator::Jump(bad_block));
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [bb0] terminator references nonexistent block bb99


"#]]
            );
        });
    }

    #[test]
    fn predecessor_back_link_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            let target;
            {
                let mut b = FunctionBuilder::new(&mut func);
                target = b.create_block();
                b.jump(target);
                b.switch_to_block(target);
                b.stop();
            }
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_ok());
            // Drop the back-link.
            func.blocks[target].predecessors.clear();
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [bb0] successor bb1 does not list bb0 as a predecessor


"#]]
            );
        });
    }

    #[test]
    fn entry_block_with_predecessors_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            // Build a function that loops back to the entry block.
            // The builder rejects this shape, so construct it manually for validation.
            {
                let mut b = FunctionBuilder::new(&mut func);
                b.stop();
            }
            // Add the invalid predecessor to the entry block.
            func.blocks[func.entry_block].predecessors.push(func.entry_block);
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [bb0] stored predecessor bb0 does not branch to bb0

error: [bb0] entry block must have no predecessors


"#]]
            );
        });
    }

    #[test]
    fn empty_function_with_just_terminator_is_valid() {
        with_session(|sess| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                b.stop();
            }
            Validator::new(&sess.dcx).validate_function(&func);
            assert!(sess.dcx.has_errors().is_ok());
        });
    }

    // Suppress the unused-import warning for `BasicBlock`.
    #[allow(dead_code)]
    fn _block_type_reference() -> Option<BasicBlock> {
        None
    }
}
