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
//! 15. **Representation boundaries**: SSA aggregates and semantic memory operations cannot survive
//!     their lowering boundaries.
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
                inst.kind.visit_operands(|value| {
                    self.validate_value_reference(func, value, num_args, block);
                });
                if let Some(value) = inst.result() {
                    self.validate_value_reference(func, value, num_args, block);
                }
            }
            if let Some(term) = &body.terminator {
                term.visit_operands(|value| {
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

            // Check stored predecessor blocks exist, are listed once, and branch to this block.
            for (index, &pred) in block.predecessors.iter().enumerate() {
                if block.predecessors[..index].contains(&pred) {
                    self.emit_at_block(
                        format_args!("predecessor bb{} is listed more than once", pred.index()),
                        block_id,
                    );
                }
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

        if num_blocks == 0 {
            self.emit("function has no entry block");
            return;
        }

        // `validate_references` already checked that every instruction, operand,
        // and result reference is in range.
        self.validate_cfg(func);
        for (block_id, block) in func.blocks.iter_enumerated() {
            if block.terminator.is_none() {
                continue;
            }

            // ----- Walk instructions in this block -----
            let block_preds = &block.predecessors;
            for &inst_id in &block.instructions {
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
                        kind.visit_operands(|operand| {
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
                        });
                    }
                }
            }
            if let Some(term) = &block.terminator {
                term.visit_operands(|operand| {
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
        self.count_return_fields(module);
    }

    fn count_return_fields(&mut self, module: &Module) {
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
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);
                inst.kind.visit_operands(|value| self.validate_live_value_type(func, value));
                if let Some(value) = inst.result() {
                    self.validate_live_value_type(func, value);
                }
            }
            if let Some(term) = &block.terminator {
                term.visit_operands(|value| self.validate_live_value_type(func, value));
            }
        }
        for ty in func
            .arg_indices()
            .map(|index| func.arg_ty(index))
            .chain(std::iter::once(func.return_type()))
            .chain(func.return_components().iter().copied())
        {
            self.validate_integer_type(ty);
            if let MirType::Struct(id) = ty
                && module.struct_types.get(id).is_none()
            {
                self.emit(format_args!("undefined struct type `struct{}`", id.index()));
            }
        }
        for (block, body) in func.blocks.iter_enumerated() {
            for &id in &body.instructions {
                let inst = func.inst(id);
                let mut has_struct_value = false;
                let mut check_struct = |ty| {
                    if let Some(MirType::Struct(ty)) = ty {
                        has_struct_value = true;
                        if module.struct_types.get(ty).is_none() {
                            self.emit(format_args!("undefined struct type `struct{}`", ty.index()));
                        }
                    }
                };
                inst.kind.visit_operands(|value| check_struct(func.value_ty(value)));
                check_struct(inst.result_ty);
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
                            Some(MirType::MemPtr),
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
                        for value in inst.kind.operands() {
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
                term.visit_operands(|value| {
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

    /// Checks that a live value has a value type and that a constant fits it.
    fn validate_live_value_type(&mut self, func: &Function, value: ValueId) {
        match func.value_ty(value) {
            None | Some(MirType::Void) => {
                self.emit(format_args!("live value v{} has no value type", value.index()));
            }
            Some(ty) => self.validate_integer_type(ty),
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

    /// Checks semantic result types and layout metadata after operand contracts have passed.
    fn validate_memory_object_types(&mut self, func: &Function) {
        for (block, body) in func.blocks.iter_enumerated() {
            for &id in &body.instructions {
                let inst = func.inst(id);
                let valid = match &inst.kind {
                    InstKind::ICall { function: Callee::Builtin(builtin), args } => {
                        let result = match builtin {
                            Builtin::Require(_) | Builtin::Check { .. } | Builtin::Transfer => None,
                            Builtin::ReturndataBytes | Builtin::Concat(_) => Some(MirType::MemPtr),
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
                                == Some(if *hash { MirType::I256 } else { MirType::MemPtr })
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
                        }) && inst.result_ty == Some(MirType::MemPtr)
                    }
                    InstKind::StorageBytesLoad(_) => inst.result_ty == Some(MirType::MemPtr),
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
        let (data, size) = match &func.inst(inst_id).kind {
            InstKind::DataCopy(data, _, size) => (data, size),
            InstKind::DataSize(size) => {
                match module.data.get(size.data) {
                    None => self.emit_at_inst(
                        format_args!("datasize references nonexistent data{}", size.data.index()),
                        block_id,
                        inst_id,
                    ),
                    Some(data) if data.bytes.known().is_some() => {
                        self.emit_at_inst("datasize requires deferred data", block_id, inst_id)
                    }
                    Some(_) => {}
                }
                return;
            }
            _ => return,
        };
        let Some(entry) = module.data.get(data.id) else {
            self.emit_at_inst(
                format_args!("datacopy references nonexistent data{}", data.id.index()),
                block_id,
                inst_id,
            );
            return;
        };
        // The length of deferred data is only known through its own `datasize`.
        if let Value::Inst(size) = func.value(*size)
            && matches!(func.inst(*size).kind, InstKind::DataSize(size) if size.is_length_of(*data))
        {
            return;
        }
        let Some(bytes) = entry.bytes.known() else {
            self.emit_at_inst(
                "datacopy size of deferred data must be its `datasize`",
                block_id,
                inst_id,
            );
            return;
        };
        let Some(size) = func.value_u256(*size) else {
            self.emit_at_inst("datacopy size must be an immediate", block_id, inst_id);
            return;
        };
        let end = U256::from(data.offset).checked_add(size);
        if end.is_none_or(|end| end > U256::from(bytes.len())) {
            self.emit_at_inst(
                format_args!(
                    "datacopy range {}..{} exceeds data size {}",
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
            if let Some(ty) = first_non_word_type(func) {
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

/// Checks `function` against the requested phase as a replacement for function `id` of `module`.
///
/// Calls resolve against the rest of the module, which is not checked again.
pub(crate) fn validate_function_at_phase(
    dcx: &DiagCtxt,
    module: &Module,
    id: FunctionId,
    function: &Function,
    phase: MirPhase,
) -> solar_interface::Result<()> {
    let mut validator = Validator::new(dcx);
    validator.returning_functions = Some(module.returning_functions());
    if function.return_abi().is_some() {
        validator.count_return_fields(module);
    }
    validator.function = Some(id);
    validator.validate_function(module, function, phase);
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
                    MirType::MemPtr if actual == MirType::I256 => {}
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

/// Returns the first type, in signature and then block order, that is not a scalar word.
fn first_non_word_type(func: &Function) -> Option<MirType> {
    let non_word = |ty: MirType| !matches!(ty, MirType::I1 | MirType::I256 | MirType::MemPtr);
    let signature = func.arg_indices().map(|index| func.arg_ty(index));
    if let Some(ty) =
        signature.chain(func.return_components().iter().copied()).find(|&ty| non_word(ty))
    {
        return Some(ty);
    }
    let value_type = |value| func.value_ty(value).filter(|&ty| non_word(ty));
    let find = |found: &mut Option<MirType>, value| {
        if found.is_none() {
            *found = value_type(value);
        }
    };
    let mut found = None;
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            inst.kind.visit_operands(|value| find(&mut found, value));
            if let Some(value) = inst.result() {
                find(&mut found, value);
            }
            if found.is_some() {
                return found;
            }
        }
        if let Some(term) = &block.terminator {
            term.visit_operands(|value| find(&mut found, value));
        }
        if found.is_some() {
            return found;
        }
    }
    func.instructions().filter_map(|id| func.inst(id).result_ty).find(|&ty| non_word(ty))
}

// =============================================================================
// Tests
// =============================================================================
