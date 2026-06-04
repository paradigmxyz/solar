//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - Two-pass assembly for label resolution

use super::{
    assembler::{Assembler, Label, opcodes},
    stack::{ScheduledOp, SpillSlot, StackOp, StackScheduler},
};
use crate::{
    IMMUTABLE_SCRATCH_BASE,
    analysis::{CopyDest, CopySource, Liveness, ParallelCopy, eliminate_phis},
    mir::{BlockId, Function, FunctionId, InstKind, MirType, Module, Terminator, ValueId},
    pass::{
        AnalysisManager, CfgSimplifyPass, CsePass, DcePass, JumpThreadingPass, LivenessAnalysis,
        MemoryDsePass, PassManager, SccpTransformPass,
    },
};
use alloy_primitives::U256;
use rustc_hash::{FxHashMap, FxHashSet};

const INTERNAL_FRAME_PTR_SLOT: u64 = 0x2000;
const LOW_MEMORY_START: u64 = 0x80;
const CONSTRUCTOR_FREE_MEMORY_START: u64 = 0x4000;
const LINEAR_SELECTOR_DISPATCH_THRESHOLD: usize = 4;

/// Describes the stack effect of an EVM instruction.
/// This is used to keep the scheduler's stack model in sync with the actual EVM stack.
#[derive(Clone, Copy, Debug)]
struct StackEffect {
    /// Number of values popped from the stack.
    pops: usize,
    /// Number of values pushed to the stack.
    pushes: usize,
}

/// What value to track for a pushed stack entry.
#[derive(Clone, Copy, Debug)]
enum StackPush {
    /// No value is pushed (pushes == 0).
    #[allow(dead_code)]
    None,
    /// Push a tracked ValueId (pushes == 1).
    Tracked(ValueId),
    /// Push an unknown/untracked value (pushes == 1).
    Unknown,
}

#[derive(Clone, Copy, Debug)]
struct SelectorDispatchEntry {
    selector: u32,
    label: Label,
}

/// EVM code generator.
pub struct EvmCodegen {
    /// The assembler for bytecode generation.
    asm: Assembler,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Function labels for direct internal calls.
    function_labels: FxHashMap<FunctionId, Label>,
    /// Per-function local frame sizes for direct internal calls.
    function_frame_sizes: FxHashMap<FunctionId, u64>,
    /// Copies to insert at block exits (from phi elimination).
    block_copies: FxHashMap<BlockId, Vec<ParallelCopy>>,
    /// Whether we're currently generating constructor code.
    /// When true, LoadArg uses CODECOPY from the end of code instead of CALLDATALOAD.
    in_constructor: bool,
    /// Number of constructor parameters (used for CODECOPY offset calculation).
    constructor_param_count: u32,
    /// Whether we're emitting an internal function body.
    in_internal_function: bool,
}

impl EvmCodegen {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            asm: Assembler::new(),
            scheduler: StackScheduler::new(),
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            function_frame_sizes: FxHashMap::default(),
            block_copies: FxHashMap::default(),
            in_constructor: false,
            constructor_param_count: 0,
            in_internal_function: false,
        }
    }

    // ==================== Stack-Aware Emitter API ====================
    //
    // These helpers ensure that all EVM stack mutations are tracked by the scheduler.
    // Any opcode that changes the EVM stack must be emitted through these methods
    // to keep the scheduler's StackModel in sync with the actual EVM stack.

    /// Emits a stack manipulation operation (DUP, SWAP, POP) and updates the scheduler.
    fn emit_stack_op(&mut self, op: StackOp) {
        self.asm.emit_op(op.opcode());
        match op {
            StackOp::Dup(n) => self.scheduler.stack.dup(n),
            StackOp::Swap(n) => self.scheduler.stack.swap(n),
            StackOp::Pop => {
                self.scheduler.stack.pop();
            }
        }
    }

    /// Emits an opcode with known stack effects and updates the scheduler.
    ///
    /// This is the core method for stack-aware emission. After emitting the opcode:
    /// - `effect.pops` values are removed from the scheduler's stack model
    /// - Values are pushed according to `push`:
    ///   - `StackPush::None`: no value pushed (effect.pushes must be 0)
    ///   - `StackPush::Tracked(v)`: push a tracked ValueId (effect.pushes must be 1)
    ///   - `StackPush::Unknown`: push an untracked value (effect.pushes must be 1)
    fn emit_op_with_effect(&mut self, opcode: u8, effect: StackEffect, push: StackPush) {
        #[cfg(debug_assertions)]
        let before = self.scheduler.depth();

        self.asm.emit_op(opcode);

        // Pop consumed values
        for _ in 0..effect.pops {
            self.scheduler.stack.pop();
        }

        // Push produced values
        match (effect.pushes, push) {
            (0, StackPush::None) => {}
            (1, StackPush::Tracked(v)) => self.scheduler.stack.push(v),
            (1, StackPush::Unknown) => self.scheduler.stack.push_unknown(),
            (n, _) if n > 1 => {
                // Multi-push: push unknown values
                for _ in 0..n {
                    self.scheduler.stack.push_unknown();
                }
            }
            _ => {}
        }

        #[cfg(debug_assertions)]
        {
            let expected = before + effect.pushes - effect.pops;
            debug_assert_eq!(
                self.scheduler.depth(),
                expected,
                "Stack model drift after opcode 0x{:02x}: expected depth {}, got {}",
                opcode,
                expected,
                self.scheduler.depth()
            );
        }
    }

    /// Generates bytecode for a module (runtime code only).
    /// Returns empty bytecode for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen.
    pub fn generate_module(&mut self, module: &mut Module) -> Vec<u8> {
        if module.is_interface {
            return Vec::new();
        }
        self.run_optimization_passes(module);
        let mut runtime = self.generate_runtime_code(module);
        runtime.resize(runtime.len() + module.immutable_data_len(), 0);
        runtime
    }

    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    /// Returns empty bytecodes for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen.
    pub fn generate_deployment_bytecode(&mut self, module: &mut Module) -> (Vec<u8>, Vec<u8>) {
        if module.is_interface {
            return (Vec::new(), Vec::new());
        }
        self.run_optimization_passes(module);
        // First generate the runtime code
        let runtime_code = self.generate_runtime_code(module);
        let runtime_len = runtime_code.len();
        let immutable_len = module.immutable_data_len();

        // Generate constructor initialization code (if any). Constructor arguments are appended
        // after the generated deployment bytecode, so the constructor arg offset depends on the
        // constructor code length. Iterate until the push widths stabilize.
        let mut deploy_code_len = 0usize;
        let mut constructor_arg_offset = runtime_len;
        let mut constructor_code = self.generate_constructor_code(module, Some(runtime_len));
        for _ in 0..8 {
            let postlude = Self::build_deployment_postlude(
                deploy_code_len,
                runtime_len,
                immutable_len,
                &module.immutables,
            );
            let next_deploy_code_len = constructor_code.len() + postlude.len();
            let next_arg_offset = next_deploy_code_len + runtime_len;
            if next_deploy_code_len == deploy_code_len && next_arg_offset == constructor_arg_offset
            {
                break;
            }
            deploy_code_len = next_deploy_code_len;
            constructor_arg_offset = next_arg_offset;
            constructor_code = self.generate_constructor_code(module, Some(constructor_arg_offset));
        }

        // Deploy code structure:
        // [constructor_code]    ; run constructor (SSTOREs + immutable staging)
        // PUSH<n> runtime_len   ; size to copy from creation code
        // DUP1                  ; duplicate for CODECOPY
        // PUSH<n> offset        ; where runtime starts
        // PUSH0                 ; memory destination = 0
        // CODECOPY              ; copy runtime to memory
        // [immutable copies]    ; copy staged immutable words after runtime
        // PUSH<n> runtime+immutables len
        // PUSH0                 ; memory offset = 0
        // RETURN                ; return the runtime code
        let postlude = Self::build_deployment_postlude(
            deploy_code_len,
            runtime_len,
            immutable_len,
            &module.immutables,
        );

        // Build the deployment bytecode
        let mut deploy_bytecode = Vec::new();

        // Add constructor code first
        deploy_bytecode.extend_from_slice(&constructor_code);
        deploy_bytecode.extend_from_slice(&postlude);

        // Append runtime code
        deploy_bytecode.extend_from_slice(&runtime_code);

        let mut deployed_runtime = runtime_code;
        deployed_runtime.resize(runtime_len + immutable_len, 0);

        (deploy_bytecode, deployed_runtime)
    }

    fn build_deployment_postlude(
        deploy_code_len: usize,
        runtime_len: usize,
        immutable_len: usize,
        immutables: &[crate::mir::ImmutableSlot],
    ) -> Vec<u8> {
        let mut bytecode = Vec::new();

        // Copy runtime code from creation code to memory offset 0.
        Self::emit_push_raw(&mut bytecode, runtime_len as u64);
        bytecode.push(opcodes::dup(1));
        Self::emit_push_raw(&mut bytecode, deploy_code_len as u64);
        bytecode.push(opcodes::PUSH0);
        bytecode.push(opcodes::CODECOPY);

        // Append constructor-staged immutable words after the runtime code.
        for slot in immutables {
            Self::emit_push_raw(&mut bytecode, IMMUTABLE_SCRATCH_BASE + slot.offset);
            bytecode.push(opcodes::MLOAD);
            Self::emit_push_raw(&mut bytecode, runtime_len as u64 + slot.offset);
            bytecode.push(opcodes::MSTORE);
        }

        Self::emit_push_raw(&mut bytecode, (runtime_len + immutable_len) as u64);
        bytecode.push(opcodes::PUSH0);
        bytecode.push(opcodes::RETURN);
        bytecode
    }

    /// Generates constructor code that runs during deployment.
    /// This includes state variable initializers.
    ///
    /// Constructor arguments are read from the end of the initcode using CODECOPY.
    /// The args are ABI-encoded and appended after the deployment bytecode.
    fn generate_constructor_code(
        &mut self,
        module: &Module,
        constructor_arg_offset: Option<usize>,
    ) -> Vec<u8> {
        // Find constructor function if it exists
        let constructor = module.functions.iter().find(|f| f.attributes.is_constructor);

        if let Some(ctor) = constructor {
            // Generate constructor bytecode
            let mut asm = Assembler::new();
            std::mem::swap(&mut self.asm, &mut asm);

            // Clear state and generate function body
            self.block_labels.clear();
            self.block_copies.clear();

            // Constructors stage immutable words in reserved memory, so keep their heap high.
            self.asm.emit_push(U256::from(CONSTRUCTOR_FREE_MEMORY_START));
            self.asm.emit_push(U256::from(0x40));
            self.asm.emit_op(opcodes::MSTORE);

            // Set constructor context for LoadArg handling
            self.in_constructor = true;
            self.constructor_param_count = ctor.params.len() as u32;

            // If constructor has parameters, copy the full ABI-encoded argument blob to memory.
            // Constructor args are appended after generated deployment bytecode, so the copy size
            // is `CODESIZE - constructor_arg_offset`.
            if !ctor.params.is_empty() {
                let arg_offset = constructor_arg_offset.unwrap_or(0);
                self.asm.emit_push(U256::from(arg_offset));
                self.asm.emit_op(opcodes::CODESIZE);
                self.asm.emit_op(opcodes::SUB); // size = CODESIZE - arg_offset
                self.asm.emit_push(U256::from(arg_offset)); // code offset
                self.asm.emit_push(U256::from(0x80)); // destOffset in memory
                self.asm.emit_op(opcodes::CODECOPY);
            }

            // Generate the constructor body (which includes SSTORE for initializers)
            self.generate_function_body(ctor);

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_param_count = 0;

            std::mem::swap(&mut self.asm, &mut asm);
            let mut bytecode = asm.assemble().bytecode;

            // Remove trailing STOP (0x00) if present - we want to fall through to CODECOPY/RETURN
            if bytecode.last() == Some(&opcodes::STOP) {
                bytecode.pop();
            }

            bytecode
        } else {
            Vec::new()
        }
    }

    /// Emit a PUSH instruction with the optimal size for the value.
    fn emit_push_raw(bytecode: &mut Vec<u8>, value: u64) {
        // PUSH0 = 0x5f, PUSH1 = 0x60, PUSH2 = 0x61, etc.
        if value == 0 {
            bytecode.push(0x5f); // PUSH0
        } else if value <= 0xFF {
            bytecode.push(0x60); // PUSH1
            bytecode.push(value as u8);
        } else if value <= 0xFFFF {
            bytecode.push(0x61); // PUSH2
            bytecode.push((value >> 8) as u8);
            bytecode.push(value as u8);
        } else if value <= 0xFFFFFF {
            bytecode.push(0x62); // PUSH3
            bytecode.push((value >> 16) as u8);
            bytecode.push((value >> 8) as u8);
            bytecode.push(value as u8);
        } else {
            // For larger values, use the minimum bytes needed
            let bytes = value.to_be_bytes();
            let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap_or(7);
            let num_bytes = 8 - first_non_zero;
            bytecode.push(0x5f + num_bytes as u8); // PUSH1 = 0x60, PUSH2 = 0x61, etc.
            bytecode.extend_from_slice(&bytes[first_non_zero..]);
        }
    }

    /// Runs optimization passes on all functions in the module via the PassManager.
    ///
    /// Order:
    /// 1. SCCP          — folds constants and prunes constant branches
    /// 2. CSE           — removes duplicate local computations
    /// 3. Jump threading — rewrites jump targets through forwarder blocks (8 gas/jump)
    /// 4. CFG simplify  — physically merges sequential blocks and removes empty forwarders
    /// 5. Memory DSE    — removes overwritten same-block memory stores
    /// 6. DCE           — removes dead instructions and any remaining unreachable blocks
    ///
    /// Each pass internally iterates to a fixed point. Threading creates orphaned
    /// forwarder blocks; cfg-simplify cleans them up by merging or eliminating them;
    /// DCE handles whatever's still unreachable.
    fn run_optimization_passes(&mut self, module: &mut Module) {
        let mut pm = PassManager::new();
        pm.add_transform(Box::new(SccpTransformPass));
        pm.add_transform(Box::new(CsePass));
        pm.add_transform(Box::new(JumpThreadingPass));
        pm.add_transform(Box::new(CfgSimplifyPass));
        pm.add_transform(Box::new(MemoryDsePass));
        pm.add_transform(Box::new(DcePass));
        for func in module.functions.iter_mut() {
            pm.run(func);
        }
    }

    /// Generates runtime bytecode for a module.
    fn generate_runtime_code(&mut self, module: &Module) -> Vec<u8> {
        self.asm = Assembler::new();
        self.block_labels.clear();
        self.function_labels.clear();
        self.function_frame_sizes.clear();
        self.block_copies.clear();

        // Dispatcher itself does not allocate. External entries set this precisely
        // after dispatch based on their local/spill footprint.
        self.asm.emit_push(U256::from(LOW_MEMORY_START));
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(opcodes::MSTORE);

        if !module.functions.is_empty() {
            // The dispatcher generates function bodies inline
            self.generate_dispatcher(module);
        }

        let result = std::mem::take(&mut self.asm).assemble();
        result.bytecode
    }

    /// Generates the function dispatcher.
    ///
    /// The dispatcher logic is:
    /// ```text
    /// if calldatasize == 0:
    ///     if has_receive: jump to receive
    ///     elif has_fallback: jump to fallback
    ///     else: revert
    /// else:
    ///     match selector...
    ///     if no match and has_fallback: jump to fallback
    ///     else: revert
    /// ```
    fn generate_dispatcher(&mut self, module: &Module) {
        // Find receive and fallback functions
        let receive_idx = module.functions.iter().position(|f| f.attributes.is_receive);
        let fallback_idx = module.functions.iter().position(|f| f.attributes.is_fallback);

        let mut internal_targets = FxHashSet::default();
        for func in module.functions.iter() {
            for inst in func.instructions.iter() {
                if let InstKind::InternalCall { function, .. } = &inst.kind {
                    internal_targets.insert(*function);
                }
            }
        }

        for (func_id, func) in module.functions.iter_enumerated() {
            self.function_frame_sizes
                .insert(func_id, func.internal_frame_size + Self::spill_frame_size(func));
        }

        // Create labels for externally reachable runtime entry points and internal-call targets.
        let mut func_labels: Vec<Option<Label>> = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            let external = func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback;
            let needs_body = !func.attributes.is_constructor
                && (external || internal_targets.contains(&func_id));
            let label = needs_body.then(|| self.asm.new_label());
            if let Some(label) = label {
                self.function_labels.insert(func_id, label);
            }
            func_labels.push(label);
        }
        let revert_label = self.asm.new_label();
        let has_calldata_label = self.asm.new_label();

        // Check if calldatasize == 0
        self.asm.emit_op(opcodes::CALLDATASIZE);
        self.asm.emit_push_label(has_calldata_label);
        self.asm.emit_op(opcodes::JUMPI);

        // calldatasize == 0: Handle receive/fallback
        // Solidity semantics: if receive exists, call it; else if fallback exists, call it; else
        // revert
        if let Some(recv_idx) = receive_idx {
            self.asm.emit_push_label(func_labels[recv_idx].expect("receive label missing"));
            self.asm.emit_op(opcodes::JUMP);
        } else if let Some(fb_idx) = fallback_idx {
            self.asm.emit_push_label(func_labels[fb_idx].expect("fallback label missing"));
            self.asm.emit_op(opcodes::JUMP);
        } else {
            self.asm.emit_push_label(revert_label);
            self.asm.emit_op(opcodes::JUMP);
        }

        // calldatasize > 0: Load selector and match
        self.asm.define_label(has_calldata_label);
        self.asm.emit_op(opcodes::JUMPDEST);

        // Load selector from calldata
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(opcodes::CALLDATALOAD);
        self.asm.emit_push(U256::from(0xe0));
        self.asm.emit_op(opcodes::SHR);

        let mut selectors: Vec<_> = module
            .functions
            .iter()
            .enumerate()
            .filter_map(|(i, func)| {
                let selector = func.selector?;
                Some(SelectorDispatchEntry {
                    selector: u32::from_be_bytes(selector),
                    label: func_labels[i].expect("selector label missing"),
                })
            })
            .collect();
        selectors.sort_by_key(|entry| entry.selector);

        let fallback_label =
            fallback_idx.map(|idx| func_labels[idx].expect("fallback label missing"));
        self.emit_selector_dispatch(&selectors, fallback_label, revert_label);

        // Define external function entry points.
        for (i, func) in module.functions.iter().enumerate() {
            let external = func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback;
            if !external {
                continue;
            }
            let Some(label) = func_labels[i] else { continue };
            self.asm.define_label(label);
            self.asm.emit_op(opcodes::JUMPDEST);

            // Pop the selector for regular functions (receive/fallback don't have it on stack)
            if func.selector.is_some() {
                self.asm.emit_op(opcodes::POP);
            }

            // Emit payable check for non-payable functions
            self.emit_payable_check(func);

            self.emit_external_free_memory_start(func);

            // Generate function body
            self.in_internal_function = false;
            self.generate_function_body(func);
        }

        // Define internal-call targets once. Calls jump here and return through the frame's
        // saved return address.
        for (i, func) in module.functions.iter().enumerate() {
            let external = func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback;
            if external {
                continue;
            }
            let Some(label) = func_labels[i] else { continue };
            self.asm.define_label(label);
            self.asm.emit_op(opcodes::JUMPDEST);
            self.in_internal_function = true;
            self.generate_function_body(func);
            self.in_internal_function = false;
        }

        // Revert label
        self.asm.define_label(revert_label);
        self.asm.emit_op(opcodes::JUMPDEST);
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(opcodes::REVERT);
    }

    fn emit_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        if selectors.len() <= LINEAR_SELECTOR_DISPATCH_THRESHOLD {
            self.emit_linear_selector_dispatch(selectors, fallback_label, revert_label);
        } else {
            self.emit_binary_selector_dispatch(selectors, fallback_label, revert_label);
        }
    }

    fn emit_linear_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        for entry in selectors {
            self.emit_selector_eq_jump(*entry);
        }
        self.emit_selector_dispatch_miss(fallback_label, revert_label);
    }

    fn emit_binary_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        if selectors.len() <= LINEAR_SELECTOR_DISPATCH_THRESHOLD {
            self.emit_linear_selector_dispatch(selectors, fallback_label, revert_label);
            return;
        }

        let mid = selectors.len() / 2;
        let left_label = self.asm.new_label();

        // Stack has the selector. With the pivot pushed on top, GT checks
        // `pivot > selector`, so jump left when selector < pivot.
        self.asm.emit_op(opcodes::dup(1));
        self.asm.emit_push(U256::from(selectors[mid].selector));
        self.asm.emit_op(opcodes::GT);
        self.asm.emit_push_label(left_label);
        self.asm.emit_op(opcodes::JUMPI);

        self.emit_binary_selector_dispatch(&selectors[mid..], fallback_label, revert_label);

        self.asm.define_label(left_label);
        self.asm.emit_op(opcodes::JUMPDEST);
        self.emit_binary_selector_dispatch(&selectors[..mid], fallback_label, revert_label);
    }

    fn emit_selector_eq_jump(&mut self, entry: SelectorDispatchEntry) {
        self.asm.emit_op(opcodes::dup(1));
        self.asm.emit_push(U256::from(entry.selector));
        self.asm.emit_op(opcodes::EQ);
        self.asm.emit_push_label(entry.label);
        self.asm.emit_op(opcodes::JUMPI);
    }

    fn emit_selector_dispatch_miss(&mut self, fallback_label: Option<Label>, revert_label: Label) {
        if let Some(fallback_label) = fallback_label {
            self.asm.emit_op(opcodes::POP);
            self.asm.emit_push_label(fallback_label);
            self.asm.emit_op(opcodes::JUMP);
        } else {
            self.asm.emit_push_label(revert_label);
            self.asm.emit_op(opcodes::JUMP);
        }
    }

    /// Emits a payable check for non-payable functions.
    /// Non-payable, view, and pure functions revert if called with value.
    fn emit_payable_check(&mut self, func: &Function) {
        use solar_sema::hir::StateMutability;

        match func.attributes.state_mutability {
            StateMutability::Payable => {
                // Payable functions accept ETH - no check needed
            }
            StateMutability::NonPayable | StateMutability::View | StateMutability::Pure => {
                // CALLVALUE ISZERO ok JUMPI PUSH0 PUSH0 REVERT ok: JUMPDEST
                let ok_label = self.asm.new_label();

                self.asm.emit_op(opcodes::CALLVALUE);
                self.asm.emit_op(opcodes::ISZERO);
                self.asm.emit_push_label(ok_label);
                self.asm.emit_op(opcodes::JUMPI);
                // Revert with empty data
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_op(opcodes::REVERT);

                self.asm.define_label(ok_label);
                self.asm.emit_op(opcodes::JUMPDEST);
            }
        }
    }

    /// Generates bytecode for a function.
    #[allow(dead_code)]
    fn generate_function(&mut self, func: &Function) {
        self.generate_function_body(func);
    }

    /// Generates the body of a function.
    fn generate_function_body(&mut self, func: &Function) {
        // Run analysis pipeline via the pass manager.
        // Currently just liveness; future analyses (dominance, loops, etc.)
        // can be added here and queried from the same AnalysisManager.
        let mut am = AnalysisManager::new();
        let liveness: &Liveness = am.get_or_compute(&LivenessAnalysis, func);

        // Eliminate phis
        let phi_result = eliminate_phis(func);
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }

        // Reset scheduler
        self.scheduler = StackScheduler::new();

        self.preallocate_cross_block_spills(func, liveness);

        // Create labels for each block
        self.block_labels.clear();
        for block_id in func.blocks.indices() {
            self.block_labels.insert(block_id, self.asm.new_label());
        }

        // Generate each block.
        let block_order: Vec<_> = func.blocks.indices().collect();
        let mut preserved_fallthrough: Option<BlockId> = None;
        for (pos, &block_id) in block_order.iter().enumerate() {
            let block = &func.blocks[block_id];
            let fallthrough = block_order.get(pos + 1).copied();
            let entered_by_preserved_fallthrough = preserved_fallthrough == Some(block_id);
            preserved_fallthrough = None;

            // Define block label
            self.asm.define_label(self.block_labels[&block_id]);
            if !entered_by_preserved_fallthrough
                && (block_id != func.entry_block || !block.predecessors.is_empty())
            {
                self.asm.emit_op(opcodes::JUMPDEST);
            }

            // Reset stack at block entry unless the previous physical block falls through with a
            // known stack layout. All other cross-block values live in spill slots.
            if !entered_by_preserved_fallthrough {
                self.scheduler.clear_stack();
            }

            // Generate instructions
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                let inst = &func.instructions[inst_id];

                // Skip phi instructions (they're handled by copies)
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }

                // Find the value ID that corresponds to this instruction (if any)
                let result_value = func
                    .values
                    .iter_enumerated()
                    .find(|(_, v)| matches!(v, crate::mir::Value::Inst(id) if *id == inst_id))
                    .map(|(vid, _)| vid);

                // Generate the instruction
                self.generate_inst(func, &inst.kind, liveness, block_id, inst_idx, result_value);
            }

            // Insert phi copies before terminator
            if let Some(copies) = self.block_copies.remove(&block_id) {
                let mut temps = FxHashMap::default();
                for copy in &copies {
                    self.generate_copy(func, copy, &mut temps);
                }
            }

            let preserve_stack_to_fallthrough =
                self.can_preserve_stack_fallthrough(func, block_id, fallthrough);

            // Spill all live-out values before the terminator so they can be reloaded in successor
            // blocks. For a single-predecessor physical fallthrough, keep stack values live
            // instead.
            if !preserve_stack_to_fallthrough {
                self.spill_live_out_values(func, liveness, block_id);
            }

            // Generate terminator
            if let Some(term) = &block.terminator {
                self.generate_terminator(func, term, fallthrough, preserve_stack_to_fallthrough);
            }
            if preserve_stack_to_fallthrough {
                preserved_fallthrough = fallthrough;
            }
        }
    }

    fn can_preserve_stack_fallthrough(
        &self,
        func: &Function,
        block_id: BlockId,
        fallthrough: Option<BlockId>,
    ) -> bool {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
            return false;
        };
        if Some(*target) != fallthrough {
            return false;
        }

        // This block is the target's only predecessor, so no non-fallthrough edge can observe or
        // depend on a JUMPDEST at the target label.
        func.blocks[*target].predecessors.as_slice() == [block_id]
    }

    /// Preallocates stable spill slots for values that may cross block boundaries.
    ///
    /// Blocks are emitted in layout order, not necessarily dominance order, so a block can be
    /// emitted before the predecessor that stores one of its live-in values. Reserving the slot up
    /// front lets the later load use a stable memory location; stores still happen only when the
    /// value is actually available on the stack.
    fn preallocate_cross_block_spills(&mut self, func: &Function, liveness: &Liveness) {
        for block_id in func.blocks.indices() {
            for val in liveness.live_in(block_id).iter().chain(liveness.live_out(block_id).iter()) {
                if matches!(
                    func.value(val),
                    crate::mir::Value::Inst(_) | crate::mir::Value::Phi { .. }
                ) {
                    self.scheduler.spills.allocate(val);
                }
            }
        }
    }

    /// Spills all live-out values that are currently on the stack to memory.
    /// This ensures values that need to be accessed in successor blocks can be reloaded.
    fn spill_live_out_values(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        let live_out = liveness.live_out(block_id);

        for val in live_out.iter() {
            self.spill_value_if_needed(func, val);
        }
    }

    /// Spills a single value to memory if it's on the stack and not already spilled.
    /// Skips immediates and args since they can be re-emitted without spilling.
    fn spill_value_if_needed(&mut self, func: &Function, val: ValueId) {
        // Skip immediates and args - they can be re-emitted without spilling
        match func.value(val) {
            crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. } => return,
            _ => {}
        }

        if let Some(depth) = self.scheduler.stack.find(val) {
            let slot = self.scheduler.spills.allocate(val);

            // DUP the value to top of stack for storing.
            // We need to DUP (not just use ensure_on_top) because:
            // 1. If value is on top, ensure_on_top does nothing but we need a copy
            // 2. MSTORE will consume the value, and we want to preserve the original
            let dup_n = (depth + 1) as u8;
            self.asm.emit_op(opcodes::dup(dup_n));
            self.scheduler.stack.dup(dup_n);

            // Store to spill slot: PUSH offset, MSTORE
            // The PUSH creates an untracked stack entry, so we track it as unknown
            self.emit_spill_slot_addr(func, slot);
            self.scheduler.stack.push_unknown();

            self.asm.emit_op(opcodes::MSTORE);
            // MSTORE consumes 2 values: the untracked offset and the DUP'd value
            self.scheduler.stack.pop(); // pop the untracked offset
            self.scheduler.stack.pop(); // pop the DUP'd value (original remains)
        }
    }

    /// Spills operands that are live-out before an instruction consumes them.
    /// This ensures cross-block values are preserved in memory.
    fn spill_live_out_operands(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        operands: &[ValueId],
    ) {
        let live_out = liveness.live_out(block_id);

        for &op in operands {
            if live_out.contains(op) {
                self.spill_value_if_needed(func, op);
            }
        }
    }

    /// Generates bytecode for an instruction.
    fn generate_inst(
        &mut self,
        func: &Function,
        kind: &InstKind,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        result_value: Option<ValueId>,
    ) {
        // Spill any operands that are live-out before they get consumed.
        // This ensures cross-block values are preserved in memory.
        let operands = kind.operands();
        self.spill_live_out_operands(func, liveness, block, &operands);

        match kind {
            // Binary arithmetic operations
            InstKind::Add(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::ADD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sub(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SUB,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mul(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::MUL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Div(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::DIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SDiv(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SDIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::MOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SMod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Exp(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::EXP,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Bitwise operations
            InstKind::And(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::AND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Or(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::OR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Xor(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::XOR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Not(a) => self.emit_unary_op_with_result(
                func,
                *a,
                opcodes::NOT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shl(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                opcodes::SHL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shr(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                opcodes::SHR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sar(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                opcodes::SAR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Byte(i, x) => self.emit_binary_op_with_result(
                func,
                *i,
                *x,
                opcodes::BYTE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Comparison operations - track results for branch conditions and Select
            InstKind::Lt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::LT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Gt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::GT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SLt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SLT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SGt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SGT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Eq(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::EQ,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::IsZero(a) => self.emit_unary_op_with_result(
                func,
                *a,
                opcodes::ISZERO,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Memory operations
            // Track MLOAD results so they can be used as operands in subsequent instructions.
            // This is essential for nested external calls where the return value from one call
            // becomes an argument to another call.
            InstKind::MLoad(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::MLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                opcodes::MSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore8(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                opcodes::MSTORE8,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MSize => {
                self.asm.emit_op(opcodes::MSIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Storage operations
            InstKind::SLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                opcodes::SLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                opcodes::SSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                opcodes::TLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TStore(slot, val) => self.emit_store_op(func, *slot, *val, opcodes::TSTORE),

            // Calldata operations
            InstKind::CalldataLoad(off) => self.emit_unary_op_with_result(
                func,
                *off,
                opcodes::CALLDATALOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CalldataSize => {
                self.asm.emit_op(opcodes::CALLDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Hash operations
            InstKind::Keccak256(off, len) => self.emit_binary_op_with_result(
                func,
                *off,
                *len,
                opcodes::KECCAK256,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Environment operations
            InstKind::Caller => {
                self.asm.emit_op(opcodes::CALLER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::CallValue => {
                self.asm.emit_op(opcodes::CALLVALUE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Address => {
                self.asm.emit_op(opcodes::ADDRESS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Origin => {
                self.asm.emit_op(opcodes::ORIGIN);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasPrice => {
                self.asm.emit_op(opcodes::GASPRICE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Gas => {
                self.asm.emit_op(opcodes::GAS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Timestamp => {
                self.asm.emit_op(opcodes::TIMESTAMP);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlockNumber => {
                self.asm.emit_op(opcodes::NUMBER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Coinbase => {
                self.asm.emit_op(opcodes::COINBASE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ChainId => {
                self.asm.emit_op(opcodes::CHAINID);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::SelfBalance => {
                self.asm.emit_op(opcodes::SELFBALANCE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BaseFee => {
                self.asm.emit_op(opcodes::BASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlobBaseFee => {
                self.asm.emit_op(opcodes::BLOBBASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasLimit => {
                self.asm.emit_op(opcodes::GASLIMIT);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::PrevRandao => {
                self.asm.emit_op(opcodes::PREVRANDAO);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Balance(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::BALANCE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlockHash(num) => self.emit_unary_op_with_result(
                func,
                *num,
                opcodes::BLOCKHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlobHash(idx) => self.emit_unary_op_with_result(
                func,
                *idx,
                opcodes::BLOBHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeSize(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::EXTCODESIZE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeHash(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::EXTCODEHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CodeSize => {
                self.asm.emit_op(opcodes::CODESIZE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ReturnDataSize => {
                self.asm.emit_op(opcodes::RETURNDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Ternary operations
            InstKind::AddMod(a, b, n) => self.emit_ternary_op(func, *a, *b, *n, opcodes::ADDMOD),
            InstKind::MulMod(a, b, n) => self.emit_ternary_op(func, *a, *b, *n, opcodes::MULMOD),

            // Select is like a ternary conditional
            InstKind::Select(cond, true_val, false_val) => {
                // select(cond, t, f) = f + cond * (t - f)
                //
                // We emit all three values to the stack, then do inline computation.
                // Stack notation: rightmost = top (depth 0).
                // Stack after emit_value calls: [f, t, cond] with cond on top.

                self.emit_value(func, *false_val); // Stack: [f]
                self.emit_value(func, *true_val); // Stack: [f, t]
                self.emit_value(func, *cond); // Stack: [f, t, cond]

                // Now compute: f + cond * (t - f)
                // Stack is [f, t, cond] with cond on top (depth 0), t at depth 1, f at depth 2
                //
                // Step 1: DUP3 to get f -> [f, t, cond, f]
                self.emit_stack_op(StackOp::Dup(3));
                // Step 2: DUP3 to get t (now at depth 2) -> [f, t, cond, f, t]
                self.emit_stack_op(StackOp::Dup(3));
                // Step 3: SUB (top - second = t - f) -> [f, t, cond, t-f]
                self.emit_op_with_effect(
                    opcodes::SUB,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 4: MUL (cond * (t-f)) -> [f, t, cond*(t-f)]
                self.emit_op_with_effect(
                    opcodes::MUL,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 5: SWAP1 -> [f, cond*(t-f), t]
                self.emit_stack_op(StackOp::Swap(1));
                // Step 6: POP (remove t) -> [f, cond*(t-f)]
                self.emit_stack_op(StackOp::Pop);
                // Step 7: ADD (cond*(t-f) + f = f + cond*(t-f)) -> [result]
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(opcodes::ADD, StackEffect { pops: 2, pushes: 1 }, push);
            }

            // Sign extend
            InstKind::SignExtend(b, x) => self.emit_binary_op_with_result(
                func,
                *b,
                *x,
                opcodes::SIGNEXTEND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // Contract creation
            InstKind::Create(value, offset, size) => {
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *value);
                self.asm.emit_op(opcodes::CREATE);
                // CREATE consumes 3 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(3, result_value);
            }

            InstKind::Create2(value, offset, size, salt) => {
                // CREATE2 expects stack (top to bottom): salt, size, offset, value
                // So we push in reverse order: value first (goes deepest), then offset, size, salt
                self.emit_value(func, *value);
                self.emit_value(func, *offset);
                self.emit_value(func, *size);
                self.emit_value(func, *salt);
                self.asm.emit_op(opcodes::CREATE2);
                // CREATE2 consumes 4 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(4, result_value);
            }

            // External calls
            //
            // These use emit_value_fresh to guarantee correct values regardless of scheduler state.
            // The stack-aware emit_op_with_effect ensures proper tracking after emission.
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
                // EVM pops in order: gas (TOS), addr, value, argsOffset, argsSize, retOffset,
                // retSize So we push in reverse order: retSize first (deepest), gas
                // last (TOS)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *addr);
                self.emit_value_fresh(func, *gas);

                // CALL consumes 7 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(opcodes::CALL, StackEffect { pops: 7, pushes: 1 }, push);
            }

            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // STATICCALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_value_fresh(func, *gas);
                // STATICCALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    opcodes::STATICCALL,
                    StackEffect { pops: 6, pushes: 1 },
                    push,
                );
            }

            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // DELEGATECALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_value_fresh(func, *gas);
                // DELEGATECALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    opcodes::DELEGATECALL,
                    StackEffect { pops: 6, pushes: 1 },
                    push,
                );
            }

            InstKind::InternalCall { function, args, returns } => {
                self.emit_internal_call(
                    func,
                    *function,
                    args,
                    *returns,
                    result_value,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::InternalFrameAddr(offset) => {
                self.emit_current_internal_frame_addr(*offset);
                if let Some(result) = result_value {
                    self.scheduler.stack.push(result);
                }
            }

            // Log operations
            InstKind::Log0(offset, size) => {
                // LOG0(offset, size) - stack order: offset on top, then size
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG0);
                self.scheduler.instruction_executed(2, None);
            }
            InstKind::Log1(offset, size, topic1) => {
                // LOG1(offset, size, topic1) - stack order: offset, size, topic1
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG1);
                self.scheduler.instruction_executed(3, None);
            }
            InstKind::Log2(offset, size, topic1, topic2) => {
                // LOG2(offset, size, topic1, topic2) - stack order: offset, size, topic1, topic2
                self.emit_value(func, *topic2);
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG2);
                self.scheduler.instruction_executed(4, None);
            }
            InstKind::Log3(offset, size, topic1, topic2, topic3) => {
                // LOG3(offset, size, topic1, topic2, topic3)
                self.emit_value(func, *topic3);
                self.emit_value(func, *topic2);
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG3);
                self.scheduler.instruction_executed(5, None);
            }
            InstKind::Log4(offset, size, topic1, topic2, topic3, topic4) => {
                // LOG4(offset, size, topic1, topic2, topic3, topic4)
                self.emit_value(func, *topic4);
                self.emit_value(func, *topic3);
                self.emit_value(func, *topic2);
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG4);
                self.scheduler.instruction_executed(6, None);
            }

            // Memory copy operations
            InstKind::CalldataCopy(dest, offset, size) => {
                // CALLDATACOPY(destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::CALLDATACOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::CodeCopy(dest, offset, size) => {
                // CODECOPY(destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::CODECOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::ReturnDataCopy(dest, offset, size) => {
                // RETURNDATACOPY(destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::RETURNDATACOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::MCopy(dest, src, size) => {
                // MCOPY(destOffset, srcOffset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *src);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::MCOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::ExtCodeCopy(addr, dest, offset, size) => {
                // EXTCODECOPY(address, destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.emit_value(func, *addr);
                self.asm.emit_op(opcodes::EXTCODECOPY);
                self.scheduler.instruction_executed(4, None);
            }
        }

        if let Some(result) = result_value
            && liveness.live_out(block).contains(result)
        {
            self.spill_value_if_needed(func, result);
        }

        // Drop dead values after the instruction
        let dead_ops = self.scheduler.drop_dead_values(liveness, block, inst_idx);
        for op in dead_ops {
            self.asm.emit_op(op.opcode());
        }

        #[cfg(debug_assertions)]
        {
            debug_assert!(self.scheduler.depth() <= 1024);
        }
    }

    fn emit_new_internal_frame_addr(&mut self, offset: u64) {
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(opcodes::MLOAD);
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(opcodes::ADD);
        }
    }

    fn emit_current_internal_frame_addr(&mut self, offset: u64) {
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(opcodes::MLOAD);
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(opcodes::ADD);
        }
    }

    fn spill_frame_size(func: &Function) -> u64 {
        func.values.len() as u64 * 32
    }

    fn external_spill_base(func: &Function) -> u64 {
        LOW_MEMORY_START + func.internal_frame_size.max(func.external_static_return_size)
    }

    fn external_free_memory_start(func: &Function) -> u64 {
        Self::external_spill_base(func) + Self::spill_frame_size(func)
    }

    fn emit_external_free_memory_start(&mut self, func: &Function) {
        self.asm.emit_push(U256::from(Self::external_free_memory_start(func)));
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(opcodes::MSTORE);
    }

    fn emit_spill_slot_addr(&mut self, func: &Function, slot: SpillSlot) {
        if self.in_internal_function {
            let spill_base =
                64 + (func.params.len() as u64) * 32 + (func.returns.len() as u64) * 32;
            self.emit_current_internal_frame_addr(
                spill_base + func.internal_frame_size + u64::from(slot.offset) * 32,
            );
        } else if self.in_constructor {
            self.asm.emit_push(U256::from(slot.byte_offset()));
        } else {
            self.asm.emit_push(U256::from(
                Self::external_spill_base(func) + u64::from(slot.offset) * 32,
            ));
        }
    }

    fn emit_internal_arg_load(&mut self, index: u32) {
        self.emit_current_internal_frame_addr(64 + u64::from(index) * 32);
        self.asm.emit_op(opcodes::MLOAD);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_internal_call(
        &mut self,
        func: &Function,
        callee: FunctionId,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let Some(&callee_label) = self.function_labels.get(&callee) else {
            return;
        };
        let return_label = self.asm.new_label();
        let local_frame_size = self.function_frame_sizes.get(&callee).copied().unwrap_or_default();
        let frame_size = 64 + ((args.len() + returns) as u64) * 32 + local_frame_size;

        // frame[0] = return label
        self.asm.emit_push_label(return_label);
        self.emit_new_internal_frame_addr(0);
        self.asm.emit_op(opcodes::MSTORE);

        // frame[32] = previous frame pointer
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(opcodes::MLOAD);
        self.emit_new_internal_frame_addr(32);
        self.asm.emit_op(opcodes::MSTORE);

        // Spill values that are live after this call BEFORE consuming the
        // arguments. An argument that is also used later (e.g. a flag passed to
        // a helper and then stored, as in `tryAdd`) would otherwise be popped by
        // the arg-store loop below and then lost when the stack is cleared for
        // the call, leaving it unavailable at its later use.
        self.spill_live_stack_values(func, liveness, block, inst_idx);

        for (i, &arg) in args.iter().enumerate() {
            self.emit_value(func, arg);
            self.emit_new_internal_frame_addr(64 + (i as u64) * 32);
            self.asm.emit_op(opcodes::MSTORE);
            self.scheduler.stack.pop();
        }

        self.pop_all_stack_values();
        self.scheduler.clear_stack();

        // current_frame = frame
        self.emit_new_internal_frame_addr(0);
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(opcodes::MSTORE);

        // free_ptr += frame_size
        self.emit_new_internal_frame_addr(frame_size);
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(opcodes::MSTORE);

        self.asm.emit_push_label(callee_label);
        self.asm.emit_op(opcodes::JUMP);

        self.asm.define_label(return_label);
        self.asm.emit_op(opcodes::JUMPDEST);
        self.scheduler.clear_stack();

        if let Some(result) = result
            && returns > 0
        {
            self.emit_current_internal_frame_addr(64 + (args.len() as u64) * 32);
            self.asm.emit_op(opcodes::MLOAD);
            self.scheduler.stack.push(result);
        }

        // Copy return values 2..N from the callee frame into scratch memory at
        // offset `i * 32`, matching what the caller's `lower_multi_var_decl`
        // reads via `mload(i * 32)`. This must happen before the frame pointer is
        // restored below, while the callee frame is still addressable. The first
        // return flows back as `result` on the stack (above); these copies have a
        // net-zero stack effect so they leave it untouched.
        for i in 1..returns {
            self.emit_current_internal_frame_addr(64 + (args.len() as u64) * 32 + (i as u64) * 32);
            self.asm.emit_op(opcodes::MLOAD);
            self.asm.emit_push(U256::from((i as u64) * 32));
            self.asm.emit_op(opcodes::MSTORE);
        }

        // Restore the caller frame pointer. If a result is on the stack, this leaves it there.
        self.emit_current_internal_frame_addr(32);
        self.asm.emit_op(opcodes::MLOAD);
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(opcodes::MSTORE);
    }

    fn spill_live_stack_values(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let stack_values: Vec<_> = self.scheduler.stack.iter().flatten().collect();
        for value in stack_values {
            if !liveness.is_dead_after(value, block, inst_idx) {
                self.spill_value_if_needed(func, value);
            }
        }
    }

    /// Emits a value to the stack.
    fn emit_value(&mut self, func: &Function, val: ValueId) {
        let ops = self.scheduler.ensure_on_top(val, func).to_vec();
        for op in ops {
            match op {
                ScheduledOp::Stack(stack_op) => {
                    self.asm.emit_op(stack_op.opcode());
                }
                ScheduledOp::PushImmediate(imm) => {
                    self.asm.emit_push(imm);
                }
                ScheduledOp::LoadSpill(slot) => {
                    // PUSH slot_offset, MLOAD
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(opcodes::MLOAD);
                }
                ScheduledOp::SaveSpill(slot) => {
                    // PUSH slot_offset, MSTORE
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(opcodes::MSTORE);
                }
                ScheduledOp::LoadArg(index) => {
                    let arg_ty = func.params.get(index as usize).copied();
                    if self.in_internal_function {
                        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
                        self.asm.emit_op(opcodes::MLOAD);
                        self.asm.emit_push(U256::from(64 + u64::from(index) * 32));
                        self.asm.emit_op(opcodes::ADD);
                        self.asm.emit_op(opcodes::MLOAD);
                    } else if self.in_constructor && matches!(arg_ty, Some(MirType::MemPtr)) {
                        Self::emit_constructor_short_string_arg(&mut self.asm, index);
                    } else if self.in_constructor {
                        // Constructor args were copied to memory at 0x80
                        // Load from memory: 0x80 + index * 32
                        let offset = 0x80 + (index as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(opcodes::MLOAD);
                    } else {
                        // Runtime function: load from calldata
                        // ABI encoding: selector (4 bytes) + args (32 bytes each)
                        // Offset = 4 + index * 32
                        let offset = 4 + (index as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(opcodes::CALLDATALOAD);
                    }
                }
            }
        }
    }

    /// Emits a constructor dynamic string/bytes argument as a short storage string word.
    ///
    /// Constructor args are copied to memory at 0x80 in ABI encoding:
    /// `head[index] = dynamic tail offset, tail = [length][data...]`.
    /// The current codegen supports short storage strings (<=31 bytes). Long storage strings need
    /// separate data-slot handling.
    fn emit_constructor_short_string_arg(asm: &mut Assembler, index: u32) {
        let head_offset = 0x80 + (index as u64) * 32;
        let low_byte_mask = U256::MAX - U256::from(0xffu64);

        // data_ptr = 0x80 + mload(head_offset)
        asm.emit_push(U256::from(head_offset));
        asm.emit_op(opcodes::MLOAD);
        asm.emit_push(U256::from(0x80));
        asm.emit_op(opcodes::ADD);

        // Stack: data_ptr
        // Keep data_ptr while loading length and first data word.
        asm.emit_op(opcodes::dup(1));
        asm.emit_op(opcodes::MLOAD); // data_ptr, len
        asm.emit_op(opcodes::dup(2)); // data_ptr, len, data_ptr
        asm.emit_push(U256::from(32));
        asm.emit_op(opcodes::ADD);
        asm.emit_op(opcodes::MLOAD); // data_ptr, len, data_word

        // Clear the low length byte in the data word.
        asm.emit_push(low_byte_mask);
        asm.emit_op(opcodes::AND); // data_ptr, len, data_clean

        // Drop data_ptr, then OR data_clean with len * 2.
        asm.emit_op(opcodes::swap(2)); // data_clean, len, data_ptr
        asm.emit_op(opcodes::POP); // data_clean, len
        asm.emit_push(U256::from(1));
        asm.emit_op(opcodes::SHL); // data_clean, len * 2
        asm.emit_op(opcodes::OR); // encoded short storage string
    }

    /// Emits a value fresh, without trying to DUP from the stack.
    /// This is used for CALL operands where we need to guarantee correct values
    /// regardless of scheduler stack tracking state.
    fn emit_value_fresh(&mut self, func: &Function, val: ValueId) {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                if let Some(u256) = imm.as_u256() {
                    self.asm.emit_push(u256);
                    self.scheduler.stack.push(val);
                }
            }
            crate::mir::Value::Arg { index, .. } => {
                if self.in_internal_function {
                    self.emit_internal_arg_load(*index);
                } else if self.in_constructor {
                    let offset = 0x80 + (*index as u64) * 32;
                    self.asm.emit_push(U256::from(offset));
                    self.asm.emit_op(opcodes::MLOAD);
                } else {
                    let offset = 4 + (*index as u64) * 32;
                    self.asm.emit_push(U256::from(offset));
                    self.asm.emit_op(opcodes::CALLDATALOAD);
                }
                self.scheduler.stack.push(val);
            }
            crate::mir::Value::Inst(inst_id) => {
                // For instruction results, we need to check if they're spilled
                // or if they're instruction results that produce fresh values (like GAS, MLOAD)
                if let Some(slot) = self.scheduler.spills.get(val) {
                    // Load from spill slot
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(opcodes::MLOAD);
                    self.scheduler.stack.push(val);
                } else {
                    // Check if the instruction is one that we can "re-execute" to get a fresh value
                    // This handles GAS (which is always fresh) and MLOAD (which re-reads from
                    // memory)
                    let inst_kind = &func.instruction(*inst_id).kind;
                    match inst_kind {
                        crate::mir::InstKind::Gas => {
                            self.asm.emit_op(opcodes::GAS);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CallValue => {
                            self.asm.emit_op(opcodes::CALLVALUE);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Caller => {
                            self.asm.emit_op(opcodes::CALLER);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Origin => {
                            self.asm.emit_op(opcodes::ORIGIN);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CalldataSize => {
                            self.asm.emit_op(opcodes::CALLDATASIZE);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::InternalFrameAddr(offset) => {
                            self.emit_current_internal_frame_addr(*offset);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Timestamp => {
                            self.asm.emit_op(opcodes::TIMESTAMP);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::BlockNumber => {
                            self.asm.emit_op(opcodes::NUMBER);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::MLoad(offset) => {
                            // Note: Re-emitting MLOAD(0x40) is incorrect for struct pointers
                            // because the free memory pointer changes. However, with spill
                            // slots now at 0x1000+ (away from dynamic allocations), values
                            // should be properly spilled and reloaded, so we shouldn't hit
                            // this path for struct pointers.
                            //
                            // For other MLOAD addresses (reading from constant locations),
                            // re-emit is safe.
                            self.emit_value_fresh(func, *offset);
                            self.asm.emit_op(opcodes::MLOAD);
                            // Pop offset, push result
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Keccak256(offset, size) => {
                            // Re-emit KECCAK256 - memory content should still be valid
                            self.emit_value_fresh(func, *offset);
                            self.emit_value_fresh(func, *size);
                            self.asm.emit_op(opcodes::KECCAK256);
                            // Pop offset and size, push result
                            self.scheduler.stack.pop();
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Add(a, b) => {
                            // Re-emit ADD
                            self.emit_value_fresh(func, *a);
                            self.emit_value_fresh(func, *b);
                            self.asm.emit_op(opcodes::ADD);
                            self.scheduler.stack.pop();
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Sub(a, b) => {
                            // Re-emit SUB
                            self.emit_value_fresh(func, *a);
                            self.emit_value_fresh(func, *b);
                            self.asm.emit_op(opcodes::SUB);
                            self.scheduler.stack.pop();
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Mul(a, b) => {
                            // Re-emit MUL
                            self.emit_value_fresh(func, *a);
                            self.emit_value_fresh(func, *b);
                            self.asm.emit_op(opcodes::MUL);
                            self.scheduler.stack.pop();
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        other => {
                            // For other instruction results that aren't spilled and can't be
                            // re-executed, this is a bug - the lowering
                            // should have ensured all CALL operands are
                            // either immediates, spilled, or re-executable instructions.
                            panic!(
                                "emit_value_fresh: unhandled instruction kind {other:?} for value {val:?}. \
                                 CALL operands should be immediates, spilled values, GAS, or MLOAD."
                            );
                        }
                    }
                }
            }
            crate::mir::Value::Phi { .. } | crate::mir::Value::Undef(_) => {
                // Phi nodes and undef values shouldn't appear in CALL operands
                panic!(
                    "emit_value_fresh: unexpected phi/undef value {val:?}. \
                     CALL operands should be concrete values."
                );
            }
        }
    }

    /// Emits a binary operation with result tracking and liveness awareness.
    /// If an operand is still live after this instruction, we DUP it before it gets consumed.
    #[allow(clippy::too_many_arguments)]
    fn emit_binary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // Check if operands are still live after this instruction
        let a_is_live = !liveness.is_dead_after(a, block, inst_idx);
        let b_is_live = !liveness.is_dead_after(b, block, inst_idx);

        // Helper to check if a value can be re-emitted (immediates and args don't need spilling)
        let can_reemit = |v: ValueId| {
            matches!(func.value(v), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
        };

        // Special case: same operand used twice (e.g., a + a, a - a)
        if a == b {
            self.emit_value(func, a);
            // Spill if live-after (now that it's on stack)
            if a_is_live && !can_reemit(a) {
                self.spill_value_if_needed(func, a);
            }
            // DUP for the second operand
            self.asm.emit_op(opcodes::DUP1);
            self.scheduler.stack.dup(1);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Check if either operand is already on stack as an untracked value
        let a_can_emit = self.scheduler.can_emit_value(a, func);
        let b_can_emit = self.scheduler.can_emit_value(b, func);
        let has_untracked = self.scheduler.has_untracked_on_top();
        let has_untracked_at_1 = self.scheduler.has_untracked_at_depth(1);

        if !a_can_emit && b_can_emit && has_untracked {
            // a is an untracked value on top of stack, emit b, then SWAP
            self.emit_value(func, b);
            // Spill b if live-after (it's now at depth 0)
            if b_is_live && !can_reemit(b) {
                self.spill_value_if_needed(func, b);
            }
            self.asm.emit_op(opcodes::SWAP1);
            self.scheduler.stack_swapped();
        } else if a_can_emit && !b_can_emit && has_untracked {
            // b is an untracked value on top of stack, emit a on top
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0)
            if a_is_live && !can_reemit(a) {
                self.spill_value_if_needed(func, a);
            }
        } else if !a_can_emit && b_can_emit && has_untracked_at_1 {
            // a is an untracked value at depth 1, b is tracked on top
            // Stack is [b, a_untracked], need [a, b]
            self.asm.emit_op(opcodes::SWAP1);
            self.scheduler.stack_swapped();
        } else {
            // Normal case: emit b first (bottom), then a (top)
            self.emit_value(func, b);
            // Spill b if live-after (it's now at depth 0)
            if b_is_live && !can_reemit(b) {
                self.spill_value_if_needed(func, b);
            }
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0)
            if a_is_live && !can_reemit(a) {
                self.spill_value_if_needed(func, a);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, result);
    }

    /// Emits a unary operation with result tracking and liveness awareness.
    /// If the operand is still live after this instruction, we spill it after emitting.
    #[allow(clippy::too_many_arguments)]
    fn emit_unary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // Check if the operand is still live after this instruction
        let a_is_live = !liveness.is_dead_after(a, block, inst_idx);

        // Check if value can be re-emitted (immediates and args don't need spilling)
        let can_reemit = matches!(
            func.value(a),
            crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. }
        );

        self.emit_value(func, a);

        // Spill the operand AFTER emitting if it's live-after (now it's on stack at depth 0)
        if a_is_live && !can_reemit {
            self.spill_value_if_needed(func, a);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(1, result);
    }

    /// Emits a store operation (consumes both operands, no result).
    fn emit_store_op(&mut self, func: &Function, addr: ValueId, val: ValueId, opcode: u8) {
        self.emit_value(func, val);
        self.emit_value(func, addr);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a store operation with liveness awareness.
    /// If the value operand is still live after this instruction, we spill it after emitting
    /// to preserve it for later use.
    #[allow(clippy::too_many_arguments)]
    fn emit_store_op_live_aware(
        &mut self,
        func: &Function,
        addr: ValueId,
        val: ValueId,
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // Check if val is still live after this instruction
        let val_is_live = !liveness.is_dead_after(val, block, inst_idx);
        // Check if addr is still live after this instruction
        let addr_is_live = !liveness.is_dead_after(addr, block, inst_idx);

        // Helper to check if a value can be re-emitted (immediates and args don't need spilling)
        let can_reemit = |v: ValueId| {
            matches!(func.value(v), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
        };

        // Emit val
        self.emit_value(func, val);
        // Spill val if live-after (it's now at depth 0)
        if val_is_live && !can_reemit(val) {
            self.spill_value_if_needed(func, val);
        }

        // Emit addr
        self.emit_value(func, addr);
        // Spill addr if live-after (it's now at depth 0)
        if addr_is_live && !can_reemit(addr) {
            self.spill_value_if_needed(func, addr);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a ternary operation.
    fn emit_ternary_op(&mut self, func: &Function, a: ValueId, b: ValueId, c: ValueId, opcode: u8) {
        self.emit_value(func, c);
        self.emit_value(func, b);
        self.emit_value(func, a);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(3, None);
    }

    /// Generates a parallel copy.
    ///
    /// Phi copies move values from source to destination. The destination is typically
    /// a phi result that needs to be available in the successor block. We handle this
    /// by spilling the source value to the destination's spill slot.
    fn generate_copy(
        &mut self,
        func: &Function,
        copy: &ParallelCopy,
        temps: &mut FxHashMap<u32, ValueId>,
    ) {
        // Handle source: either a MIR value or a temporary
        match &copy.src {
            CopySource::Value(val) => {
                self.emit_value(func, *val);
            }
            CopySource::Temp(temp_id) => {
                // Temporaries are tracked in our temps map with their ValueId
                if let Some(&temp_val) = temps.get(temp_id) {
                    // DUP the temp value to top of stack
                    if let Some(depth) = self.scheduler.stack.find(temp_val) {
                        let dup_n = (depth + 1) as u8;
                        self.asm.emit_op(opcodes::dup(dup_n));
                        self.scheduler.stack.dup(dup_n);
                    }
                }
            }
        }

        // Handle destination: either a MIR value or a temporary
        match &copy.dst {
            CopyDest::Value(dst_val) => {
                // Spill the value on top of stack to the destination's spill slot
                // This allows the successor block to reload it
                let slot = self.scheduler.spills.allocate(*dst_val);
                self.emit_spill_slot_addr(func, slot);
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(opcodes::MSTORE);
                self.scheduler.stack.pop(); // pop the untracked offset
                self.scheduler.stack.pop(); // pop the value
            }
            CopyDest::Temp(temp_id) => {
                // Mark this temporary as defined - it's now on the stack
                // Get the ValueId of the value currently on top
                if let Some(val_on_top) = self.scheduler.stack.top() {
                    temps.insert(*temp_id, val_on_top);
                }
            }
        }
    }

    /// Pops all remaining values from the stack.
    /// This ensures the stack is empty before control flow transfer to another block.
    fn pop_all_stack_values(&mut self) {
        while self.scheduler.stack_depth() > 0 {
            self.asm.emit_op(opcodes::POP);
            self.scheduler.stack.pop();
        }
    }

    fn emit_internal_return(&mut self, func: &Function, values: &[ValueId]) {
        let return_base = 64 + (func.params.len() as u64) * 32;
        for (i, &value) in values.iter().enumerate() {
            self.emit_value(func, value);
            self.emit_current_internal_frame_addr(return_base + (i as u64) * 32);
            self.asm.emit_op(opcodes::MSTORE);
            self.scheduler.stack.pop();
        }

        self.pop_all_stack_values();
        self.emit_current_internal_frame_addr(0);
        self.asm.emit_op(opcodes::MLOAD);
        self.asm.emit_op(opcodes::JUMP);
    }

    /// Generates bytecode for a terminator.
    fn generate_terminator(
        &mut self,
        func: &Function,
        term: &Terminator,
        fallthrough: Option<BlockId>,
        preserve_stack_to_fallthrough: bool,
    ) {
        match term {
            Terminator::Jump(target) => {
                // Pop any remaining values from the stack before jumping.
                // Each block starts with an empty stack, so we must ensure the stack is
                // clean before jumping to another block (especially important for loops).
                if Some(*target) == fallthrough {
                    if !preserve_stack_to_fallthrough {
                        self.pop_all_stack_values();
                    }
                    return;
                }
                self.pop_all_stack_values();
                self.asm.emit_push_label(self.block_labels[target]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Branch { condition, then_block, else_block } => {
                // Emit the condition first (it's still on the stack)
                self.emit_value(func, *condition);

                // Now pop all OTHER values (condition is on top, keep it)
                // We do this by tracking that condition was just pushed and emitting POPs for
                // everything else
                while self.scheduler.depth() > 1 {
                    // SWAP to get unwanted value to top, then POP
                    self.asm.emit_op(opcodes::SWAP1);
                    self.scheduler.stack_swapped();
                    self.asm.emit_op(opcodes::POP);
                    self.scheduler.stack.pop();
                }

                match fallthrough {
                    Some(next) if *else_block == next => {
                        // JUMPI consumes the condition; false falls through to `else_block`.
                        self.asm.emit_push_label(self.block_labels[then_block]);
                        self.asm.emit_op(opcodes::JUMPI);
                        self.scheduler.stack.pop(); // condition consumed by JUMPI
                    }
                    Some(next) if *then_block == next => {
                        // Invert the condition so true falls through to `then_block`.
                        self.asm.emit_op(opcodes::ISZERO);
                        self.scheduler.instruction_executed_untracked(1);
                        self.asm.emit_push_label(self.block_labels[else_block]);
                        self.asm.emit_op(opcodes::JUMPI);
                        self.scheduler.stack.pop(); // inverted condition consumed by JUMPI
                    }
                    _ => {
                        // JUMPI consumes the condition
                        self.asm.emit_push_label(self.block_labels[then_block]);
                        self.asm.emit_op(opcodes::JUMPI);
                        self.scheduler.stack.pop(); // condition consumed by JUMPI

                        self.asm.emit_push_label(self.block_labels[else_block]);
                        self.asm.emit_op(opcodes::JUMP);
                    }
                }
            }

            Terminator::Switch { value, default, cases } => {
                // Pop all stack values first (live-out values are already spilled)
                self.pop_all_stack_values();

                // Emit the switch value (will reload from spill if needed)
                self.emit_value(func, *value);

                for (case_val, target) in cases {
                    // DUP the value, compare, jump if equal
                    self.asm.emit_op(opcodes::DUP1);
                    self.scheduler.stack.dup(1);
                    self.emit_value(func, *case_val);
                    self.asm.emit_op(opcodes::EQ);
                    self.scheduler.instruction_executed(2, None); // EQ consumes 2, pushes 1
                    self.asm.emit_push_label(self.block_labels[target]);
                    self.asm.emit_op(opcodes::JUMPI);
                    self.scheduler.instruction_executed(1, None); // JUMPI consumes condition
                }

                // Pop the value and jump to default
                self.asm.emit_op(opcodes::POP);
                self.scheduler.stack.pop();
                self.asm.emit_push_label(self.block_labels[default]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Return { values } => {
                if self.in_internal_function {
                    self.emit_internal_return(func, values);
                    return;
                }

                assert!(values.is_empty(), "external ABI returns with values must use ReturnData");
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_op(opcodes::RETURN);
            }

            Terminator::Revert { offset, size } => {
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::REVERT);
            }

            Terminator::ReturnData { offset, size } => {
                debug_assert!(!self.in_internal_function);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::RETURN);
            }

            Terminator::Stop => {
                if self.in_internal_function {
                    self.emit_internal_return(func, &[]);
                } else {
                    self.asm.emit_op(opcodes::STOP);
                }
            }

            Terminator::SelfDestruct { recipient } => {
                self.emit_value(func, *recipient);
                self.asm.emit_op(opcodes::SELFDESTRUCT);
            }

            Terminator::Invalid => {
                self.asm.emit_op(opcodes::INVALID);
            }
        }
    }
}

impl Default for EvmCodegen {
    fn default() -> Self {
        Self::new()
    }
}

/// The artifact produced by the EVM backend: deployment and runtime bytecode.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Deployment (init) bytecode that, when run, returns the runtime code.
    pub deployment: Vec<u8>,
    /// Runtime bytecode, i.e. the code stored on-chain.
    pub runtime: Vec<u8>,
}

impl crate::backend::Backend for EvmCodegen {
    type Output = EvmArtifact;

    fn name(&self) -> &str {
        "evm"
    }

    fn lower_module(&mut self, module: &mut Module) -> EvmArtifact {
        let (deployment, runtime) = self.generate_deployment_bytecode(module);
        EvmArtifact { deployment, runtime }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower;
    use solar_interface::Session;
    use solar_sema::Compiler;
    use std::{ops::ControlFlow, path::PathBuf};

    /// Helper to compile Solidity source to bytecode, returning Result.
    fn compile_source(source: &str) -> Result<Vec<u8>, String> {
        let sess = Session::builder().with_buffer_emitter(Default::default()).build();
        let mut compiler = Compiler::new(sess);

        // Parse
        let parse_result = compiler.enter_mut(|c| -> solar_interface::Result<_> {
            let mut ctx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("test.sol"), source.to_string())
                .unwrap();
            ctx.add_file(file);
            ctx.parse();
            Ok(())
        });
        if parse_result.is_err() {
            return Err("Parse error".to_string());
        }

        // Lower and codegen
        compiler.enter_mut(|c| -> Result<Vec<u8>, String> {
            let ControlFlow::Continue(()) = c.lower_asts().map_err(|_| "Lower AST error")? else {
                return Err("Lower AST break".to_string());
            };
            let ControlFlow::Continue(()) = c.analysis().map_err(|_| "Analysis error")? else {
                return Err("Analysis break".to_string());
            };

            let gcx = c.gcx();
            for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                if contract.name.as_str() == "Test" {
                    let mut module = lower::lower_contract(gcx, contract_id);
                    let mut codegen = EvmCodegen::new();
                    let bytecode = codegen.generate_module(&mut module);
                    return Ok(bytecode);
                }
            }
            Err("Contract 'Test' not found".to_string())
        })
    }

    #[test]
    fn test_local_var_in_conditional_ice() {
        // Minimal repro for stack underflow ICE:
        // 1. Read storage into local variable
        // 2. Use local variable in conditional check
        // 3. Use local variable inside the conditional body
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public value;
                function test() public {
                    uint256 v = value;
                    if (v != 0) value = v - 1;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }

    #[test]
    fn test_direct_storage_in_conditional_works() {
        // This works: directly referencing storage in both condition and body
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public value;
                function test() public {
                    if (value != 0) value = value - 1;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    }

    #[test]
    fn test_phi_value_used_after_if_else() {
        // Test case for phi node handling when a variable is assigned in both
        // if/else branches and then used after the if/else.
        // This pattern is common in Uniswap V2 and similar contracts.
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public totalSupply;
                function mint() external returns (uint256 liquidity) {
                    if (totalSupply == 0) {
                        liquidity = 1;
                    } else {
                        liquidity = 2;
                    }
                    totalSupply += liquidity;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }

    #[test]
    fn test_phi_value_used_multiple_times_after_if_else() {
        // Test case where the phi result is used multiple times after the if/else
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public totalSupply;
                function mint() external returns (uint256 result) {
                    uint256 liquidity;
                    if (totalSupply == 0) {
                        liquidity = 1;
                    } else {
                        liquidity = 2;
                    }
                    totalSupply += liquidity;
                    uint256 x = liquidity * 2;
                    result = x + liquidity;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }

    #[test]
    fn test_phi_with_ternary_in_branch() {
        // Complex phi case with nested ternary operators
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public totalSupply;
                uint256 public reserve0;
                uint256 public reserve1;
                
                function mint() external returns (uint256 liquidity) {
                    uint256 amount0 = 100;
                    uint256 amount1 = 200;
                    
                    if (totalSupply == 0) {
                        liquidity = amount0 * amount1;
                    } else {
                        uint256 l1 = (amount0 * totalSupply) / reserve0;
                        uint256 l2 = (amount1 * totalSupply) / reserve1;
                        liquidity = l1 < l2 ? l1 : l2;
                    }
                    
                    totalSupply += liquidity;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }
}
