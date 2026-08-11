//! MIR validator — checks SSA invariants on a [`Function`].
//!
//! This is the Solar equivalent of LLVM's `verify` pass / Cranelift's
//! `Function::verify`. It walks a function once and reports every invariant
//! violation it finds through the compiler diagnostic context.
//!
//! # Checks performed
//!
//! 1. **Defined-before-use**: every `ValueId` referenced as an operand has an entry in the
//!    function's value arena.
//! 2. **Block reference validity**: every `BlockId` mentioned in a terminator or phi has an entry
//!    in `func.blocks`.
//! 3. **Result consistency**: every value-producing instruction records a matching `Value::Inst`
//!    entry.
//! 4. **Terminator presence**: every block has a terminator.
//! 5. **Predecessor back-link**: if A's terminator targets B, then B's `predecessors` contains A.
//! 6. **Entry block has no predecessors**.
//! 7. **Phi block coverage**: every `InstKind::Phi`'s incoming blocks are predecessors of the
//!    containing block, and every predecessor has an incoming entry.
//! 8. **Instruction-block consistency**: each instruction's `block` field matches the block whose
//!    `instructions` vector contains it.
//! 9. **Predecessor consistency**: every stored predecessor actually branches to the block.
//! 10. **Use reachability**: for every reachable use of an instruction result, the defining block
//!     can reach the using block (phi inputs: their incoming predecessor). Within an acyclic block,
//!     the definition must also precede an instruction use. MIR is deliberately loose SSA, but a
//!     use its definition can never reach is garbage on every execution.
//! 11. **Call consistency**: internal and tail-call targets exist and their argument counts match
//!     the callee.
//! 12. **Immutable consistency**: immutable declarations and stores use supported representations,
//!     and loads use the declared type.
//!
//! # Usage
//!
//! ```ignore
//! solar_codegen::mir::validate(dcx, &module);
//! ```

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, FunctionId, InstId, InstKind, Module, Value, ValueId},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_interface::{diagnostics::DiagCtxt, kw};
use std::fmt;

/// Stateful MIR verifier.
struct Validator<'a> {
    dcx: &'a DiagCtxt,
    function: Option<FunctionId>,
    error_count: usize,
}

impl<'a> Validator<'a> {
    /// Creates a verifier that emits findings into `dcx`.
    const fn new(dcx: &'a DiagCtxt) -> Self {
        Self { dcx, function: None, error_count: 0 }
    }

    #[track_caller]
    fn emit(&mut self, message: impl fmt::Display) {
        // TODO: Use MIR debug-info spans when emitting verifier diagnostics.
        let message = fmt::from_fn(|f| {
            if let Some(function) = self.function {
                write!(f, "[fn{}] ", function.index())?;
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
    #[cfg(test)]
    fn validate_standalone_function(mut self, func: &Function) {
        self.validate_function_body(func);
    }

    fn validate_function(&mut self, module: &Module, func: &Function) {
        self.validate_function_body(func);
        self.validate_immutables(module, func);
        self.validate_calls(module, func);
        self.validate_function_phase(module, func);
    }

    fn validate_function_body(&mut self, func: &Function) {
        let errors_before = self.error_count;
        let num_values = func.num_values();
        let num_blocks = func.blocks.len();
        let num_insts = func.num_insts();

        if num_blocks == 0 {
            self.emit("function has no entry block");
            return;
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
                let inst = func.inst(inst_id);

                match (inst.result_ty, func.inst_result_value(inst_id)) {
                    (Some(_), Some(result)) if result.index() >= num_values => {
                        self.emit_at_inst(
                            format_args!(
                                "instruction result references undefined value v{} \
                                 (only {num_values} values exist)",
                                result.index()
                            ),
                            block_id,
                            inst_id,
                        );
                    }
                    (Some(_), Some(result)) => {
                        if !matches!(func.value(result), Value::Inst(def) if *def == inst_id) {
                            self.emit_at_inst(
                                format_args!(
                                    "instruction result v{} does not refer back to inst{}",
                                    result.index(),
                                    inst_id.index()
                                ),
                                block_id,
                                inst_id,
                            );
                        }
                    }
                    (Some(_), None) => {
                        self.emit_at_inst(
                            "value-producing instruction has no result value",
                            block_id,
                            inst_id,
                        );
                    }
                    (None, Some(result)) => {
                        self.emit_at_inst(
                            format_args!(
                                "instruction records result v{} but has no result type",
                                result.index()
                            ),
                            block_id,
                            inst_id,
                        );
                    }
                    (None, None) => {}
                }

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

        // ----- Entry block invariants -----
        if !func.blocks[BlockId::ENTRY].predecessors.is_empty() {
            self.emit_at_block("entry block must have no predecessors", BlockId::ENTRY);
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
        let mut def_location_of: IndexVec<ValueId, Option<(BlockId, usize)>> =
            index_vec![None; num_values];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                if let Some(result) = func.inst_result_value(inst_id) {
                    def_location_of[result] = Some((block_id, index));
                }
            }
        }
        let mut reach_cache: FxHashMap<BlockId, DenseBitSet<BlockId>> = FxHashMap::default();
        let mut reaches = |from: BlockId, to: BlockId| {
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
            let block_in_cycle = reaches(block_id, block_id);
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                match &func.inst(inst_id).kind {
                    InstKind::Phi(incoming) => {
                        for &(pred, value) in incoming {
                            // An edge from an unreachable predecessor never
                            // executes, so whatever it names is vacuous. A pass
                            // that makes a predecessor unreachable need not also
                            // rewrite every phi that still lists it.
                            if !cfg.is_reachable(pred) {
                                continue;
                            }
                            if let Some((def, _)) = def_location_of[value]
                                && def != pred
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
                            if let Some((def, def_index)) = def_location_of[operand] {
                                if def == block_id {
                                    if !block_in_cycle && def_index >= index {
                                        self.emit_at_inst(
                                            format_args!(
                                                "use of {operand:?} precedes its definition in \
                                                 this acyclic block"
                                            ),
                                            block_id,
                                            inst_id,
                                        );
                                    }
                                } else if !reaches(def, block_id) {
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
            }
            if let Some(term) = &block.terminator {
                for &operand in term.operands().iter() {
                    if let Some((def, _)) = def_location_of[operand]
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

    fn validate_immutables(&mut self, module: &Module, func: &Function) {
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            match inst.kind {
                InstKind::LoadImmutable(id) => {
                    match (module.get_immutable_type(id), inst.result_ty) {
                        (Some(expected), Some(actual)) if actual != expected => {
                            self.emit(format_args!(
                                "inst{} loads immutable {} as `{actual}`, expected `{expected}`",
                                inst_id.index(),
                                id.index(),
                            ));
                        }
                        (Some(_), None) => self.emit(format_args!(
                            "inst{} loads immutable {} without a result type",
                            inst_id.index(),
                            id.index(),
                        )),
                        (None, _) => self.emit(format_args!(
                            "inst{} loads nonexistent immutable {}",
                            inst_id.index(),
                            id.index()
                        )),
                        _ => {}
                    }
                }
                InstKind::StoreImmutable(id, value) => {
                    let Some(immutable) = module.get_immutable(id) else {
                        self.emit(format_args!(
                            "inst{} stores nonexistent immutable {}",
                            inst_id.index(),
                            id.index()
                        ));
                        continue;
                    };
                    if let Some(actual) = func.value_ty(value)
                        && actual.immutable_encoding().is_none()
                    {
                        self.emit(format_args!(
                            "inst{} stores `{actual}` value into immutable `{}` of type `{}`",
                            inst_id.index(),
                            immutable.name,
                            immutable.ty,
                        ));
                    }
                }
                InstKind::ConstructorArgsBase
                    if !func.attributes.is_constructor && func.name.symbol != kw::Constructor =>
                {
                    self.emit(format_args!(
                        "inst{} uses the constructor argument base outside a constructor",
                        inst_id.index()
                    ));
                }
                _ => {}
            }
        }
    }

    fn validate_immutable_declarations(&mut self, module: &Module) {
        for (_, immutable) in module.iter_immutables() {
            if immutable.ty.immutable_encoding().is_none() {
                self.emit(format_args!(
                    "immutable `{}` cannot use type `{}`",
                    immutable.name, immutable.ty
                ));
            }
        }
    }

    /// Validates every function in a module.
    fn validate_module(mut self, module: &Module) {
        self.validate_module_phase(module);
        self.validate_immutable_declarations(module);
        for (id, func) in module.iter_functions() {
            self.function = Some(id);
            self.validate_function(module, func);
        }
        self.function = None;
    }

    /// Checks that call targets exist and argument counts match.
    ///
    /// Only live instructions — those still present in a block — are checked. An
    /// inlined or DCE'd call leaves its `Instruction` orphaned in the arena; a
    /// later signature change (e.g. `lower-slices` expanding a slice parameter
    /// into a pointer/length pair) makes that dead call's arg count disagree
    /// with the callee even though it is never emitted. Block-based iteration
    /// mirrors how the display and every pass treat instructions.
    fn validate_calls(&mut self, module: &Module, func: &Function) {
        for inst_id in func.instructions() {
            let InstKind::InternalCall { function, args, .. } = &func.inst(inst_id).kind else {
                continue;
            };
            let Some(callee) = module.functions.get(*function) else {
                self.emit(format_args!(
                    "internal_call targets nonexistent function fn{}",
                    function.index()
                ));
                continue;
            };
            if args.len() != callee.params.len() {
                self.emit(format_args!(
                    "internal_call to `{}` passes {} argument(s), expected {}",
                    callee.name,
                    args.len(),
                    callee.params.len()
                ));
            }
        }
        for block in func.blocks.iter() {
            let Some(crate::mir::Terminator::TailCall { function, args }) = &block.terminator
            else {
                continue;
            };
            let Some(callee) = module.functions.get(*function) else {
                self.emit(format_args!(
                    "tail_call targets nonexistent function fn{}",
                    function.index()
                ));
                continue;
            };
            if args.len() != callee.params.len() {
                self.emit(format_args!(
                    "tail_call to `{}` passes {} argument(s), expected {}",
                    callee.name,
                    args.len(),
                    callee.params.len()
                ));
            }
        }
    }

    /// Checks that the module's content satisfies its declared
    /// [`MirPhase`](crate::mir::MirPhase), so
    /// the phase is a real contract rather than a label.
    fn validate_module_phase(&mut self, module: &Module) {
        // From the `dispatch` phase on, routing is materialized: a module with
        // a runtime interface must contain exactly one synthesized `entry`.
        if module.phase < crate::mir::MirPhase::Dispatch {
            return;
        }
        let dispatch_entries =
            module.functions.iter().filter(|f| f.attributes.is_dispatch_entry).count();
        if dispatch_entries > 1 {
            self.emit(format_args!(
                "module is in the `{}` phase but has multiple `entry` routing functions",
                module.phase.name()
            ));
        } else if dispatch_entries == 0
            && module.functions.iter().any(|f| {
                f.selector.is_some() || f.attributes.is_receive || f.attributes.is_fallback
            })
        {
            self.emit(format_args!(
                "module is in the `{}` phase but has no `entry` routing function",
                module.phase.name()
            ));
        }
    }

    fn validate_function_phase(&mut self, module: &Module, func: &Function) {
        // From the `abi` phase on, every bodied external (selector-bearing)
        // function is an argument-free self-decoding wrapper.
        if module.phase >= crate::mir::MirPhase::Abi
            && func.selector.is_some()
            && !func.params.is_empty()
        {
            self.emit(format_args!(
                "selector function `{}` still takes arguments in the `{}` phase \
                 (expected an argument-free ABI wrapper)",
                func.name,
                module.phase.name()
            ));
        }
        // The memory-lowered phase is a strict representation boundary: no
        // nominal object types, layouts, or semantic accesses may survive.
        if module.phase >= crate::mir::MirPhase::MemoryLowered {
            let signature_types = func
                .arg_indices()
                .map(|index| func.arg_ty(index))
                .chain(func.returns.iter().copied());
            for ty in signature_types {
                if matches!(ty, crate::mir::MirType::MemoryObject(_)) {
                    self.emit(format_args!(
                        "memory-object signature type `{ty}` survives the `{}` phase boundary",
                        module.phase.name()
                    ));
                }
            }
            let mut values = DenseBitSet::new_empty(func.num_values());
            for value in func.live_values() {
                if value.index() < func.num_values() {
                    values.insert(value);
                }
            }
            for value in values.iter() {
                if let Value::Undef(ty) = func.value(value)
                    && matches!(ty, crate::mir::MirType::MemoryObject(_))
                {
                    self.emit(format_args!(
                        "memory-object value type survives the `{}` phase boundary",
                        module.phase.name()
                    ));
                }
            }
            for (block_id, block) in func.blocks.iter_enumerated() {
                for &inst_id in &block.instructions {
                    let inst = func.inst(inst_id);
                    let semantic = matches!(
                        inst.kind,
                        InstKind::Alloc { kind: crate::mir::AllocationKind::Object(_), .. }
                            | InstKind::MemoryObjectLen(_, _)
                            | InstKind::SetMemoryObjectLen(_, _, _)
                            | InstKind::MemoryObjectData(_, _)
                            | InstKind::MemoryObjectFieldAddr { .. }
                            | InstKind::MemoryObjectElementAddr { .. }
                            | InstKind::Keccak256Bytes(_)
                    ) || inst
                        .result_ty
                        .is_some_and(|ty| matches!(ty, crate::mir::MirType::MemoryObject(_)));
                    if semantic {
                        self.emit_at_inst(
                            format_args!(
                                "memory-object instruction `{}` survives the `{}` phase boundary",
                                inst.kind.mnemonic(),
                                module.phase.name()
                            ),
                            block_id,
                            inst_id,
                        );
                    }
                }
            }
        }
        // EVM-shaped MIR is the semantic boundary consumed by the word-based
        // backend. High-level memory operations must have been expanded by
        // their named lowering passes before the module enters this phase.
        if module.phase >= crate::mir::MirPhase::EvmShaped {
            for (block_id, block) in func.blocks.iter_enumerated() {
                for &inst_id in &block.instructions {
                    let kind = &func.inst(inst_id).kind;
                    let semantic_op = match kind {
                        InstKind::MakeSlice { .. }
                        | InstKind::SlicePtr(_)
                        | InstKind::SliceLen(_) => Some("slice"),
                        InstKind::Fmp | InstKind::SetFmp(_) => Some("abstract allocation"),
                        InstKind::Alloc { .. } if !func.inst(inst_id).metadata.deferred_alloc() => {
                            Some("abstract allocation")
                        }
                        InstKind::MemoryZero(_, _) => Some("memory zero"),
                        InstKind::AbiEncode { .. } => Some("ABI encoding"),
                        InstKind::StorageToMemory { .. }
                        | InstKind::MemoryToStorage { .. }
                        | InstKind::ClearStorage { .. } => Some("aggregate"),
                        InstKind::StoreImmutable(..) => Some("immutable assignment"),
                        _ => None,
                    };
                    if let Some(semantic_op) = semantic_op {
                        self.emit_at_inst(
                            format_args!(
                                "{semantic_op} instruction `{}` survives the `{}` phase boundary",
                                kind.mnemonic(),
                                module.phase.name()
                            ),
                            block_id,
                            inst_id,
                        );
                    }
                }
            }
        }
    }
}

pub(crate) fn validate(dcx: &DiagCtxt, module: &Module) {
    Validator::new(dcx).validate_module(module);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, FunctionBuilder, MirType, Terminator};
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
    fn multiple_dispatch_entries_are_caught_without_runtime_attributes() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            module.phase = crate::mir::MirPhase::EvmShaped;
            for _ in 0..2 {
                let mut func = make_func();
                func.attributes.is_dispatch_entry = true;
                FunctionBuilder::new(&mut func).stop();
                module.functions.push(func);
            }
            Validator::new(&sess.dcx).validate_module(&module);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: module is in the `evm-shaped` phase but has multiple `entry` routing functions


"#]]
            );
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
            Validator::new(&sess.dcx).validate_standalone_function(&func);
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
            func.blocks[BlockId::ENTRY].terminator = Some(Terminator::Jump(bad_block));
            Validator::new(&sess.dcx).validate_standalone_function(&func);
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
            Validator::new(&sess.dcx).validate_standalone_function(&func);
            assert!(sess.dcx.has_errors().is_ok());
            // Drop the back-link.
            func.blocks[target].predecessors.clear();
            Validator::new(&sess.dcx).validate_standalone_function(&func);
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
    fn unexpected_stored_predecessor_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            let target;
            {
                let mut builder = FunctionBuilder::new(&mut func);
                target = builder.create_block();
                builder.stop();
                builder.switch_to_block(target);
                builder.stop();
            }
            func.blocks[target].predecessors.push(BlockId::ENTRY);
            Validator::new(&sess.dcx).validate_standalone_function(&func);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [bb1] stored predecessor bb0 does not branch to bb1


"#]]
            );
        });
    }

    #[test]
    fn function_without_entry_block_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            func.blocks.clear();
            Validator::new(&sess.dcx).validate_standalone_function(&func);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: function has no entry block


"#]]
            );
        });
    }
}
