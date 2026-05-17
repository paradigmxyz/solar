use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    hir::{self, Item, ItemId, Res, Visit},
    ty::{Gcx, Ty, TyKind},
};
use alloy_primitives::{B256, U256};
use rayon::prelude::*;
use solar_ast::{DataLocation, StateMutability, UserDefinableOperator, Visibility};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
    parallel,
};
use solar_interface::{Span, error_code};
use std::ops::ControlFlow;

mod checker;
mod override_checker;

pub(crate) fn check(gcx: Gcx<'_>) {
    parallel!(
        gcx.sess,
        gcx.hir.par_contract_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.contract_scopes[id]);
            check_storage_size_upper_bound(gcx, id);
            check_payable_fallback_without_receive(gcx, id);
            check_external_type_clashes(gcx, id);
            check_receive_function(gcx, id);
            for using in gcx.hir.contract(id).usings {
                check_using_directive(gcx, using);
            }
            check_unimplemented_functions(gcx, id);
            override_checker::check(gcx, id);
        }),
        gcx.hir.par_source_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.source_scopes[id]);
            check_break_continue(gcx, id);
            for using in gcx.hir.source(id).usings {
                check_using_directive(gcx, using);
            }
            if gcx.sess.opts.unstable.typeck {
                // TODO: Parallelize more.
                checker::check(gcx, id);
            }
        }),
    );
}

fn check_using_directive(gcx: Gcx<'_>, using: &hir::UsingDirective<'_>) {
    let using_ty = using.ty.as_ref().map(|ty| gcx.type_of_hir_ty(ty));

    if using.global
        && let Some(ty) = using_ty
    {
        match same_source_file_level_user_type(gcx, ty, using.source) {
            Ok(()) => {}
            Err(GlobalUsingError::NotUserDefined) => {
                gcx.dcx()
                    .err("can only use `global` with user-defined types")
                    .span(using.span)
                    .emit();
            }
            Err(GlobalUsingError::NotSameSourceFileLevel) => {
                gcx.dcx()
                    .err("can only use `global` with types defined in the same source unit at file level")
                    .span(using.span)
                    .emit();
            }
        }
    }

    for entry in using.entries {
        match entry.kind {
            hir::UsingEntryKind::Library(id) => {
                if !gcx.hir.contract(id).kind.is_library() {
                    gcx.dcx()
                        .err("library name expected")
                        .span(entry.span)
                        .help("if you want to attach a function, use `{...}`")
                        .emit();
                }
            }
            hir::UsingEntryKind::Functions(functions) => {
                for &function in functions {
                    check_using_function(gcx, using, entry, function, using_ty);
                }
            }
            hir::UsingEntryKind::Err => {}
        }
    }
}

enum GlobalUsingError {
    NotUserDefined,
    NotSameSourceFileLevel,
}

fn same_source_file_level_user_type(
    gcx: Gcx<'_>,
    ty: Ty<'_>,
    source: hir::SourceId,
) -> Result<(), GlobalUsingError> {
    match ty.peel_refs().kind {
        TyKind::Struct(id) => {
            let item = gcx.hir.strukt(id);
            if item.source == source && item.contract.is_none() {
                Ok(())
            } else {
                Err(GlobalUsingError::NotSameSourceFileLevel)
            }
        }
        TyKind::Enum(id) => {
            let item = gcx.hir.enumm(id);
            if item.source == source && item.contract.is_none() {
                Ok(())
            } else {
                Err(GlobalUsingError::NotSameSourceFileLevel)
            }
        }
        TyKind::Udvt(_, id) => {
            let item = gcx.hir.udvt(id);
            if item.source == source && item.contract.is_none() {
                Ok(())
            } else {
                Err(GlobalUsingError::NotSameSourceFileLevel)
            }
        }
        _ => Err(GlobalUsingError::NotUserDefined),
    }
}

fn check_using_function<'gcx>(
    gcx: Gcx<'gcx>,
    using: &hir::UsingDirective<'_>,
    entry: &hir::UsingEntry<'_>,
    function_id: hir::FunctionId,
    using_ty: Option<Ty<'gcx>>,
) {
    let function = gcx.hir.function(function_id);
    let is_library_function =
        function.contract.is_some_and(|id| gcx.hir.contract(id).kind.is_library());
    if !function.is_free() && !is_library_function {
        gcx.dcx()
            .err("only file-level functions and library functions can be attached to a type in a `using` statement")
            .span(entry.span)
            .emit();
    }

    let TyKind::FnPtr(function_ty) = gcx.type_of_item(function_id.into()).kind else {
        unreachable!()
    };
    if function_ty.parameters.is_empty() {
        gcx.dcx()
            .err(format!(
                "function `{}` does not have any parameters and therefore cannot be attached",
                gcx.item_name(function_id).as_str()
            ))
            .span(entry.span)
            .span_note(function.span, "function defined here")
            .emit();
        return;
    }

    if function.visibility == Visibility::Private && function.contract != using.contract {
        gcx.dcx()
            .err(format!(
                "function `{}` is private and therefore cannot be attached to a type outside of the library where it is defined",
                gcx.item_name(function_id).as_str()
            ))
            .span(entry.span)
            .span_note(function.span, "function defined here")
            .emit();
    }

    if let Some(using_ty) = using_ty {
        let normalized_using_ty = using_ty.with_loc_if_ref(gcx, DataLocation::Storage);
        let self_ty = function_ty.parameters[0].with_loc_if_ref(gcx, DataLocation::Storage);
        if !normalized_using_ty.convert_implicit_to(self_ty, gcx) && entry.operator.is_none() {
            gcx.dcx()
                .err(format!(
                    "function `{}` cannot be attached to type `{}`",
                    gcx.item_name(function_id).as_str(),
                    using_ty.display(gcx)
                ))
                .span(entry.span)
                .span_note(function.span, "function defined here")
                .help(format!(
                    "the type cannot be implicitly converted to the first function argument `{}`",
                    function_ty.parameters[0].display(gcx)
                ))
                .emit();
        }
    }

    if let Some(op) = entry.operator {
        check_using_operator(gcx, using, entry, function_id, function_ty, using_ty, op);
    }
}

fn check_using_operator<'gcx>(
    gcx: Gcx<'gcx>,
    using: &hir::UsingDirective<'_>,
    entry: &hir::UsingEntry<'_>,
    function_id: hir::FunctionId,
    function_ty: &crate::ty::TyFnPtr<'gcx>,
    using_ty: Option<Ty<'gcx>>,
    op: UserDefinableOperator,
) {
    let function = gcx.hir.function(function_id);

    if !using.global {
        gcx.dcx()
            .err("operators can only be defined in a global `using for` directive")
            .span(entry.span)
            .emit();
    }

    if function_ty.state_mutability != StateMutability::Pure || !function.is_free() {
        gcx.dcx()
            .err("only pure free functions can be used to define operators")
            .span(entry.span)
            .span_note(function.span, "function defined here")
            .emit();
    }

    let Some(using_ty) = using_ty else { return };
    let TyKind::Udvt(_, _) = using_ty.kind else {
        gcx.dcx()
            .err("operators can only be implemented for user-defined value types")
            .span(entry.span)
            .emit();
        return;
    };

    let params = function_ty.parameters;
    let is_unary_only = matches!(op, UserDefinableOperator::BitNot);
    let is_binary_only = !matches!(op, UserDefinableOperator::Sub | UserDefinableOperator::BitNot);
    let first_matches = params.first().is_some_and(|&ty| ty == using_ty);
    let first_two_match = params.len() < 2 || params[0] == params[1];

    let wrong_params = if is_binary_only && (params.len() != 2 || !first_two_match) {
        Some(format!("two parameters of type `{}`", using_ty.display(gcx)))
    } else if is_unary_only && (params.len() != 1 || !first_matches) {
        Some(format!("exactly one parameter of type `{}`", using_ty.display(gcx)))
    } else if params.len() >= 3 || !first_matches || !first_two_match {
        Some(format!("one or two parameters of type `{}`", using_ty.display(gcx)))
    } else {
        None
    };
    if let Some(expected) = wrong_params {
        gcx.dcx()
            .err("wrong parameters in operator definition")
            .span(function.span)
            .span_note(entry.span, "function was used to implement an operator here")
            .help(format!(
                "function `{}` needs to have {expected} to be used for operator `{}`",
                gcx.item_name(function_id).as_str(),
                op.to_str()
            ))
            .emit();
    }

    let returns = function_ty.returns;
    let returns_bool = matches!(
        op,
        UserDefinableOperator::Eq
            | UserDefinableOperator::Ne
            | UserDefinableOperator::Lt
            | UserDefinableOperator::Le
            | UserDefinableOperator::Gt
            | UserDefinableOperator::Ge
    );
    let wrong_returns = if returns_bool {
        returns.len() != 1 || returns[0] != gcx.types.bool
    } else {
        returns.len() != 1 || returns[0] != using_ty
    };
    if wrong_returns {
        let expected = if returns_bool {
            "`bool`".to_string()
        } else {
            format!("`{}`", using_ty.display(gcx))
        };
        gcx.dcx()
            .err("wrong return parameters in operator definition")
            .span(function.span)
            .span_note(entry.span, "function was used to implement an operator here")
            .help(format!(
                "function `{}` needs to return {expected} to be used for operator `{}`",
                gcx.item_name(function_id).as_str(),
                op.to_str()
            ))
            .emit();
    }

    if matches!(params.len(), 1 | 2) {
        let matches =
            gcx.user_operator(using_ty, using.source, using.contract, op, params.len() == 1);
        if matches.len() >= 2 {
            gcx.dcx()
                .err(format!(
                    "user-defined {} operator `{}` has more than one definition matching the operand type visible in the current scope",
                    if params.len() == 1 { "unary" } else { "binary" },
                    op.to_str()
                ))
                .span(entry.span)
                .emit();
        }
    }
}

fn check_external_type_clashes(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    if gcx.hir.contract(contract_id).kind.is_library() {
        return;
    }

    let mut external_declarations: FxHashMap<B256, Vec<ItemId>> = FxHashMap::default();

    for item_id in gcx.hir.contract_item_ids(contract_id) {
        match gcx.hir.item(item_id) {
            Item::Function(f) if f.is_part_of_external_interface() => {
                let s = gcx.item_selector(item_id);
                external_declarations.entry(s).or_default().push(item_id);
            }
            _ => {}
        }
    }
    for items in external_declarations.values() {
        for (i, &item) in items.iter().enumerate() {
            if let Some(&dup) = items.iter().skip(i + 1).find(|&&other| {
                !same_external_params(gcx, gcx.type_of_item(item), gcx.type_of_item(other))
            }) {
                gcx.dcx()
                    .err(
                        "function overload clash during conversion to external types for arguments",
                    )
                    .code(error_code!(9914))
                    .span(gcx.item_span(item))
                    .span_help(gcx.item_span(dup), "other declaration is here")
                    .emit();
            }
        }
    }
}

fn check_payable_fallback_without_receive(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);

    if let Some(fallback) = contract.fallback {
        let fallback = gcx.hir.function(fallback);
        if fallback.state_mutability.is_payable()
            && contract.receive.is_none()
            && !gcx.interface_functions(contract_id).is_empty()
        {
            gcx.dcx()
                .warn("contract has a payable fallback function, but no receive ether function")
                .span(contract.name.span)
                .code(error_code!(3628))
                .span_suggestion(
                    fallback.keyword_span(),
                    "consider changing to",
                    "receive",
                    solar_interface::diagnostics::Applicability::MachineApplicable,
                )
                .emit();
        }
    }
}

/// Checks for definitions that have the same name and parameter types in the given scope.
fn check_duplicate_definitions(gcx: Gcx<'_>, scope: &Declarations) {
    let is_duplicate = |a: Declaration, b: Declaration| -> bool {
        let (Res::Item(a), Res::Item(b)) = (a.res, b.res) else { return false };
        if !a.matches(&b) {
            return false;
        }
        if !(a.is_function() || a.is_event()) {
            return false;
        }
        // Don't check inheritance since this check would be incorrect with virtual/override.
        if let (hir::ItemId::Function(f1), hir::ItemId::Function(f2)) = (a, b) {
            let f1 = gcx.hir.function(f1);
            let f2 = gcx.hir.function(f2);
            if f1.contract != f2.contract {
                return false;
            }
        }
        if !same_external_params(gcx, gcx.type_of_item(a), gcx.type_of_item(b)) {
            return false;
        }
        true
    };

    let mut reported = FxHashSet::default();
    for (_name, decls) in scope.iter() {
        if decls.len() <= 1 {
            continue;
        }
        reported.clear();
        for (i, &decl) in decls.iter().enumerate() {
            if reported.contains(&i) {
                continue;
            }

            let mut duplicates = Vec::new();
            for (j, &other_decl) in decls.iter().enumerate().skip(i + 1) {
                if is_duplicate(decl, other_decl) {
                    reported.insert(j);
                    duplicates.push(other_decl.span);
                }
            }
            if !duplicates.is_empty() {
                let msg = format!(
                    "{} with same name and parameter types declared twice",
                    decl.description()
                );
                let mut err = gcx.dcx().err(msg).span(decl.span);
                for duplicate in duplicates {
                    err = err.span_note(duplicate, "other declaration");
                }
                err.emit();
            }
        }
    }
}

fn same_external_params<'gcx>(gcx: Gcx<'gcx>, a: Ty<'gcx>, b: Ty<'gcx>) -> bool {
    let key = |ty: Ty<'gcx>| ty.as_externally_callable_function(gcx).parameters().unwrap();
    key(a) == key(b)
}

fn check_unimplemented_functions(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);

    for f_id in contract.functions() {
        let f = gcx.hir.function(f_id);

        if f.marked_virtual && f.visibility == Visibility::Private {
            gcx.dcx()
                .err("`virtual` and `private` cannot be used together")
                .code(error_code!(3942))
                .span(f.span)
                .emit();
        }

        if f.body.is_some() {
            continue;
        }

        if f.kind.is_constructor() {
            gcx.dcx()
                .err("constructor must be implemented if declared")
                .code(error_code!(5700))
                .span(f.span)
                .emit();
        } else if contract.kind.is_library() {
            gcx.dcx()
                .err("library functions must be implemented if declared")
                .code(error_code!(9231))
                .span(f.span)
                .emit();
        } else if !f.virtual_ {
            gcx.dcx()
                .err("functions without implementation must be marked virtual")
                .code(error_code!(5424))
                .span(f.span)
                .emit();
        }
    }
}

fn check_receive_function(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);

    // Libraries cannot have receive functions
    if contract.kind.is_library() {
        if let Some(receive) = contract.receive {
            gcx.dcx()
                .err("libraries cannot have receive ether functions")
                .span(gcx.item_span(receive))
                .emit();
        }
        return;
    }
    if let Some(receive) = contract.receive {
        let f = gcx.hir.function(receive);
        // Check visibility
        if f.visibility != Visibility::External {
            gcx.dcx()
                .err("receive ether function must be defined as `external`")
                .span(gcx.item_span(receive))
                .emit();
        }

        // Check state mutability
        if f.state_mutability != StateMutability::Payable {
            gcx.dcx()
                .err("receive ether function must be payable")
                .span(gcx.item_span(receive))
                .help("add `payable` state mutability")
                .emit();
        }

        // Check parameters
        if !f.parameters.is_empty() {
            gcx.dcx()
                .err("receive ether function cannot take parameters")
                .span(gcx.item_span(receive))
                .emit();
        }

        // Check return values
        if !f.returns.is_empty() {
            gcx.dcx()
                .err("receive ether function cannot return values")
                .span(gcx.item_span(receive))
                .emit();
        }
    }
}

/// Checks for violation of maximum storage size to ensure slot allocation algorithms works.
///
/// Reference: <https://github.com/argotorg/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/analysis/ContractLevelChecker.cpp#L556C1-L570C2>
fn check_storage_size_upper_bound(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let span = gcx.hir.contract(contract_id).name.span;
    let Some(total_size) = storage_size_upper_bound(gcx, contract_id) else {
        gcx.dcx().err("contract requires too much storage").span(span).emit();
        return;
    };

    if gcx.sess.opts.unstable.print_max_storage_sizes {
        let full_contract_name = gcx.contract_fully_qualified_name(contract_id);
        gcx.dcx()
            .note(format!("{full_contract_name} requires a maximum of {total_size} storage slots"))
            .span(span)
            .emit();
    }
}

fn storage_size_upper_bound(gcx: Gcx<'_>, contract_id: hir::ContractId) -> Option<U256> {
    let mut total_size = U256::ZERO;
    for item_id in gcx.hir.contract_item_ids(contract_id) {
        // Skip constant and immutable variables
        if let hir::Item::Variable(var) = gcx.hir.item(item_id)
            && !(var.is_constant() || var.is_immutable())
        {
            let ty = gcx.type_of_item(item_id);
            total_size = total_size.checked_add(ty_storage_size_upper_bound(ty, gcx)?)?;
        }
    }
    Some(total_size)
}

fn ty_storage_size_upper_bound(ty: Ty<'_>, gcx: Gcx<'_>) -> Option<U256> {
    match ty.kind {
        TyKind::Elementary(..)
        | TyKind::StringLiteral(..)
        | TyKind::IntLiteral(..)
        | TyKind::Mapping(..)
        | TyKind::Contract(..)
        | TyKind::Udvt(..)
        | TyKind::Enum(..)
        | TyKind::FnPtr(..)
        | TyKind::DynArray(..) => Some(U256::from(1)),
        TyKind::Ref(ty, _) => ty_storage_size_upper_bound(ty, gcx),
        TyKind::Array(ty, uint) => {
            // https://github.com/argotorg/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L1800C1-L1806C2
            let elem_size = ty_storage_size_upper_bound(ty, gcx)?;
            uint.checked_mul(elem_size)
        }
        TyKind::Struct(struct_id) => {
            // https://github.com/argotorg/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L2303C1-L2309C2
            let mut total_size = U256::from(1);
            for t in gcx.struct_field_types(struct_id) {
                let size_contribution = ty_storage_size_upper_bound(*t, gcx)?;
                total_size = total_size.checked_add(size_contribution)?;
            }
            Some(total_size)
        }

        TyKind::Slice(..)
        | TyKind::Type(..)
        | TyKind::Tuple(..)
        | TyKind::Module(..)
        | TyKind::BuiltinModule(..)
        | TyKind::Event(..)
        | TyKind::Meta(..)
        | TyKind::Err(..)
        | TyKind::Error(..) => {
            unreachable!()
        }
    }
}

fn check_break_continue(gcx: Gcx<'_>, source: hir::SourceId) {
    let mut checker = BreakContinueChecker::new(gcx);
    let _ = checker.visit_nested_source(source);
}

struct BreakContinueChecker<'gcx> {
    gcx: Gcx<'gcx>,
    loop_depth: u32,
}

impl<'gcx> BreakContinueChecker<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, loop_depth: 0 }
    }

    fn visit_block(&mut self, block: hir::Block<'gcx>) -> ControlFlow<Never> {
        for stmt in block.stmts {
            self.visit_stmt(stmt)?;
        }
        ControlFlow::Continue(())
    }

    fn check_break_continue(&self, span: Span, kind: &str) {
        if self.loop_depth == 0 {
            let msg = format!("`{kind}` outside of a loop");
            self.gcx.sess.dcx.err(msg).span(span).emit();
        }
    }
}

impl<'gcx> Visit<'gcx> for BreakContinueChecker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_function(&mut self, id: hir::FunctionId) -> ControlFlow<Self::BreakValue> {
        let loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let r = self.visit_function(self.hir().function(id));
        self.loop_depth = loop_depth;
        r
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        match stmt.kind {
            hir::StmtKind::Break => self.check_break_continue(stmt.span, "break"),
            hir::StmtKind::Continue => self.check_break_continue(stmt.span, "continue"),
            hir::StmtKind::Loop(block, _) => {
                self.loop_depth += 1;
                let r = self.visit_block(block);
                self.loop_depth -= 1;
                return r;
            }
            _ => {}
        }

        self.walk_stmt(stmt)
    }

    // Statements don't appear in expressions. Short-circuit to avoid walking the full tree.
    #[inline]
    fn visit_expr(&mut self, _expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }
}
