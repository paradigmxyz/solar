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
//! 10. **SSA dominance**: every instruction result dominates each reachable use (phi inputs: their
//!     incoming predecessor). Within a block, definitions precede ordinary uses, including in
//!     loops; loop-carried values must use explicit phis.
//! 11. **Call consistency**: internal and tail-call targets exist and their argument counts match
//!     the callee.
//! 12. **Immutable consistency**: immutable declarations and stores use supported representations,
//!     and loads use the declared type.
//! 13. **Program data consistency**: data references name allocated entries at valid offsets.
//! 14. **Return contracts**: return counts match signatures, including through tail-call chains;
//!     signatures cannot contain void values.
//! 15. **Representation boundaries**: SSA aggregates and semantic memory types/operations cannot
//!     survive their lowering boundaries. Object operations agree with nominal reference kinds.
//!
//! # Usage
//!
//! ```ignore
//! solar_codegen::mir::validate(dcx, &module);
//! ```

use crate::mir::{
    AddressCallKind, BlockId, Builtin, Callee, Function, FunctionId, InstId, InstKind,
    MemoryObjectKind, MemoryObjectLayout, MirPhase, MirType, Module, RequireKind, ResultKind,
    SliceLocation, StructId, Terminator, TypeSize, Value, ValueId, analysis::CfgInfo,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
};
use solar_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    kw,
};
use std::fmt;

/// Stateful MIR verifier.
struct Validator<'a> {
    dcx: &'a DiagCtxt,
    function: Option<FunctionId>,
    error_count: usize,
    error: Option<ErrorGuaranteed>,
    returning_functions: Option<DenseBitSet<FunctionId>>,
    return_field_counts: IndexVec<StructId, Option<usize>>,
}

impl<'a> Validator<'a> {
    /// Creates a verifier that emits findings into `dcx`.
    fn new(dcx: &'a DiagCtxt) -> Self {
        Self {
            dcx,
            function: None,
            error_count: 0,
            error: None,
            returning_functions: None,
            return_field_counts: IndexVec::new(),
        }
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
        self.error = Some(self.dcx.err(message.to_string()).emit());
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
        if self.validate_references(func) {
            self.validate_function_body(None, func);
        }
    }

    fn validate_function(&mut self, module: &Module, func: &Function, phase: MirPhase) {
        if !self.validate_references(func) {
            return;
        }
        let errors_before = self.error_count;
        for (index, ty) in func.params.iter().enumerate() {
            if *ty == MirType::Void {
                self.emit(format_args!("parameter {index} cannot have type `void`"));
            }
        }
        let num_args = func.arg_indices().count();
        for (index, &ty) in func.params.iter_enumerated() {
            if index.index() >= num_args || func.arg_ty(index) != ty {
                self.emit("argument type does not match its parameter declaration");
            }
        }
        if func.return_components().contains(&MirType::Void) {
            self.emit("return signature cannot contain `void`; use an empty return list");
        }
        self.validate_function_body(Some(module), func);
        self.validate_module_references(module, func);
        self.validate_calls(module, func);
        if self.error_count == errors_before {
            self.validate_value_types(module, func);
            self.validate_memory_object_types(func);
        }
        self.validate_function_phase(phase, func);
    }

    /// Checks arena references before any operation can query operand types.
    fn validate_references(&mut self, func: &Function) -> bool {
        let errors_before = self.error_count;
        let mut seen = DenseBitSet::new_empty(func.num_insts());
        let num_args = func.arg_indices().count();
        for (block, body) in func.blocks.iter_enumerated() {
            for &id in &body.instructions {
                if id.index() >= func.num_insts() {
                    self.emit_at_block(
                        format_args!("block contains nonexistent inst{}", id.index()),
                        block,
                    );
                    continue;
                }
                if !seen.insert(id) {
                    self.emit_at_inst("instruction appears more than once", block, id);
                }
                let inst = func.inst(id);
                for value in inst.kind.operands().into_iter().chain(inst.result()) {
                    self.validate_value_reference(func, value, num_args, block);
                }
            }
            if let Some(term) = &body.terminator {
                term.for_each_operand(|value| {
                    self.validate_value_reference(func, value, num_args, block);
                });
            }
        }
        self.error_count == errors_before
    }

    fn validate_value_reference(
        &mut self,
        func: &Function,
        value: ValueId,
        num_args: usize,
        block: BlockId,
    ) {
        if value.index() >= func.num_values() {
            self.emit_at_block(
                format_args!("reference to undefined value v{}", value.index()),
                block,
            );
            return;
        }
        match func.value(value) {
            Value::Inst(id) if id.index() >= func.num_insts() => self.emit_at_block(
                format_args!("value v{} references nonexistent inst{}", value.index(), id.index()),
                block,
            ),
            Value::Arg(index) if index.index() >= num_args => self.emit_at_block(
                format_args!(
                    "value v{} references nonexistent argument {}",
                    value.index(),
                    index.index()
                ),
                block,
            ),
            _ => {}
        }
    }

    /// Checks terminators and both directions of the maintained predecessor relation.
    fn validate_cfg(&mut self, func: &Function) {
        let num_blocks = func.blocks.len();
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

            self.validate_terminator_types(func, block_id, term);

            // Check successor blocks exist and back-link.
            term.for_each_successor(|succ| {
                if succ.index() >= num_blocks {
                    self.emit_at_block(
                        format_args!("terminator references nonexistent block bb{}", succ.index()),
                        block_id,
                    );
                } else if !func.blocks[succ].predecessors.contains(&block_id) {
                    self.emit_at_block(
                        format_args!(
                            "successor bb{} does not list bb{} as a predecessor",
                            succ.index(),
                            block_id.index()
                        ),
                        block_id,
                    );
                }
            });

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
                if !pred_term.has_successor(block_id) {
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
        }

        // ----- Entry block invariants -----
        if !func.blocks[BlockId::ENTRY].predecessors.is_empty() {
            self.emit_at_block("entry block must have no predecessors", BlockId::ENTRY);
        }
    }

    fn validate_terminator_types(&mut self, func: &Function, block: BlockId, term: &Terminator) {
        match term {
            Terminator::Branch { condition, .. } => {
                if func.value_ty(*condition) != Some(MirType::I1) {
                    self.emit_at_block(
                        "branch condition must have type `i1`; compare words with zero",
                        block,
                    );
                }
            }
            Terminator::Switch { value, cases, .. } => {
                let ty = func.value_ty(*value);
                if !matches!(ty, Some(MirType::Int(_))) {
                    self.emit_at_block("switch selector must have an integer type", block);
                }
                if cases.iter().any(|&(value, _)| func.value_ty(value) != ty) {
                    self.emit_at_block("switch cases must have the selector type", block);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                if [*offset, *size]
                    .into_iter()
                    .any(|value| func.value_ty(value) != Some(MirType::I256))
                {
                    self.emit_at_block(
                        "raw return and revert operands must have type `i256`; use an explicit cast",
                        block,
                    );
                }
            }
            Terminator::SelfDestruct { recipient } => {
                if func.value_ty(*recipient) != Some(MirType::I256) {
                    self.emit_at_block(
                        "selfdestruct recipient must have type `i256`; use an explicit cast",
                        block,
                    );
                }
            }
            // Function signatures are checked with module context in validate_value_types.
            Terminator::Return { .. } | Terminator::TailCall { .. } => {}
            Terminator::Jump(_)
            | Terminator::RevertReturndata
            | Terminator::Stop
            | Terminator::Invalid => {}
        }
    }

    fn validate_function_body(&mut self, module: Option<&Module>, func: &Function) {
        let errors_before = self.error_count;
        let num_values = func.num_values();
        let num_blocks = func.blocks.len();
        let num_insts = func.num_insts();

        if num_blocks == 0 {
            self.emit("function has no entry block");
            return;
        }

        self.validate_cfg(func);
        for (block_id, block) in func.blocks.iter_enumerated() {
            let Some(term) = &block.terminator else { continue };

            // Check terminator operands are in range.
            term.for_each_operand(|op| {
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
            });

            // ----- Walk instructions in this block -----
            let block_preds = &block.predecessors;
            for &inst_id in &block.instructions {
                if inst_id.index() >= num_insts {
                    self.emit_at_block(
                        format_args!("block contains nonexistent inst{}", inst_id.index()),
                        block_id,
                    );
                    continue;
                }
                let inst = func.inst(inst_id);

                if !inst.kind.scalar_types_match(func, inst.result_ty) {
                    self.emit_at_inst(
                        format_args!(
                            "`{}` has incompatible scalar types; use an explicit cast",
                            inst.kind.mnemonic()
                        ),
                        block_id,
                        inst_id,
                    );
                }

                let result_kind = inst.kind.op_def().result;
                if result_kind != ResultKind::Custom
                    && result_kind.produces_value() != inst.result_ty.is_some()
                {
                    self.emit_at_inst(
                        format_args!(
                            "`{}` {} a value but {} a result type",
                            inst.kind.mnemonic(),
                            if result_kind.produces_value() {
                                "produces"
                            } else {
                                "does not produce"
                            },
                            if inst.result_ty.is_some() { "has" } else { "has no" },
                        ),
                        block_id,
                        inst_id,
                    );
                }

                if let Some(ty) = inst.result_ty
                    && !inst.kind.admits_result_type(ty)
                {
                    self.emit_at_inst(
                        format_args!(
                            "`{}` produces {:?} but its result type is `{ty}`",
                            inst.kind.mnemonic(),
                            result_kind,
                        ),
                        block_id,
                        inst_id,
                    );
                }

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

                if let Some(module) = module {
                    self.validate_data_reference(module, func, block_id, inst_id);
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
                    for pred in block_preds {
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

        // ----- SSA dominance -----
        // Structural errors must be reported before constructing the CFG.
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
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) {
                continue;
            }
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
                            if let Some((def, _)) =
                                self.live_definition(func, &def_location_of, value, block_id)
                                && def != pred
                                && !cfg.dominators().dominates(def, pred)
                            {
                                self.emit_at_inst(
                                    format_args!(
                                        "phi input {value:?} from bb{} is not dominated by \
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
                            if let Some((def, def_index)) =
                                self.live_definition(func, &def_location_of, operand, block_id)
                            {
                                if def == block_id {
                                    if def_index >= index {
                                        self.emit_at_inst(
                                            format_args!(
                                                "use of {operand:?} precedes its definition in \
                                                 this block"
                                            ),
                                            block_id,
                                            inst_id,
                                        );
                                    }
                                } else if !cfg.dominators().dominates(def, block_id) {
                                    self.emit_at_inst(
                                        format_args!(
                                            "use of {operand:?} is not dominated by its \
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
                term.for_each_operand(|operand| {
                    if let Some((def, _)) =
                        self.live_definition(func, &def_location_of, operand, block_id)
                        && def != block_id
                        && !cfg.dominators().dominates(def, block_id)
                    {
                        self.emit_at_block(
                            format_args!(
                                "terminator use of {operand:?} is not dominated by its \
                         definition in bb{}",
                                def.index()
                            ),
                            block_id,
                        );
                    }
                });
            }
        }
    }

    /// Returns an instruction's live definition, rejecting orphaned instruction values.
    fn live_definition(
        &mut self,
        func: &Function,
        locations: &IndexVec<ValueId, Option<(BlockId, usize)>>,
        value: ValueId,
        block: BlockId,
    ) -> Option<(BlockId, usize)> {
        let location = locations[value];
        if location.is_none() && matches!(func.value(value), Value::Inst(_)) {
            self.emit_at_block(format_args!("use of {value:?} has no live definition"), block);
        }
        location
    }

    fn validate_module_references(&mut self, module: &Module, func: &Function) {
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            match inst.kind {
                InstKind::LibraryAddress(id) if module.libraries.get(id).is_none() => {
                    self.emit(format_args!(
                        "inst{} references nonexistent library {}",
                        inst_id.index(),
                        id.index()
                    ));
                }
                InstKind::LoadImmutable(id) => {
                    match (module.get_immutable_type(id), inst.result_ty) {
                        (Some(expected), Some(actual))
                            if actual != expected.mir_type()
                                && !(actual == MirType::I256
                                    && expected.mir_type().integer_bits().is_some()) =>
                        {
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
                    if func.value_ty(value) != Some(immutable.ty.mir_type()) {
                        let actual = func.value_ty(value).unwrap_or(MirType::Void);
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
        self.validate_module_at_phase(module, module.phase());
    }

    fn validate_module_at_phase(&mut self, module: &Module, phase: MirPhase) {
        self.returning_functions = Some(module.returning_functions());
        self.prepare_return_abi_validation(module);
        for (id, ty) in module.struct_types.iter_enumerated() {
            for field in &ty.fields {
                self.validate_integer_type(*field);
                if *field == MirType::Void
                    || matches!(field, MirType::Struct(nested) if *nested >= id)
                {
                    self.emit(format_args!(
                        "invalid field type `{field}` in `struct{}`",
                        id.index()
                    ));
                }
            }
        }
        self.validate_module_phase(module, phase);
        self.validate_immutable_declarations(module);
        for (id, func) in module.iter_functions() {
            self.function = Some(id);
            self.validate_function(module, func, phase);
        }
        self.function = None;
    }

    fn prepare_return_abi_validation(&mut self, module: &Module) {
        if !module.functions.iter().any(|func| func.return_abi().is_some()) {
            return;
        }
        for (id, structure) in module.struct_types.iter_enumerated() {
            let count = structure.fields.iter().try_fold(0usize, |count, &field| {
                let fields = match field {
                    MirType::Struct(nested) if nested < id => self.return_field_counts[nested]?,
                    MirType::Struct(_) | MirType::Void => return None,
                    _ => 1,
                };
                count.checked_add(fields)
            });
            self.return_field_counts.push(count);
        }
    }

    fn validate_return_abi(&mut self, module: &Module, func: &Function) {
        if let Some(components) = func.return_abi()
            && !return_abi_matches(
                module,
                func.return_type(),
                components,
                &self.return_field_counts,
            )
        {
            self.emit("internal return ABI does not match the function's result type");
        }
    }

    fn validate_integer_type(&mut self, ty: MirType) {
        if let Some(bits) = ty.integer_bits()
            && !MirType::valid_integer_width(bits)
        {
            self.emit(format_args!(
                "unsupported integer type `{ty}`; expected i1 or a byte width from i8 through i256"
            ));
        }
    }

    /// Checks constant widths and aggregate operands against their declared types.
    fn validate_value_types(&mut self, module: &Module, func: &Function) {
        self.validate_return_abi(module, func);
        for ty in func
            .params
            .iter()
            .copied()
            .chain([func.return_type()])
            .chain(func.return_components().iter().copied())
        {
            self.validate_integer_type(ty);
        }
        let mut checked = DenseBitSet::new_empty(func.num_values());
        for value in func.live_values() {
            if !checked.insert(value) {
                continue;
            }
            match func.value_ty(value) {
                None | Some(MirType::Void) => {
                    self.emit(format_args!("live value v{} has no value type", value.index()));
                }
                Some(ty) => self.validate_integer_type(ty),
            }
            if let Value::Immediate(crate::mir::Immediate::Pointer(_, ty)) = func.value(value)
                && !ty.is_pointer()
            {
                self.emit("pointer constant must have a pointer type");
            }
            if let Value::Immediate(immediate) = func.value(value)
                && let MirType::Int(bits) = immediate.ty()
                && bits.get() < 256
                && immediate.as_u256().is_some_and(|word| word.bit_len() > bits.get() as usize)
            {
                self.emit(format_args!(
                    "constant v{} does not fit its type `{}`",
                    value.index(),
                    immediate.ty()
                ));
            }
        }
        for ty in func
            .arg_indices()
            .map(|index| func.arg_ty(index))
            .chain(std::iter::once(func.return_type()))
            .chain(func.return_components().iter().copied())
        {
            if let MirType::Struct(id) = ty
                && module.struct_types.get(id).is_none()
            {
                self.emit(format_args!("undefined struct type `struct{}`", id.index()));
            }
        }
        for (block, body) in func.blocks.iter_enumerated() {
            for &id in &body.instructions {
                let inst = func.inst(id);
                let operands = inst.kind.operands();
                let mut has_struct_value = false;
                for ty in
                    operands.iter().filter_map(|&value| func.value_ty(value)).chain(inst.result_ty)
                {
                    if let MirType::Struct(ty) = ty {
                        has_struct_value = true;
                        if module.struct_types.get(ty).is_none() {
                            self.emit(format_args!("undefined struct type `struct{}`", ty.index()));
                        }
                    }
                }
                match &inst.kind {
                    InstKind::Phi(incoming) => {
                        for &(_, value) in incoming {
                            self.check_value_type(func.value_ty(value), inst.result_ty, block, id);
                        }
                    }
                    InstKind::Select(condition, a, b) => {
                        self.check_value_type(
                            func.value_ty(*condition),
                            Some(MirType::I1),
                            block,
                            id,
                        );
                        for value in [*a, *b] {
                            self.check_value_type(func.value_ty(value), inst.result_ty, block, id);
                        }
                    }
                    InstKind::ICall { function: Callee::Function(function), args, .. } => {
                        if let Some(callee) = module.functions.get(*function) {
                            for (&value, &ty) in args.iter().zip(&callee.params) {
                                self.check_value_type(func.value_ty(value), Some(ty), block, id);
                            }
                            if inst.result_ty.is_some() {
                                self.check_value_type(
                                    inst.result_ty,
                                    callee.return_components().first().copied(),
                                    block,
                                    id,
                                );
                            }
                        }
                    }
                    InstKind::AbiEncode { args, layout, .. }
                    | InstKind::ICall {
                        function:
                            Callee::Builtin(Builtin::Require(RequireKind::CustomError(layout))),
                        args,
                    } => {
                        let values = if matches!(inst.kind, InstKind::ICall { .. }) {
                            &args[2..]
                        } else {
                            args.as_ref()
                        };
                        if values.iter().zip(&layout.types).any(|(&value, ty)| {
                            func.value_ty(value).is_none_or(|actual| !ty.accepts_input_type(actual))
                        }) {
                            self.emit_at_inst(
                                "ABI input location does not support its layout",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::AbiDecode { data, layout } => {
                        self.check_value_type(
                            func.value_ty(*data),
                            Some(MirType::MemoryObject(MemoryObjectKind::Bytes)),
                            block,
                            id,
                        );
                        let valid = match inst.result_ty {
                            Some(MirType::Struct(ty)) => {
                                module.struct_types.get(ty).is_some_and(|ty| {
                                    ty.fields.len() == layout.types.len()
                                        && ty
                                            .fields
                                            .iter()
                                            .zip(&layout.types)
                                            .all(|(&field, abi)| field == abi.mir_type())
                                })
                            }
                            Some(ty) => layout.types.len() == 1 && ty == layout.types[0].mir_type(),
                            None => false,
                        };
                        if !valid {
                            self.emit_at_inst(
                                "ABI decode result does not match its layout",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::InsertValue { .. } | InstKind::ExtractValue { .. } => {}
                    _ if has_struct_value => {
                        if matches!(inst.result_ty, Some(MirType::Struct(_))) {
                            self.emit_at_inst(
                                "instruction cannot produce a struct value",
                                block,
                                id,
                            );
                        }
                        for &value in &operands {
                            if matches!(func.value_ty(value), Some(MirType::Struct(_))) {
                                self.emit_at_inst(
                                    "instruction cannot consume a struct value",
                                    block,
                                    id,
                                );
                            }
                        }
                    }
                    _ => {}
                }
                let (ty, aggregate, index, inserted) = match inst.kind {
                    InstKind::InsertValue { ty, aggregate, index, value } => {
                        (ty, aggregate, index, Some(value))
                    }
                    InstKind::ExtractValue { ty, aggregate, index } => (ty, aggregate, index, None),
                    _ => continue,
                };
                let Some(fields) = module.struct_types.get(ty) else {
                    self.emit_at_inst(
                        format_args!("undefined struct type `struct{}`", ty.index()),
                        block,
                        id,
                    );
                    continue;
                };
                let Some(&field) = fields.fields.get(index as usize) else {
                    self.emit_at_inst("struct field index is out of bounds", block, id);
                    continue;
                };
                if func.value_ty(aggregate) != Some(MirType::Struct(ty)) {
                    self.emit_at_inst(
                        "aggregate operand does not match its declared struct type",
                        block,
                        id,
                    );
                }
                let result = if let Some(value) = inserted {
                    if func.value_ty(value) != Some(field) {
                        self.emit_at_inst(
                            format_args!("inserted value must have type `{field}`"),
                            block,
                            id,
                        );
                    }
                    MirType::Struct(ty)
                } else {
                    field
                };
                if inst.result_ty != Some(result) {
                    self.emit_at_inst(
                        format_args!("struct instruction result must have type `{result}`"),
                        block,
                        id,
                    );
                }
            }
            if let Some(term) = &body.terminator {
                term.for_each_operand(|value| {
                    if let Some(MirType::Struct(ty)) = func.value_ty(value)
                        && module.struct_types.get(ty).is_none()
                    {
                        self.emit(format_args!("undefined struct type `struct{}`", ty.index()));
                    }
                });
            }
            match &body.terminator {
                Some(crate::mir::Terminator::Return { values }) => {
                    if values.len() != func.return_components().len() {
                        self.emit_at_block(
                            format_args!(
                                "return has {} value(s), signature expects {}",
                                values.len(),
                                func.return_components().len()
                            ),
                            block,
                        );
                    } else if values
                        .iter()
                        .zip(func.return_components())
                        .any(|(&value, &ty)| func.value_ty(value) != Some(ty))
                    {
                        self.emit_at_block("return values do not match the signature", block);
                    }
                }
                Some(crate::mir::Terminator::TailCall { function, args }) => {
                    if let Some(callee) = module.functions.get(*function) {
                        if args.iter().zip(&callee.params).any(|(&value, &ty)| {
                            let actual = func.value_ty(value);
                            actual != Some(ty)
                        }) {
                            self.emit_at_block(
                                "tail-call arguments do not match the signature",
                                block,
                            );
                        }
                        if func.selector.is_none()
                            && func.return_components() != callee.return_components()
                            && self
                                .returning_functions
                                .as_ref()
                                .is_some_and(|returning| returning.contains(*function))
                        {
                            self.emit_at_block(
                                "tail-call results do not match the signature",
                                block,
                            );
                        }
                    }
                }
                Some(term)
                    if term
                        .operands()
                        .iter()
                        .any(|&value| matches!(func.value_ty(value), Some(MirType::Struct(_)))) =>
                {
                    self.emit_at_block("terminator cannot consume a struct value", block);
                }
                _ => {}
            }
        }
    }

    /// Checks semantic result types and layout metadata after operand contracts have passed.
    fn validate_memory_object_types(&mut self, func: &Function) {
        for (block, body) in func.blocks.iter_enumerated() {
            for &id in &body.instructions {
                let inst = func.inst(id);
                let valid = match &inst.kind {
                    InstKind::ICall { function: Callee::Builtin(builtin), args } => {
                        let result = match builtin {
                            Builtin::Require(_) | Builtin::Check { .. } | Builtin::Transfer => None,
                            Builtin::ReturndataBytes | Builtin::Concat(_) => {
                                Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                            }
                            Builtin::CheckedAddMod
                            | Builtin::CheckedMulMod
                            | Builtin::Sha256
                            | Builtin::Ripemd160
                            | Builtin::Erc7201
                            | Builtin::EcRecover
                            | Builtin::Send => Some(MirType::I256),
                        };
                        let metadata_valid = match builtin {
                            Builtin::Require(RequireKind::ShortString) => args
                                .get(1)
                                .and_then(|&value| func.value_u64(value))
                                .is_some_and(|length| (1..=32).contains(&length)),
                            Builtin::Concat(types) => types.iter().all(|ty| {
                                matches!(
                                    ty,
                                    crate::mir::ValueLayout::MemoryObject(MemoryObjectKind::Bytes)
                                        | crate::mir::ValueLayout::FixedBytes(_)
                                )
                            }),
                            _ => true,
                        };
                        metadata_valid
                            && inst.result_ty == result
                            && builtin.fixed_arity().is_none_or(|count| count == args.len())
                    }
                    InstKind::AbiEncodePacked { parts, hash } => {
                        let valid_parts = parts.iter().all(|part| match part {
                            crate::mir::PackedPart::Literal(_)
                            | crate::mir::PackedPart::Bytes(_) => true,
                            crate::mir::PackedPart::Scalar { ty, .. } => {
                                matches!(
                                    ty,
                                    crate::mir::ValueLayout::UInt(_)
                                        | crate::mir::ValueLayout::Int(_)
                                        | crate::mir::ValueLayout::FixedBytes(_)
                                        | crate::mir::ValueLayout::Address
                                        | crate::mir::ValueLayout::Bool
                                        | crate::mir::ValueLayout::Function
                                ) && ty.type_size().is_some_and(|size| size.bytes() != 0)
                            }
                            crate::mir::PackedPart::Array { element, source, .. } => {
                                !hash
                                    && crate::mir::packed_element_bytes(element).is_some()
                                    && match source {
                                        crate::mir::PackedArraySource::Memory { layout } => {
                                            matches!(
                                                layout,
                                                MemoryObjectLayout::DynamicArray { .. }
                                                    | MemoryObjectLayout::FixedArray { .. }
                                            )
                                        }
                                        crate::mir::PackedArraySource::Slice(location) => matches!(
                                            location,
                                            SliceLocation::Memory | SliceLocation::Calldata
                                        ),
                                    }
                            }
                        });
                        valid_parts
                            && inst.result_ty
                                == Some(if *hash {
                                    MirType::I256
                                } else {
                                    MirType::MemoryObject(MemoryObjectKind::Bytes)
                                })
                    }
                    InstKind::CheckedBinary { arithmetic, .. } => {
                        let (crate::mir::ArithmeticKind::Unsigned(bits)
                        | crate::mir::ArithmeticKind::Signed(bits)) = arithmetic;
                        (8..=256).contains(bits) && bits % 8 == 0
                    }
                    InstKind::StorageArrayLoad { element, enum_variants, .. } => {
                        matches!(
                            element,
                            crate::mir::ValueLayout::UInt(_)
                                | crate::mir::ValueLayout::Int(_)
                                | crate::mir::ValueLayout::FixedBytes(_)
                                | crate::mir::ValueLayout::MemoryObject(MemoryObjectKind::Bytes)
                        ) && enum_variants.is_none_or(|variants| {
                            (1..=256).contains(&variants)
                                && *element
                                    == crate::mir::ValueLayout::UInt(TypeSize::new_int_bits(8))
                        }) && inst.result_ty
                            == Some(MirType::MemoryObject(MemoryObjectKind::DynamicArray))
                    }
                    InstKind::StorageBytesLoad(_) => {
                        inst.result_ty == Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                    }
                    InstKind::AddressCall { kind, value, .. } => {
                        *kind == AddressCallKind::Call || value.is_none()
                    }
                    InstKind::MemoryObjectCopyFromSlice { source, .. }
                    | InstKind::MemoryObjectCopyFromSliceAt { source, .. } => {
                        matches!(func.value_ty(*source), Some(MirType::Slice(_)))
                    }
                    _ => true,
                };
                if !valid {
                    self.emit_at_inst(
                        "instruction result or layout does not match its type contract",
                        block,
                        id,
                    );
                }
            }
        }
    }

    fn check_value_type(
        &mut self,
        actual: Option<MirType>,
        expected: Option<MirType>,
        block: BlockId,
        inst: InstId,
    ) {
        if actual != expected {
            let actual = actual.unwrap_or(MirType::Void);
            let expected = expected.unwrap_or(MirType::Void);
            self.emit_at_inst(
                format_args!(
                    "value has type `{actual}`, expected `{expected}`; use an explicit cast"
                ),
                block,
                inst,
            );
        }
    }

    fn validate_data_reference(
        &mut self,
        module: &Module,
        func: &Function,
        block_id: BlockId,
        inst_id: InstId,
    ) {
        let InstKind::DataCopy(data, _, size) = &func.inst(inst_id).kind else { return };
        let Some(bytes) = module.get_data(data.id) else {
            self.emit_at_inst(
                format_args!("data_copy references nonexistent data{}", data.id.index()),
                block_id,
                inst_id,
            );
            return;
        };
        let Some(size) = func.value_u256(*size) else {
            self.emit_at_inst("data_copy size must be an immediate", block_id, inst_id);
            return;
        };
        let end = U256::from(data.offset).checked_add(size);
        if end.is_none_or(|end| end > U256::from(bytes.len())) {
            self.emit_at_inst(
                format_args!(
                    "data_copy range {}..{} exceeds data size {}",
                    data.offset,
                    end.map_or_else(|| "overflow".into(), |end| end.to_string()),
                    bytes.len()
                ),
                block_id,
                inst_id,
            );
        }
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
            let InstKind::ICall { function: Callee::Function(function), args } =
                &func.inst(inst_id).kind
            else {
                continue;
            };
            let Some(callee) = module.functions.get(*function) else {
                self.emit(format_args!(
                    "icall targets nonexistent function fn{}",
                    function.index()
                ));
                continue;
            };
            if args.len() != callee.params.len() {
                self.emit(format_args!(
                    "icall to `{}` passes {} argument(s), expected {}",
                    callee.name,
                    args.len(),
                    callee.params.len()
                ));
            }
            if func.inst(inst_id).result_ty.is_some() && callee.return_components().is_empty() {
                self.emit(format_args!(
                    "icall to `{}` produces a value but the callee returns no values",
                    callee.name,
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
            // A tail call delivers the callee's results as the caller's own, so
            // the two signatures must agree; dead-result elimination clearing
            // only one side would leave callers adopting a phantom value. A
            // callee that never returns (an outlined revert stub) delivers
            // nothing, and an external caller's MIR signature does not model
            // its ABI returns, so both are exempt.
            if func.selector.is_none()
                && func.return_components().len() != callee.return_components().len()
                && self
                    .returning_functions
                    .as_ref()
                    .is_some_and(|returning| returning.contains(*function))
            {
                self.emit(format_args!(
                    "tail_call to `{}` returns {} value(s), caller signature expects {}",
                    callee.name,
                    callee.return_components().len(),
                    func.return_components().len()
                ));
            }
        }
    }

    /// Checks that the module's content satisfies its declared
    /// [`MirPhase`], so
    /// the phase is a real contract rather than a label.
    fn validate_module_phase(&mut self, module: &Module, phase: MirPhase) {
        // Lowered MIR has explicit routing: a module with
        // a runtime interface must contain exactly one synthesized `entry`.
        if phase < MirPhase::Lowered {
            return;
        }
        let dispatch_entry = module.dispatch_entry();
        if dispatch_entry.is_some_and(|entry| module.functions.get(entry).is_none()) {
            self.emit(format_args!(
                "module is in the `{}` phase but has an invalid `entry` routing function",
                phase.name()
            ));
        } else if dispatch_entry.is_none()
            && module.functions.iter().any(|f| {
                f.selector.is_some() || f.attributes.is_receive || f.attributes.is_fallback
            })
        {
            self.emit(format_args!(
                "module is in the `{}` phase but has no `entry` routing function",
                phase.name()
            ));
        }
    }

    fn validate_function_phase(&mut self, phase: MirPhase, func: &Function) {
        if phase == MirPhase::Lowered && func.is_external_entry() && !func.attributes.is_abi_wrapper
        {
            self.emit("external entry has no explicit ABI implementation");
        }
        if func.attributes.is_abi_wrapper
            && (func.abi_params.is_some()
                || func.abi_returns.is_some()
                || func.abi_return_params.is_some())
        {
            self.emit("ABI wrapper retains an implicit ABI layout");
        }
        // A self-decoding runtime wrapper has no callable MIR parameters.
        if (phase == MirPhase::Lowered || func.attributes.is_abi_wrapper)
            && func.selector.is_some()
            && !func.params.is_empty()
        {
            self.emit(format_args!(
                "selector function `{}` still takes arguments in the `{}` phase \
                 (expected an argument-free ABI wrapper)",
                func.name,
                phase.name()
            ));
        }
        if phase == MirPhase::Lowered {
            let types = func
                .arg_indices()
                .map(|index| func.arg_ty(index))
                .chain(func.return_components().iter().copied())
                .chain(func.live_values().filter_map(|value| func.value_ty(value)))
                .chain(func.instructions().filter_map(|id| func.inst(id).result_ty));
            if let Some(ty) = types
                .into_iter()
                .find(|ty| !matches!(*ty, MirType::I1 | MirType::I256 | MirType::MemPtr))
            {
                self.emit(format_args!(
                    "non-word type `{ty}` survives the `lowered` phase boundary"
                ));
            }
            for (block_id, block) in func.blocks.iter_enumerated() {
                if matches!(block.terminator, Some(crate::mir::Terminator::RevertReturndata)) {
                    self.emit_at_block(
                        "returndata bubbling survives the `lowered` phase boundary",
                        block_id,
                    );
                }
                for &inst_id in &block.instructions {
                    let kind = &func.inst(inst_id).kind;
                    if let InstKind::Alloc { size, .. } = *kind
                        && func.inst(inst_id).metadata.deferred_alloc()
                        && func.value_u64(size).is_none()
                    {
                        self.emit_at_inst(
                            "deferred allocation requires a constant size",
                            block_id,
                            inst_id,
                        );
                    }
                    let semantic_op = func
                        .inst(inst_id)
                        .unlowered_reason(func)
                        .or_else(|| kind.phase_violation(phase, &func.inst(inst_id).metadata));
                    if let Some(semantic_op) = semantic_op {
                        self.emit_at_inst(
                            format_args!(
                                "{semantic_op} instruction `{}` survives the `{}` phase boundary",
                                kind.mnemonic(),
                                phase.name()
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

/// Checks all SSA, type, and representation invariants against the requested phase.
pub(crate) fn validate_phase(
    dcx: &DiagCtxt,
    module: &Module,
    phase: MirPhase,
) -> solar_interface::Result<()> {
    let mut validator = Validator::new(dcx);
    validator.validate_module_at_phase(module, phase);
    validator.error.map_or(Ok(()), Err)
}

/// Matches result components without recursive traversal or expanding repeated empty structs.
fn return_abi_matches(
    module: &Module,
    ty: MirType,
    mut components: &[MirType],
    field_counts: &IndexVec<StructId, Option<usize>>,
) -> bool {
    let pointer_only = matches!(ty, MirType::Slice(_));
    let mut pending = SmallVec::<[MirType; 4]>::from_slice(&[ty]);
    while let Some(ty) = pending.pop() {
        match ty {
            MirType::Void => {
                if !components.is_empty() {
                    return false;
                }
            }
            MirType::Struct(id) => {
                let Some(&Some(count)) = field_counts.get(id) else { return false };
                if count > components.len() {
                    return false;
                }
                if count != 0 {
                    pending.extend(module.struct_types[id].fields.iter().rev().copied());
                }
            }
            _ => {
                let Some((&actual, rest)) = components.split_first() else { return false };
                components = rest;
                if actual == ty {
                    continue;
                }
                match ty {
                    MirType::MemoryObject(_) if actual == MirType::I256 => {}
                    MirType::Slice(location) => {
                        let pointer = match location {
                            SliceLocation::Memory => MirType::I256,
                            SliceLocation::Calldata => MirType::I256,
                            SliceLocation::Returndata => MirType::I256,
                        };
                        if actual != pointer {
                            return false;
                        }
                        if pointer_only && components.is_empty() {
                            continue;
                        }
                        let Some((&length, rest)) = components.split_first() else { return false };
                        if length != MirType::I256 {
                            return false;
                        }
                        components = rest;
                    }
                    _ => return false,
                }
            }
        }
    }
    components.is_empty()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{DataId, DataRef, Function, FunctionBuilder, Immediate, MirType, Terminator};
    use alloy_primitives::U256;
    use snapbox::{assert_data_eq, str};
    use solar_interface::{ColorChoice, Ident, Session};
    use std::num::NonZeroU32;

    fn with_session<F: FnOnce(&Session) + Send>(f: F) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        sess.enter(|| f(&sess));
    }

    fn make_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn phase_boundary_rejects_invalid_value_references() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            for mode in 0..4 {
                let mut function = make_func();
                let value = match mode {
                    0 => ValueId::from_usize(99),
                    1 => {
                        let value = function.alloc_value(Value::Undef(MirType::I256));
                        *function.value_mut(value) = Value::Inst(InstId::from_usize(99));
                        value
                    }
                    2 => function.alloc_value(Value::Arg(crate::mir::ArgIdx::from_usize(99))),
                    _ => {
                        function.blocks[BlockId::ENTRY].instructions.push(InstId::from_usize(99));
                        function.alloc_value(Value::Immediate(Immediate::I1(true)))
                    }
                };
                // return invalid_value
                function.blocks[BlockId::ENTRY].terminator =
                    Some(Terminator::Return { values: smallvec::smallvec![value] });
                module.add_function(function);
            }
            assert!(validate_phase(&sess.dcx, &module, MirPhase::Lowered).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] [bb0] reference to undefined value v99

error: [fn1] [bb0] value v0 references nonexistent inst99

error: [fn2] [bb0] value v0 references nonexistent argument 99

error: [fn3] [bb0] block contains nonexistent inst99


"#]]
            );
        });
    }

    #[test]
    fn phase_boundary_checks_types() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = make_func();
            let mut builder = FunctionBuilder::new(&mut function);
            let value = builder.imm(0);
            // return i256 0 from an i1 function
            builder.set_return_type(MirType::I1);
            builder.set_terminator(Terminator::Return { values: smallvec::smallvec![value] });
            module.add_function(function);
            assert!(validate_phase(&sess.dcx, &module, MirPhase::Lowered).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] [bb0] return values do not match the signature


"#]]
            );
        });
    }

    #[test]
    fn terminator_operands_require_explicit_conversions() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            for mode in 0..4 {
                let mut function = make_func();
                let boolean = function.alloc_value(Value::Immediate(Immediate::I1(true)));
                let word = function.alloc_value(Value::Immediate(Immediate::I256(U256::ONE)));
                let term = match mode {
                    0 => Terminator::Revert { offset: boolean, size: word },
                    1 => Terminator::ReturnData { offset: word, size: boolean },
                    2 => Terminator::SelfDestruct { recipient: boolean },
                    _ => {
                        let next = function.alloc_block();
                        function.blocks[next].terminator = Some(Terminator::Stop);
                        Terminator::Switch {
                            value: boolean,
                            default: next,
                            cases: vec![(word, next)],
                        }
                    }
                };
                // terminate with an operand of the wrong type
                FunctionBuilder::new(&mut function).set_terminator(term);
                module.add_function(function);
            }
            assert!(validate_phase(&sess.dcx, &module, MirPhase::Lowered).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] [bb0] raw return and revert operands must have type `i256`; use an explicit cast

error: [fn1] [bb0] raw return and revert operands must have type `i256`; use an explicit cast

error: [fn2] [bb0] selfdestruct recipient must have type `i256`; use an explicit cast

error: [fn3] [bb0] switch cases must have the selector type


"#]]
            );
        });
    }

    #[test]
    fn integer_constants_must_fit_their_types() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            for bits in [1, 8, 160] {
                let mut function = make_func();
                let width = NonZeroU32::new(bits).unwrap();
                let value = function
                    .alloc_value(Value::Immediate(Immediate::Int(U256::ONE << bits, width)));
                // ret an out-of-range integer constant
                let mut builder = FunctionBuilder::new(&mut function);
                builder.set_return_type(MirType::Int(width));
                builder.ret([value]);
                module.add_function(function);
            }
            validate(&sess.dcx, &module);
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] constant v0 does not fit its type `i1`

error: [fn1] constant v0 does not fit its type `i8`

error: [fn2] constant v0 does not fit its type `i160`


"#]]
            );
        });
    }

    #[test]
    fn return_abi_matches_struct_fields() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            let pair = module.intern_struct(vec![MirType::I256, MirType::I1]);
            let slice = MirType::Slice(SliceLocation::Memory);
            let nested = module.intern_struct(vec![pair, slice]);
            let mut function = make_func();
            function.set_return_type(nested);
            let words = [MirType::I256, MirType::I1, MirType::I256, MirType::I256];
            function.set_return_abi(words);
            module.add_function(function);
            let mut validator = Validator::new(&sess.dcx);
            validator.prepare_return_abi_validation(&module);
            let matches = |ty, words: &[MirType]| {
                return_abi_matches(&module, ty, words, &validator.return_field_counts)
            };
            assert!(matches(nested, &words));
            assert!(!matches(nested, &words[..3]));
            assert!(!matches(pair, &words));
            assert!(!matches(pair, &[MirType::I1, MirType::I256]));
            assert!(matches(slice, &[MirType::I256]));
            assert!(matches(slice, &words[2..]));
            assert!(!matches(slice, &[MirType::I256, MirType::I1]));
        });
    }

    #[test]
    fn return_abi_skips_repeated_empty_structs() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            let mut ty = module.intern_struct(Vec::new());
            for _ in 0..128 {
                ty = module.intern_struct(vec![ty, ty]);
            }
            let mut function = make_func();
            function.set_return_type(ty);
            function.set_return_abi([]);
            module.add_function(function);
            let mut validator = Validator::new(&sess.dcx);
            validator.prepare_return_abi_validation(&module);
            assert!(return_abi_matches(&module, ty, &[], &validator.return_field_counts));
            assert!(!return_abi_matches(
                &module,
                ty,
                &[MirType::I256],
                &validator.return_field_counts
            ));
        });
    }

    #[test]
    fn orphaned_instruction_result_is_caught() {
        with_session(|sess| {
            let mut func = make_func();
            {
                let mut builder = FunctionBuilder::new(&mut func);
                // value = calldatasize
                // return value
                let value = builder.calldatasize();
                builder.ret([value]);
            }
            func.blocks[BlockId::ENTRY].instructions.clear();
            Validator::new(&sess.dcx).validate_standalone_function(&func);
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [bb0] use of ValueId(0) has no live definition


"#]]
            );
        });
    }

    #[test]
    fn phase_boundary_checks_predecessors_and_call_targets() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            let mut func = make_func();
            let next = func.alloc_block();
            {
                let mut builder = FunctionBuilder::new(&mut func);
                // icall fn99
                // jump bb1
                // bb1: tail_call fn98
                builder.icall_void(FunctionId::from_usize(99), Vec::new());
                builder.jump(next);
                builder.switch_to_block(next);
                builder.tail_call(FunctionId::from_usize(98), Vec::new());
            }
            func.blocks[next].predecessors.clear();
            module.add_function(func);
            assert!(module.advance_phase(&sess.dcx, MirPhase::Lowered).is_err());
            assert_eq!(module.phase(), MirPhase::Semantic);
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] [bb0] successor bb1 does not list bb0 as a predecessor

error: [fn0] icall targets nonexistent function fn99

error: [fn0] tail_call targets nonexistent function fn98


"#]]
            );
        });
    }

    #[test]
    fn phase_boundary_checks_tail_call_results() {
        for mode in 0..3 {
            with_session(|sess| {
                let mut module = Module::new(Ident::DUMMY);
                let mut callee = make_func();
                callee.set_return_type(MirType::I256);
                // ret 0
                let mut builder = FunctionBuilder::new(&mut callee);
                let zero = builder.imm(0);
                builder.ret([zero]);
                let callee = module.add_function(callee);
                let mut caller = make_func();
                caller.set_return_type(MirType::I256);
                // tail_call callee
                FunctionBuilder::new(&mut caller).tail_call(callee, Vec::new());
                let caller = module.add_function(caller);
                assert!(module.advance_phase(&sess.dcx, MirPhase::Lowered).is_ok());
                module.functions[caller].set_return_type(MirType::Void);
                match mode {
                    0 => validate(&sess.dcx, &module),
                    1 => assert!(module.advance_phase(&sess.dcx, MirPhase::Lowered).is_err()),
                    _ => assert!(module.as_lowered(&sess.dcx).is_err()),
                }
                assert_data_eq!(
                    sess.emitted_diagnostics().unwrap().to_string(),
                    str![[r#"
error: [fn1] tail_call to `.0` returns 1 value(s), caller signature expects 0


"#]]
                );
            });
        }
    }

    #[test]
    fn phase_boundary_ignores_other_modules_errors() {
        with_session(|sess| {
            sess.dcx.err("another module failed").emit();
            let mut module = Module::new(Ident::DUMMY);
            let mut func = make_func();
            // stop
            FunctionBuilder::new(&mut func).stop();
            module.add_function(func);
            assert!(module.advance_phase(&sess.dcx, MirPhase::Lowered).is_ok());
            assert!(module.as_lowered(&sess.dcx).is_ok());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: another module failed


"#]]
            );
        });
    }

    #[test]
    fn missing_dispatch_entry_is_caught_without_runtime_attributes() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);

            for _ in 0..2 {
                let mut func = make_func();
                func.selector = Some([0; 4]);
                func.attributes.is_abi_wrapper = true;
                FunctionBuilder::new(&mut func).stop();
                module.functions.push(func);
            }
            let _ = validate_phase(&sess.dcx, &module, MirPhase::Lowered);
            assert!(sess.dcx.has_errors().is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: module is in the `lowered` phase but has no `entry` routing function


"#]]
            );
        });
    }

    #[test]
    fn invalid_data_reference_is_caught() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            let mut func = make_func();
            {
                let mut builder = FunctionBuilder::new(&mut func);
                let dest = builder.imm(0);
                let size = builder.imm(1);
                builder.data_copy(DataRef::new(DataId::from_usize(7), 0), dest, size);
                builder.stop();
            }
            module.functions.push(func);
            Validator::new(&sess.dcx).validate_module(&module);
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] [bb0, inst0] data_copy references nonexistent data7


"#]]
            );
        });
    }

    #[test]
    fn invalid_data_offset_is_caught() {
        with_session(|sess| {
            let mut module = Module::new(Ident::DUMMY);
            let data = module.add_data(vec![0; 4].into(), None);
            let mut func = make_func();
            {
                let mut builder = FunctionBuilder::new(&mut func);
                let dest = builder.imm(0);
                let size = builder.imm(1);
                builder.data_copy(DataRef::new(data, 5), dest, size);
                builder.stop();
            }
            module.functions.push(func);
            Validator::new(&sess.dcx).validate_module(&module);
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: [fn0] [bb0, inst0] data_copy range 5..6 exceeds data size 4


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
                let _p = b.add_param(MirType::I256);
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
                let x = b.add_param(MirType::I256);
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
