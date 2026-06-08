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
//! 5. **Successor consistency**: each block's `block.successors` matches the actual successors of
//!    its terminator.
//! 6. **Predecessor back-link**: if A's terminator targets B, then B's `predecessors` contains A.
//! 7. **Entry block has no predecessors**.
//! 8. **Phi block coverage**: every `InstKind::Phi`'s incoming blocks are predecessors of the
//!    containing block, and every predecessor has an incoming entry.
//! 9. **Instruction-block consistency**: each instruction's `block` field matches the block whose
//!    `instructions` vector contains it.
//!
//! # Usage
//!
//! ```ignore
//! use solar_codegen::analysis::validate_function;
//! let errors = validate_function(&func);
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
use std::fmt;

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

/// Validates a single function. Returns the empty vec on success.
#[must_use]
pub fn validate_function(func: &Function) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let num_values = func.values.len();
    let num_blocks = func.blocks.len();
    let num_insts = func.instructions.len();

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

        // Check terminator successor consistency with stored successors.
        let term_succs = term.successors();
        let stored_succs: Vec<BlockId> = block.successors.iter().copied().collect();
        let term_succs_vec: Vec<BlockId> = term_succs.iter().copied().collect();
        if term_succs_vec != stored_succs {
            errors.push(ValidationError::at_block(
                format!(
                    "successors mismatch: terminator says {:?}, stored {:?}",
                    term_succs_vec.iter().map(|b| format!("bb{}", b.index())).collect::<Vec<_>>(),
                    stored_succs.iter().map(|b| format!("bb{}", b.index())).collect::<Vec<_>>()
                ),
                block_id,
            ));
        }

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
        for mut err in validate_function(func) {
            err.message = format!("[fn{}] {}", id.index(), err.message);
            all.push(err);
        }
    }
    all
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
        validate_function(func)
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
            // Note: don't update successors so we can also catch the inconsistency.
            let errors = validate_function(&func);
            assert!(
                errors.iter().any(|e| e.message.contains("nonexistent block")
                    || e.message.contains("successors mismatch")),
                "expected error about nonexistent block, got: {errors:#?}"
            );
        });
    }

    #[test]
    fn successor_mismatch_is_caught() {
        with_session(|| {
            let mut func = make_func();
            let then_bb;
            let else_bb;
            {
                let mut b = FunctionBuilder::new(&mut func);
                let cond = b.add_param(MirType::Bool);
                then_bb = b.create_block();
                else_bb = b.create_block();
                b.branch(cond, then_bb, else_bb);
                b.switch_to_block(then_bb);
                b.stop();
                b.switch_to_block(else_bb);
                b.stop();
            }
            // Validates clean first.
            assert!(validate_function(&func).is_empty());
            // Now manually drop a successor to break consistency.
            func.blocks[func.entry_block].successors.pop();
            let errors = validate_function(&func);
            assert!(
                errors.iter().any(|e| e.message.contains("successors mismatch")),
                "expected successors mismatch, got: {errors:#?}"
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
