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
    AddressCallKind, BlockId, Function, FunctionId, InstId, InstKind, MemoryObjectKind,
    MemoryObjectLayout, MirPhase, MirType, Module, SliceLocation, TypeSize, Value, ValueId,
    analysis::CfgInfo,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
};
use solar_interface::{diagnostics::DiagCtxt, kw};
use std::fmt;

/// Stateful MIR verifier.
struct Validator<'a> {
    dcx: &'a DiagCtxt,
    function: Option<FunctionId>,
    error_count: usize,
    returning_functions: DenseBitSet<FunctionId>,
}

impl<'a> Validator<'a> {
    /// Creates a verifier that emits findings into `dcx`.
    fn new(dcx: &'a DiagCtxt) -> Self {
        Self { dcx, function: None, error_count: 0, returning_functions: DenseBitSet::new_empty(0) }
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
        self.validate_function_body(None, func);
    }

    fn validate_function(&mut self, module: &Module, func: &Function) {
        let errors_before = self.error_count;
        for (index, ty) in func.params.iter().enumerate() {
            if *ty == MirType::Void {
                self.emit(format_args!("parameter {index} cannot have type `void`"));
            }
        }
        if func.returns.contains(&MirType::Void) {
            self.emit("return signature cannot contain `void`; use an empty return list");
        }
        self.validate_function_body(Some(module), func);
        self.validate_immutables(module, func);
        self.validate_calls(module, func);
        if self.error_count == errors_before {
            self.validate_struct_values(module, func);
            self.validate_memory_object_types(func);
        }
        self.validate_function_phase(module.phase(), func);
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

        // ----- Entry block invariants -----
        if !func.blocks[BlockId::ENTRY].predecessors.is_empty() {
            self.emit_at_block("entry block must have no predecessors", BlockId::ENTRY);
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
        self.returning_functions = module.returning_functions();
        for (id, ty) in module.struct_types.iter_enumerated() {
            for field in &ty.fields {
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
        self.validate_module_phase(module, module.phase());
        self.validate_immutable_declarations(module);
        for (id, func) in module.iter_functions() {
            self.function = Some(id);
            self.validate_function(module, func);
        }
        self.function = None;
    }

    /// Checks aggregate operands, field indices, and result types against the module declarations.
    fn validate_struct_values(&mut self, module: &Module, func: &Function) {
        if func.returns.len() > 1 && func.returns.iter().any(|ty| matches!(ty, MirType::Struct(_)))
        {
            self.emit("a struct result must be the function's only result");
        }
        for ty in func
            .arg_indices()
            .map(|index| func.arg_ty(index))
            .chain(func.returns.iter().copied())
            .chain(func.live_values().filter_map(|value| func.value_ty(value)))
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
                match &inst.kind {
                    InstKind::Phi(incoming) => {
                        for &(_, value) in incoming {
                            self.check_struct_type(func.value_ty(value), inst.result_ty, block, id);
                        }
                    }
                    InstKind::Select(condition, a, b) => {
                        self.check_struct_type(
                            func.value_ty(*condition),
                            Some(MirType::Bool),
                            block,
                            id,
                        );
                        for value in [*a, *b] {
                            self.check_struct_type(func.value_ty(value), inst.result_ty, block, id);
                        }
                    }
                    InstKind::ICall { function, args, .. } => {
                        if let Some(callee) = module.functions.get(*function) {
                            for (&value, &ty) in args.iter().zip(&callee.params) {
                                self.check_struct_type(func.value_ty(value), Some(ty), block, id);
                            }
                            if inst.result_ty.is_some() {
                                self.check_struct_type(
                                    inst.result_ty,
                                    callee.returns.first().copied(),
                                    block,
                                    id,
                                );
                            }
                        }
                    }
                    InstKind::AbiDecode { data, layout } => {
                        self.check_struct_type(
                            func.value_ty(*data),
                            Some(MirType::MemoryObject(MemoryObjectKind::Bytes)),
                            block,
                            id,
                        );
                        let valid = match inst.result_ty {
                            Some(MirType::Struct(ty)) => {
                                module.struct_types.get(ty).is_some_and(|ty| {
                                    ty.fields.len() == layout.types.len()
                                        && ty.fields.iter().zip(&layout.types).all(
                                            |(&field, abi)| {
                                                field == abi.mir_type().return_field_type()
                                            },
                                        )
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
                    InstKind::WordCast(value) => {
                        if inst.result_ty != Some(MirType::uint256())
                            || !func.value_ty(*value).is_some_and(MirType::is_word)
                        {
                            self.emit_at_inst(
                                "word cast requires a one-word operand and u256 result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::MemoryObjectFromPtr { ptr, kind } => {
                        if inst.result_ty != Some(MirType::MemoryObject(*kind))
                            || !func.value_ty(*ptr).is_some_and(MirType::is_word)
                        {
                            self.emit_at_inst("memory object pointer conversion requires a word and matching object result", block, id);
                        }
                    }
                    InstKind::InsertValue { .. } | InstKind::ExtractValue { .. } => {}
                    _ => {
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
                    if !func.value_ty(value).is_some_and(|actual| field.accepts_field_value(actual))
                    {
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
            match &body.terminator {
                Some(crate::mir::Terminator::Return { values }) => {
                    let has_struct = func.returns.iter().any(|ty| matches!(ty, MirType::Struct(_)))
                        || values
                            .iter()
                            .any(|&value| matches!(func.value_ty(value), Some(MirType::Struct(_))));
                    if values.len() != func.returns.len() {
                        self.emit_at_block(
                            format_args!(
                                "return has {} value(s), signature expects {}",
                                values.len(),
                                func.returns.len()
                            ),
                            block,
                        );
                    } else if has_struct
                        && values
                            .iter()
                            .zip(&func.returns)
                            .any(|(&value, &ty)| func.value_ty(value) != Some(ty))
                    {
                        self.emit_at_block(
                            "return values do not match the struct signature",
                            block,
                        );
                    }
                }
                Some(crate::mir::Terminator::TailCall { function, args }) => {
                    if let Some(callee) = module.functions.get(*function) {
                        if args.iter().zip(&callee.params).any(|(&value, &ty)| {
                            let actual = func.value_ty(value);
                            actual != Some(ty)
                                && (matches!(actual, Some(MirType::Struct(_)))
                                    || matches!(ty, MirType::Struct(_)))
                        }) {
                            self.emit_at_block(
                                "tail-call arguments do not match the struct signature",
                                block,
                            );
                        }
                        if func.selector.is_none()
                            && func.returns != callee.returns
                            && func
                                .returns
                                .iter()
                                .chain(&callee.returns)
                                .any(|ty| matches!(ty, MirType::Struct(_)))
                            && self.returning_functions.contains(*function)
                        {
                            self.emit_at_block(
                                "tail-call results do not match the struct signature",
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

    /// Checks nominal object kinds without rejecting the raw pointer carriers used during lowering.
    fn validate_memory_object_types(&mut self, func: &Function) {
        for (block, body) in func.blocks.iter_enumerated() {
            for &id in &body.instructions {
                if let InstKind::Require { condition, payload } = &func.inst(id).kind {
                    let word = |value| {
                        func.value_ty(value).is_some_and(|ty| {
                            ty.is_word() && !matches!(ty, MirType::MemoryObject(_))
                        })
                    };
                    if !word(*condition) || func.inst(id).result_ty.is_some() {
                        self.emit_at_inst(
                            "require needs a word condition and no result",
                            block,
                            id,
                        );
                    }
                    let valid = match payload.as_ref() {
                        crate::mir::RevertPayload::ShortString { length, data } => {
                            word(*length)
                                && word(*data)
                                && func
                                    .value_u64(*length)
                                    .is_some_and(|length| (1..=32).contains(&length))
                        }
                        crate::mir::RevertPayload::EmptyString => true,
                        crate::mir::RevertPayload::ErrorString(value) => matches!(
                            func.value_ty(*value),
                            Some(
                                MirType::MemoryObject(MemoryObjectKind::Bytes)
                                    | MirType::MemPtr
                                    | MirType::UInt(_)
                            )
                        ),
                        crate::mir::RevertPayload::CustomError { selector, layout, values } => {
                            word(*selector) && values.len() == layout.types.len()
                        }
                    };
                    if !valid {
                        self.emit_at_inst("require payload has incompatible arguments", block, id);
                    }
                }
                if let InstKind::AbiEncodePacked { parts, hash } = &func.inst(id).kind {
                    for part in parts {
                        let valid = match part {
                            crate::mir::PackedPart::Literal(_) => true,
                            crate::mir::PackedPart::Scalar { value, ty } => {
                                matches!(
                                    ty,
                                    MirType::UInt(_)
                                        | MirType::Int(_)
                                        | MirType::FixedBytes(_)
                                        | MirType::Address
                                        | MirType::Bool
                                        | MirType::Function
                                ) && ty.type_size().is_some_and(|size| size.bytes() != 0)
                                    && func.value_ty(*value).is_some_and(|ty| {
                                        ty.is_word() && !matches!(ty, MirType::MemoryObject(_))
                                    })
                            }
                            crate::mir::PackedPart::Bytes(value) => matches!(
                                func.value_ty(*value),
                                Some(
                                    MirType::MemoryObject(MemoryObjectKind::Bytes)
                                        | MirType::MemPtr
                                        | MirType::UInt(_)
                                        | MirType::Slice(
                                            SliceLocation::Memory | SliceLocation::Calldata
                                        )
                                )
                            ),
                            crate::mir::PackedPart::Array { value, element, source } => {
                                !hash
                                    && crate::mir::packed_element_bytes(element).is_some()
                                    && match source {
                                        crate::mir::PackedArraySource::Memory { layout } => {
                                            matches!(
                                                layout,
                                                MemoryObjectLayout::DynamicArray { .. }
                                                    | MemoryObjectLayout::FixedArray { .. }
                                            ) && match func.value_ty(*value) {
                                                Some(MirType::MemoryObject(kind)) => {
                                                    kind == layout.kind()
                                                }
                                                Some(MirType::MemPtr | MirType::UInt(_)) => true,
                                                _ => false,
                                            }
                                        }
                                        crate::mir::PackedArraySource::Slice(location) => {
                                            matches!(
                                                location,
                                                SliceLocation::Memory | SliceLocation::Calldata
                                            ) && func.value_ty(*value)
                                                == Some(MirType::Slice(*location))
                                        }
                                    }
                            }
                        };
                        if !valid {
                            self.emit_at_inst(
                                "packed encoding input has an incompatible shape",
                                block,
                                id,
                            );
                        }
                    }
                    let result = if *hash {
                        MirType::bytes32()
                    } else {
                        MirType::MemoryObject(MemoryObjectKind::Bytes)
                    };
                    if func.inst(id).result_ty != Some(result) {
                        self.emit_at_inst(
                            "packed encoding has an incompatible result type",
                            block,
                            id,
                        );
                    }
                }
                if let InstKind::Concat(parts) = &func.inst(id).kind {
                    for part in parts {
                        let valid = match part {
                            crate::mir::ConcatPart::Bytes(value) => {
                                matches!(
                                    func.value_ty(*value),
                                    Some(
                                        MirType::MemoryObject(MemoryObjectKind::Bytes)
                                            | MirType::MemPtr
                                            | MirType::UInt(_)
                                    )
                                )
                            }
                            crate::mir::ConcatPart::Fixed { value, .. } => {
                                func.value_ty(*value).is_some_and(|ty| {
                                    ty.is_word() && !matches!(ty, MirType::MemoryObject(_))
                                })
                            }
                        };
                        if !valid {
                            self.emit_at_inst("concat input has an incompatible type", block, id);
                        }
                    }
                    if func.inst(id).result_ty
                        != Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                    {
                        self.emit_at_inst("concat requires a memorybytes result", block, id);
                    }
                }
                let mut check = |object, expected| {
                    if let Some(MirType::MemoryObject(actual)) = func.value_ty(object)
                        && actual != expected
                    {
                        self.emit_at_inst(
                            format_args!(
                                "memory object has type `{actual}`, expected `{expected}`"
                            ),
                            block,
                            id,
                        );
                    }
                };
                match func.inst(id).kind {
                    InstKind::Check { condition, .. } => {
                        if func.inst(id).result_ty.is_some()
                            || func.value_ty(condition).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        {
                            self.emit_at_inst(
                                "conditional check requires a word condition and no result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::ValidateAbi(value) => {
                        if func.inst(id).result_ty.is_some()
                            || func.value_ty(value).is_none_or(|ty| !ty.is_word())
                        {
                            self.emit_at_inst(
                                "ABI validation requires one word operand and no result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::CheckedAddMod(a, b, modulus)
                    | InstKind::CheckedMulMod(a, b, modulus) => {
                        if [a, b, modulus].iter().any(|value| {
                            func.value_ty(*value).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        }) {
                            self.emit_at_inst(
                                "checked modular arithmetic requires word operands",
                                block,
                                id,
                            );
                        }
                        if func.inst(id).result_ty != Some(MirType::uint256()) {
                            self.emit_at_inst(
                                "checked modular arithmetic requires a u256 result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::CheckedBinary { arithmetic, lhs, rhs, .. } => {
                        let (crate::mir::ArithmeticKind::Unsigned(bits)
                        | crate::mir::ArithmeticKind::Signed(bits)) = arithmetic;
                        if !(8..=256).contains(&bits) || bits % 8 != 0 {
                            self.emit_at_inst("checked arithmetic has an invalid width", block, id);
                        }
                        if [lhs, rhs].iter().any(|value| {
                            func.value_ty(*value).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        }) {
                            self.emit_at_inst(
                                "checked arithmetic requires word operands",
                                block,
                                id,
                            );
                        }
                        if func.inst(id).result_ty != Some(MirType::uint256()) {
                            self.emit_at_inst(
                                "checked arithmetic requires a u256 result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::StorageArrayLoad { slot, element, enum_variants } => {
                        if func.value_ty(slot).is_none_or(|ty| {
                            !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                        }) {
                            self.emit_at_inst("storage array load requires a word slot", block, id);
                        }
                        if !matches!(
                            element,
                            MirType::UInt(_)
                                | MirType::Int(_)
                                | MirType::FixedBytes(_)
                                | MirType::MemoryObject(MemoryObjectKind::Bytes)
                        ) {
                            self.emit_at_inst("invalid storage array element type", block, id);
                        }
                        if let Some(variants) = enum_variants
                            && (!(1..=256).contains(&variants)
                                || element != MirType::UInt(TypeSize::new_int_bits(8)))
                        {
                            self.emit_at_inst("invalid storage array enum type", block, id);
                        }
                        if func.inst(id).result_ty
                            != Some(MirType::MemoryObject(MemoryObjectKind::DynamicArray))
                        {
                            self.emit_at_inst(
                                "storage array load requires an array result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::StorageBytesStore(slot, object) => {
                        if func.value_ty(slot).is_none_or(|ty| {
                            !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                        }) {
                            self.emit_at_inst(
                                "storage bytes store requires a word slot",
                                block,
                                id,
                            );
                        }
                        if func.value_ty(object)
                            != Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                        {
                            self.emit_at_inst(
                                "storage bytes store requires a bytes object",
                                block,
                                id,
                            );
                        }
                        if func.inst(id).result_ty.is_some() {
                            self.emit_at_inst(
                                "storage bytes store cannot produce a result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::StorageClearWords(slot, first, end) => {
                        if [slot, first, end].iter().any(|&value| {
                            func.value_ty(value).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        }) {
                            self.emit_at_inst("storage clear requires word operands", block, id);
                        }
                        if func.inst(id).result_ty.is_some() {
                            self.emit_at_inst("storage clear cannot produce a result", block, id);
                        }
                    }
                    InstKind::ValidateStorageBytes(operand)
                    | InstKind::StorageBytesLoad(operand)
                    | InstKind::StorageBytesStoreLiteral { slot: operand, .. } => {
                        if func.value_ty(operand).is_none_or(|ty| {
                            !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                        }) {
                            self.emit_at_inst(
                                "storage bytes operation requires a word operand",
                                block,
                                id,
                            );
                        }
                        let result_ty =
                            if matches!(func.inst(id).kind, InstKind::StorageBytesLoad(_)) {
                                Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                            } else {
                                None
                            };
                        if func.inst(id).result_ty != result_ty {
                            self.emit_at_inst(
                                "storage bytes operation has an invalid result type",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::Erc7201(object)
                    | InstKind::Sha256(object)
                    | InstKind::Ripemd160(object) => {
                        if !matches!(
                            func.value_ty(object),
                            Some(
                                MirType::MemoryObject(MemoryObjectKind::Bytes)
                                    | MirType::MemPtr
                                    | MirType::UInt(_)
                            )
                        ) {
                            self.emit_at_inst(
                                "hash builtin requires a memorybytes operand",
                                block,
                                id,
                            );
                        }
                        if func.inst(id).result_ty != Some(MirType::uint256()) {
                            self.emit_at_inst("hash builtin requires a u256 result", block, id);
                        }
                    }
                    InstKind::AddressCall { kind, address, input, gas, value } => {
                        check(input, MemoryObjectKind::Bytes);
                        if std::iter::once(address).chain(gas).chain(value).any(|operand| {
                            func.value_ty(operand).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        }) {
                            self.emit_at_inst(
                                "address call options require word operands",
                                block,
                                id,
                            );
                        }
                        if kind != AddressCallKind::Call && value.is_some() {
                            self.emit_at_inst(
                                "only address_call accepts a value option",
                                block,
                                id,
                            );
                        }
                        if func.inst(id).result_ty != Some(MirType::uint256()) {
                            self.emit_at_inst("address call requires a u256 result", block, id);
                        }
                    }
                    InstKind::ReturndataBytes => {
                        if func.inst(id).result_ty
                            != Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                        {
                            self.emit_at_inst(
                                "returndata_bytes requires a bytes object result",
                                block,
                                id,
                            );
                        }
                    }
                    InstKind::Send(address, amount) | InstKind::Transfer(address, amount) => {
                        if [address, amount].iter().any(|value| {
                            func.value_ty(*value).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        }) {
                            self.emit_at_inst("payable call requires word operands", block, id);
                        }
                        let expected = matches!(func.inst(id).kind, InstKind::Send(..))
                            .then_some(MirType::uint256());
                        if func.inst(id).result_ty != expected {
                            self.emit_at_inst("payable call has an invalid result type", block, id);
                        }
                    }
                    InstKind::EcRecover(a, b, c, d) => {
                        if [a, b, c, d].iter().any(|v| {
                            func.value_ty(*v).is_none_or(|ty| {
                                !ty.is_word() || matches!(ty, MirType::MemoryObject(_))
                            })
                        }) {
                            self.emit_at_inst("ecrecover requires word operands", block, id);
                        }
                        if func.inst(id).result_ty != Some(MirType::uint256()) {
                            self.emit_at_inst("ecrecover requires a u256 result", block, id);
                        }
                    }
                    InstKind::MemoryObjectLen(object, kind)
                    | InstKind::SetMemoryObjectLen(object, _, kind)
                    | InstKind::MemoryObjectData(object, kind)
                    | InstKind::MemoryObjectCopyFromSlice { object, kind, .. }
                    | InstKind::MemoryObjectCopyFromSliceAt { object, kind, .. } => {
                        check(object, kind)
                    }
                    InstKind::MemoryObjectFieldAddr { object, layout, .. }
                    | InstKind::MemoryObjectElementAddr { object, layout, .. }
                    | InstKind::MemoryObjectLoadField { object, layout, .. }
                    | InstKind::MemoryObjectStoreField { object, layout, .. }
                    | InstKind::MemoryObjectLoadElement { object, layout, .. }
                    | InstKind::MemoryObjectStoreElement { object, layout, .. } => {
                        check(object, layout.kind())
                    }
                    InstKind::MemoryObjectLoadByte { object, .. }
                    | InstKind::MemoryObjectStoreByte { object, .. }
                    | InstKind::MemoryObjectStoreWord { object, .. } => {
                        check(object, MemoryObjectKind::Bytes)
                    }
                    InstKind::MemoryObjectCopy {
                        destination,
                        destination_kind,
                        source,
                        source_kind,
                        ..
                    } => {
                        check(destination, destination_kind);
                        check(source, source_kind);
                    }
                    _ => {}
                }
            }
        }
    }

    fn check_struct_type(
        &mut self,
        actual: Option<MirType>,
        expected: Option<MirType>,
        block: BlockId,
        inst: InstId,
    ) {
        if actual != expected
            && [actual, expected].iter().any(|ty| matches!(ty, Some(MirType::Struct(_))))
        {
            self.emit_at_inst("struct value does not match the required type", block, inst);
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
            let InstKind::ICall { function, args } = &func.inst(inst_id).kind else {
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
            if func.inst(inst_id).result_ty.is_some() && callee.returns.is_empty() {
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
                && func.returns.len() != callee.returns.len()
                && self.returning_functions.contains(*function)
            {
                self.emit(format_args!(
                    "tail_call to `{}` returns {} value(s), caller signature expects {}",
                    callee.name,
                    callee.returns.len(),
                    func.returns.len()
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
                .chain(func.returns.iter().copied())
                .chain(func.live_values().filter_map(|value| func.value_ty(value)))
                .chain(func.instructions().filter_map(|id| func.inst(id).result_ty));
            if let Some(ty) =
                types.into_iter().find(|ty| !ty.is_word() || matches!(ty, MirType::MemoryObject(_)))
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
                    let semantic_op = func.inst(inst_id).unlowered_reason();
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

/// Checks representation legality without computing dominance or call summaries.
pub(crate) fn validate_phase(
    dcx: &DiagCtxt,
    module: &Module,
    phase: MirPhase,
) -> solar_interface::Result<()> {
    let mut validator = Validator::new(dcx);
    validator.validate_module_phase(module, phase);
    for (id, func) in module.iter_functions() {
        validator.function = Some(id);
        validator.validate_function_phase(phase, func);
    }
    dcx.has_errors()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{DataId, DataRef, Function, FunctionBuilder, MirType, Terminator};
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
