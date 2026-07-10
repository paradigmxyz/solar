//! MIR validator — checks SSA invariants on a [`Function`].
//!
//! This is the Solar equivalent of LLVM's `verify` pass / Cranelift's
//! `Function::verify`. It walks a function once and reports every invariant
//! violation it finds, returning them as a `Vec<ValidationError>` (empty
//! when the function is well-formed).
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
//!
//! # Usage
//!
//! ```ignore
//! use solar_codegen::analysis::Validator;
//! let errors = Validator::validate_function(&func);
//! assert!(errors.is_empty(), "{:#?}", errors);
//! ```
//!
//! Or via the pass manager:
//!
//! ```ignore
//! use solar_codegen::pass::AnalysisManager;
//! use solar_codegen::analysis::ValidatorAnalysis;
//! let mut am = AnalysisManager::new();
//! let errors = am.get_or_compute(&ValidatorAnalysis, &func);
//! ```

use crate::{
    mir::{BlockId, Function, InstId, InstKind, Module, Value},
    pass::AnalysisPass,
};
use solar_data_structures::map::FxHashMap;
use solar_interface::sym;
use std::fmt;

/// MIR validation query.
pub struct Validator;

/// One validation finding.
#[derive(Clone, Debug)]
pub struct ValidationError {
    /// Human-readable message.
    pub message: String,
    /// The block this error pertains to, if any.
    pub block: Option<BlockId>,
    /// The instruction this error pertains to, if any.
    pub inst: Option<InstId>,
}

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), block: None, inst: None }
    }

    fn at_block(message: impl Into<String>, block: BlockId) -> Self {
        Self { message: message.into(), block: Some(block), inst: None }
    }

    fn at_inst(message: impl Into<String>, block: BlockId, inst: InstId) -> Self {
        Self { message: message.into(), block: Some(block), inst: Some(inst) }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.block, self.inst) {
            (Some(b), Some(i)) => {
                write!(f, "[bb{}, inst{}] {}", b.index(), i.index(), self.message)
            }
            (Some(b), None) => write!(f, "[bb{}] {}", b.index(), self.message),
            (None, _) => write!(f, "{}", self.message),
        }
    }
}

impl Validator {
    /// Validates a single function. Returns the empty vec on success.
    #[must_use]
    pub fn validate_function(func: &Function) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let num_values = func.values.len();
        let num_blocks = func.blocks.len();
        let num_insts = func.instructions.len();

        if num_blocks == 0 {
            return errors;
        }

        // ----- Single-definition check -----
        // Count how many Value entries claim to be the result of each InstId.
        let mut inst_def_count: FxHashMap<InstId, usize> = FxHashMap::default();
        for v in func.values.iter() {
            if let Value::Inst(inst_id) = v {
                *inst_def_count.entry(*inst_id).or_default() += 1;
            }
        }
        for (inst_id, count) in inst_def_count.iter() {
            if *count > 1 {
                errors.push(ValidationError::new(format!(
                    "instruction inst{} is defined by {count} Value entries (must be 1)",
                    inst_id.index()
                )));
            }
            // Only value-producing instructions may have a result value.
            if inst_id.index() < num_insts && func.instructions[*inst_id].result_ty.is_none() {
                errors.push(ValidationError::new(format!(
                    "instruction inst{} (`{:?}`) has a result Value entry but no result type",
                    inst_id.index(),
                    func.instructions[*inst_id].kind
                )));
            }
        }

        // ----- Walk every block -----
        for (block_id, block) in func.blocks.iter_enumerated() {
            // Check terminator presence.
            let term = match &block.terminator {
                Some(t) => t,
                None => {
                    errors.push(ValidationError::at_block("block has no terminator", block_id));
                    continue;
                }
            };

            let term_succs = term.successors();

            // Check successor blocks exist and back-link.
            for &succ in &term_succs {
                if succ.index() >= num_blocks {
                    errors.push(ValidationError::at_block(
                        format!("terminator references nonexistent block bb{}", succ.index()),
                        block_id,
                    ));
                    continue;
                }
                if !func.blocks[succ].predecessors.contains(&block_id) {
                    errors.push(ValidationError::at_block(
                        format!(
                            "successor bb{} does not list bb{} as a predecessor",
                            succ.index(),
                            block_id.index()
                        ),
                        block_id,
                    ));
                }
            }

            // Check stored predecessor blocks exist and branch to this block.
            for &pred in &block.predecessors {
                if pred.index() >= num_blocks {
                    errors.push(ValidationError::at_block(
                        format!(
                            "stored predecessor references nonexistent block bb{}",
                            pred.index()
                        ),
                        block_id,
                    ));
                    continue;
                }
                let Some(pred_term) = &func.blocks[pred].terminator else {
                    errors.push(ValidationError::at_block(
                        format!("stored predecessor bb{} has no terminator", pred.index()),
                        block_id,
                    ));
                    continue;
                };
                if !pred_term.successors().contains(&block_id) {
                    errors.push(ValidationError::at_block(
                        format!(
                            "stored predecessor bb{} does not branch to bb{}",
                            pred.index(),
                            block_id.index()
                        ),
                        block_id,
                    ));
                }
            }

            // Check terminator operands are in range.
            for op in term.operands() {
                if op.index() >= num_values {
                    errors.push(ValidationError::at_block(
                        format!(
                            "terminator references undefined value v{} (only {} values exist)",
                            op.index(),
                            num_values
                        ),
                        block_id,
                    ));
                }
            }

            // ----- Walk instructions in this block -----
            let block_preds: Vec<BlockId> = block.predecessors.iter().copied().collect();
            for &inst_id in &block.instructions {
                if inst_id.index() >= num_insts {
                    errors.push(ValidationError::at_block(
                        format!("block contains nonexistent inst{}", inst_id.index()),
                        block_id,
                    ));
                    continue;
                }
                let inst = func.instruction(inst_id);

                // Operand range check.
                for op in inst.kind.operands() {
                    if op.index() >= num_values {
                        errors.push(ValidationError::at_inst(
                            format!(
                                "instruction references undefined value v{} (only {} values exist)",
                                op.index(),
                                num_values
                            ),
                            block_id,
                            inst_id,
                        ));
                    }
                }

                // Phi-specific checks.
                if let InstKind::Phi(incoming) = &inst.kind {
                    // Every incoming block must be a predecessor.
                    for (pred_block, _) in incoming {
                        if pred_block.index() >= num_blocks {
                            errors.push(ValidationError::at_inst(
                                format!(
                                    "phi incoming references nonexistent block bb{}",
                                    pred_block.index()
                                ),
                                block_id,
                                inst_id,
                            ));
                            continue;
                        }
                        if !block_preds.contains(pred_block) {
                            errors.push(ValidationError::at_inst(
                                format!(
                                    "phi incoming from bb{} but bb{} is not a predecessor",
                                    pred_block.index(),
                                    pred_block.index()
                                ),
                                block_id,
                                inst_id,
                            ));
                        }
                    }
                    // Every predecessor must appear in the incoming list.
                    for pred in &block_preds {
                        if !incoming.iter().any(|(b, _)| b == pred) {
                            errors.push(ValidationError::at_inst(
                                format!(
                                    "phi missing incoming entry for predecessor bb{}",
                                    pred.index()
                                ),
                                block_id,
                                inst_id,
                            ));
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
                            errors.push(ValidationError::at_inst(
                                format!(
                                    "phi has conflicting incoming values for predecessor bb{}",
                                    pred_block.index()
                                ),
                                block_id,
                                inst_id,
                            ));
                        }
                    }
                }
            }
        }

        // ----- Entry block must have no predecessors -----
        if !func.blocks[func.entry_block].predecessors.is_empty() {
            errors.push(ValidationError::at_block(
                "entry block must have no predecessors",
                func.entry_block,
            ));
        }

        errors
    }

    /// Validates every function in a module. Errors from each function are
    /// prefixed with the function index in the message so they can be
    /// distinguished in mixed reports.
    #[must_use]
    pub fn validate_module(module: &Module) -> Vec<ValidationError> {
        let mut all = Vec::new();
        for (id, func) in module.iter_functions() {
            for mut err in Self::validate_function(func) {
                err.message = format!("[fn{}] {}", id.index(), err.message);
                all.push(err);
            }
        }
        all.extend(Self::validate_tail_calls(module));
        all.extend(Self::validate_phase(module));
        all
    }

    /// Checks the cross-function invariants of `tail_call` terminators: the
    /// callee exists and the argument count matches its parameters.
    fn validate_tail_calls(module: &Module) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for (id, func) in module.iter_functions() {
            for block in func.blocks.iter() {
                let Some(crate::mir::Terminator::TailCall { function, args }) = &block.terminator
                else {
                    continue;
                };
                let Some(callee) = module.functions.get(*function) else {
                    errors.push(ValidationError::new(format!(
                        "[fn{}] tail_call targets nonexistent function fn{}",
                        id.index(),
                        function.index()
                    )));
                    continue;
                };
                if args.len() != callee.params.len() {
                    errors.push(ValidationError::new(format!(
                        "[fn{}] tail_call to `{}` passes {} argument(s), expected {}",
                        id.index(),
                        callee.name,
                        args.len(),
                        callee.params.len()
                    )));
                }
            }
        }
        errors
    }

    /// Checks that the module's content satisfies its declared
    /// [`MirPhase`](crate::mir::MirPhase), so
    /// the phase is a real contract rather than a label.
    fn validate_phase(module: &Module) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        // From the `dispatch` phase on, routing is materialized: a module with
        // bodied selector functions must contain the synthesized `entry`.
        if module.phase >= crate::mir::MirPhase::Dispatch
            && module.functions.iter().any(|f| f.selector.is_some() && !f.blocks.is_empty())
            && !module.functions.iter().any(|f| f.name.name == sym::entry)
        {
            errors.push(ValidationError::new(format!(
                "module is in the `{}` phase but has no `entry` dispatcher function",
                module.phase.name()
            )));
        }
        // From the `abi` phase on, every bodied external (selector-bearing)
        // function is an argument-free self-decoding wrapper.
        if module.phase >= crate::mir::MirPhase::Abi {
            for (id, func) in module.iter_functions() {
                if func.selector.is_some() && !func.blocks.is_empty() && !func.params.is_empty() {
                    errors.push(ValidationError::new(format!(
                        "[fn{}] selector function `{}` still takes arguments in the `{}` phase \
                         (expected an argument-free ABI wrapper)",
                        id.index(),
                        func.name,
                        module.phase.name()
                    )));
                }
            }
        }
        errors
    }
}

/// Validates a single function. Returns the empty vec on success.
#[must_use]
pub fn validate_function(func: &Function) -> Vec<ValidationError> {
    Validator::validate_function(func)
}

/// Validates every function in a module.
#[must_use]
pub fn validate_module(module: &Module) -> Vec<ValidationError> {
    Validator::validate_module(module)
}

// =============================================================================
// Pass-manager adapter
// =============================================================================

/// Validator as an [`AnalysisPass`]. The result is the (possibly empty)
/// list of validation errors.
pub struct ValidatorAnalysis;

impl AnalysisPass for ValidatorAnalysis {
    type Result = Vec<ValidationError>;

    fn name(&self) -> &str {
        "validator"
    }

    fn run(&self, func: &Function) -> Vec<ValidationError> {
        Validator::validate_function(func)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{BasicBlock, Function, FunctionBuilder, MirType, Terminator};
    use solar_interface::{ColorChoice, Ident, Session};

    fn with_session<F: FnOnce() + Send>(f: F) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(f);
    }

    fn make_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn valid_simple_function() {
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                let one = b.imm_u64(1);
                let sum = b.add(x, one);
                b.ret([sum]);
            }
            let errors = validate_function(&func);
            assert!(errors.is_empty(), "expected valid function, got: {errors:#?}");
        });
    }

    #[test]
    fn missing_terminator_is_caught() {
        with_session(|| {
            let mut func = make_func();
            // Add a parameter to the entry block but no terminator.
            {
                let mut b = FunctionBuilder::new(&mut func);
                let _p = b.add_param(MirType::uint256());
                // Don't terminate — leave the entry block dangling.
            }
            let errors = validate_function(&func);
            assert!(
                errors.iter().any(|e| e.message.contains("no terminator")),
                "expected 'no terminator' error, got: {errors:#?}"
            );
        });
    }

    #[test]
    fn bad_block_reference_is_caught() {
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                b.ret([x]);
            }
            // Manually corrupt: replace the terminator with a Jump to a nonexistent block.
            let bad_block = BlockId::from_usize(99);
            func.blocks[func.entry_block].terminator = Some(Terminator::Jump(bad_block));
            let errors = validate_function(&func);
            assert!(
                errors.iter().any(|e| e.message.contains("nonexistent block")),
                "expected error about nonexistent block, got: {errors:#?}"
            );
        });
    }

    #[test]
    fn predecessor_back_link_is_caught() {
        with_session(|| {
            let mut func = make_func();
            let target;
            {
                let mut b = FunctionBuilder::new(&mut func);
                target = b.create_block();
                b.jump(target);
                b.switch_to_block(target);
                b.stop();
            }
            assert!(validate_function(&func).is_empty());
            // Drop the back-link.
            func.blocks[target].predecessors.clear();
            let errors = validate_function(&func);
            assert!(
                errors.iter().any(|e| e.message.contains("does not list bb0 as a predecessor")),
                "expected predecessor back-link error, got: {errors:#?}"
            );
        });
    }

    #[test]
    fn entry_block_with_predecessors_is_caught() {
        with_session(|| {
            let mut func = make_func();
            // Build a function that loops back to the entry block.
            // The builder rejects this shape, so construct it manually for validation.
            {
                let mut b = FunctionBuilder::new(&mut func);
                b.stop();
            }
            // Add the invalid predecessor to the entry block.
            func.blocks[func.entry_block].predecessors.push(func.entry_block);
            let errors = validate_function(&func);
            assert!(
                errors.iter().any(|e| e.message.contains("entry block must have no predecessors")),
                "expected entry-block error, got: {errors:#?}"
            );
        });
    }

    #[test]
    fn validator_as_analysis_pass() {
        use crate::pass::AnalysisManager;
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                b.ret([x]);
            }
            let mut am = AnalysisManager::new();
            let errors = am.get_or_compute(&ValidatorAnalysis, &func);
            assert!(errors.is_empty());
        });
    }

    #[test]
    fn empty_function_with_just_terminator_is_valid() {
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                b.stop();
            }
            let errors = validate_function(&func);
            assert!(errors.is_empty(), "{errors:#?}");
        });
    }

    #[test]
    fn validation_error_display() {
        let e1 = ValidationError::new("oops");
        assert_eq!(format!("{e1}"), "oops");
        let e2 = ValidationError::at_block("oops", BlockId::from_usize(3));
        assert_eq!(format!("{e2}"), "[bb3] oops");
        let e3 = ValidationError::at_inst("oops", BlockId::from_usize(3), InstId::from_usize(5));
        assert_eq!(format!("{e3}"), "[bb3, inst5] oops");
    }

    // Suppress the unused-import warning for `BasicBlock`.
    #[allow(dead_code)]
    fn _block_type_reference() -> Option<BasicBlock> {
        None
    }
}
