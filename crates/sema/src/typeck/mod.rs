use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    hir::{self, Item, ItemId, Res, Visit},
    ty::{Gcx, SameSourceFileLevelUserTypeError, Ty, TyKind, TypeckResults},
};
use alloy_primitives::{B256, U256};
use rayon::prelude::*;
use solar_ast::{DataLocation, StateMutability, Visibility};
use solar_data_structures::{Never, bit_set::GrowableBitSet, map::FxIndexMap, parallel};
use solar_interface::{Span, diagnostics::ErrorGuaranteed, error_code};
use std::ops::ControlFlow;

mod checker;
pub(crate) mod override_checker;
mod udvt;

pub(crate) fn check(gcx: Gcx<'_>) {
    let mut typeck_results = TypeckResults::default();
    parallel!(gcx.sess, gcx.hir.par_contract_ids().for_each(|id| check_contract(gcx, id)), {
        typeck_results = gcx
            .hir
            .par_source_ids()
            .map(|id| {
                check_source(gcx, id);
                // TODO: Parallelize more.
                checker::check(gcx, id)
            })
            .reduce(TypeckResults::default, |mut a, b| {
                merge_typeck_results(gcx, &mut a, b);
                a
            });
    },);
    gcx.set_typeck_results(typeck_results);
}

fn check_contract(gcx: Gcx<'_>, id: hir::ContractId) {
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
}

fn check_source(gcx: Gcx<'_>, id: hir::SourceId) {
    check_duplicate_definitions(gcx, &gcx.symbol_resolver.source_scopes[id]);
    check_break_continue(gcx, id);
    for using in gcx.hir.source(id).usings {
        check_using_directive(gcx, using);
    }
}

fn merge_typeck_results<'gcx>(
    gcx: Gcx<'gcx>,
    results: &mut TypeckResults<'gcx>,
    new_results: TypeckResults<'gcx>,
) {
    for (id, ty) in new_results.expr_types {
        if let Some(prev_ty) = results.expr_types.insert(id, ty) {
            gcx.dcx()
                .bug(format!(
                    "expression {id:?} already has type {}; tried to register {}",
                    prev_ty.display(gcx),
                    ty.display(gcx),
                ))
                .emit();
        }
    }

    for (id, res) in new_results.resolved_callees {
        if let Some(prev_res) = results.resolved_callees.insert(id, res) {
            gcx.dcx()
                .bug(format!(
                    "expression {id:?} already has resolved callee {prev_res:?}; tried to register {res:?}",
                ))
                .emit();
        }
    }

    for (id, member) in new_results.resolved_members {
        if let Some(prev_member) = results.resolved_members.insert(id, member) {
            gcx.dcx()
                .bug(format!(
                    "expression {id:?} already has resolved member {prev_member:?}; tried to register {member:?}",
                ))
                .emit();
        }
    }

    for id in new_results.unsupported_udvt_operators.iter() {
        results.unsupported_udvt_operators.insert(id);
    }
}

fn check_using_directive<'gcx>(gcx: Gcx<'gcx>, using: &'gcx hir::UsingDirective<'gcx>) {
    let using_ty = gcx.type_of_using_directive(using);

    if using.global
        && let Some(ty) = using_ty
    {
        match ty.same_source_file_level_user_type(gcx, using.source) {
            Ok(()) => {}
            Err(SameSourceFileLevelUserTypeError::NotUserDefined) => {
                gcx.dcx().emit_err(using.span, "can only use `global` with user-defined types");
            }
            Err(SameSourceFileLevelUserTypeError::NotSameSourceFileLevel) => {
                gcx.dcx().emit_err(using.span, "can only use `global` with types defined in the same source unit at file level");
            }
        }
    }

    if !using.global
        && let Some(ty) = using_ty
        && let TyKind::Contract(id) = ty.kind
        && gcx.hir.contract(id).kind.is_library()
    {
        gcx.dcx().emit_err(using.span, "invalid use of library name");
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
            hir::UsingEntryKind::Err(_) => {}
        }
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
        gcx.dcx().emit_err(entry.span, "only file-level functions and library functions can be attached to a type in a `using` statement");
    }

    let TyKind::Fn(function_ty) = gcx.type_of_item(function_id.into()).kind else { unreachable!() };
    if function_ty.parameters.is_empty() {
        gcx.dcx().emit_err_span_note(
            entry.span,
            format!(
                "function `{}` does not have any parameters and therefore cannot be attached",
                gcx.item_name(function_id).as_str()
            ),
            function.span,
            "function defined here",
        );
        return;
    }

    if function.visibility == Visibility::Private && function.contract != using.contract {
        gcx.dcx().emit_err_span_note(entry.span, format!(
            "function `{}` is private and therefore cannot be attached to a type outside of the library where it is defined",
            gcx.item_name(function_id).as_str()
        ), function.span, "function defined here");
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
        udvt::check_using_operator(gcx, using, entry, function_id, function_ty, using_ty, op);
    }
}

fn check_external_type_clashes(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    if gcx.hir.contract(contract_id).kind.is_library() {
        return;
    }

    let mut external_declarations: FxIndexMap<B256, Vec<ItemId>> = FxIndexMap::default();

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

    let mut reported = GrowableBitSet::new_empty();
    for (_name, decls) in scope.iter() {
        if decls.len() <= 1 {
            continue;
        }
        reported.clear();
        for (i, &decl) in decls.iter().enumerate() {
            if reported.contains(i) {
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
    let key = |ty: Ty<'gcx>| ty.as_externally_callable_function(false, gcx).parameters().unwrap();
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
                .emit_err(gcx.item_span(receive), "libraries cannot have receive ether functions");
        }
        return;
    }
    if let Some(receive) = contract.receive {
        let f = gcx.hir.function(receive);
        // Check visibility
        if f.visibility != Visibility::External {
            gcx.dcx().emit_err(
                gcx.item_span(receive),
                "receive ether function must be defined as `external`",
            );
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
                .emit_err(gcx.item_span(receive), "receive ether function cannot take parameters");
        }

        // Check return values
        if !f.returns.is_empty() {
            gcx.dcx()
                .emit_err(gcx.item_span(receive), "receive ether function cannot return values");
        }
    }
}

/// Checks for violation of maximum storage size to ensure slot allocation algorithms works.
///
/// Reference: <https://github.com/argotorg/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/analysis/ContractLevelChecker.cpp#L556C1-L570C2>
fn check_storage_size_upper_bound(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let span = gcx.hir.contract(contract_id).name.span;
    let mut storage_size = None;
    let mut transient_storage_size = None;
    for location in [DataLocation::Storage, DataLocation::Transient] {
        let total_size = match storage_size_upper_bound(gcx, contract_id, location) {
            Ok(Some(total_size)) => total_size,
            Ok(None) => {
                let message = if location == DataLocation::Storage {
                    "contract requires too much storage"
                } else {
                    "contract requires too much transient storage"
                };
                gcx.dcx().emit_err(span, message);
                continue;
            }
            Err(_) => continue,
        };
        match location {
            DataLocation::Storage => storage_size = Some(total_size),
            DataLocation::Transient => transient_storage_size = Some(total_size),
            DataLocation::Memory | DataLocation::Calldata => unreachable!(),
        }
    }

    if gcx.sess.opts.unstable.print_max_storage_sizes
        && let (Some(storage_size), Some(transient_storage_size)) =
            (storage_size, transient_storage_size)
    {
        let full_contract_name = gcx.contract_fully_qualified_name(contract_id);
        gcx.dcx()
            .note(format!(
                "{full_contract_name} requires a maximum of {storage_size} storage slots"
            ))
            .span(span)
            .emit();
        gcx.dcx()
            .note(format!(
                "{full_contract_name} requires a maximum of {transient_storage_size} transient storage slots"
            ))
            .span(span)
            .emit();
    }
}

fn storage_size_upper_bound(
    gcx: Gcx<'_>,
    contract_id: hir::ContractId,
    location: DataLocation,
) -> Result<Option<U256>, ErrorGuaranteed> {
    let mut total_size = U256::ZERO;
    for item_id in gcx.hir.contract_item_ids(contract_id) {
        // Skip constant and immutable variables and variables from the other storage space.
        if let hir::Item::Variable(var) = gcx.hir.item(item_id)
            && !(var.is_constant() || var.is_immutable())
            && (var.data_location == Some(DataLocation::Transient))
                == (location == DataLocation::Transient)
        {
            let ty = gcx.type_of_item(item_id);
            let Some(size) = ty_storage_size_upper_bound(ty, gcx)? else { return Ok(None) };
            let Some(size) = total_size.checked_add(size) else { return Ok(None) };
            total_size = size;
        }
    }
    Ok(Some(total_size))
}

fn ty_storage_size_upper_bound(ty: Ty<'_>, gcx: Gcx<'_>) -> Result<Option<U256>, ErrorGuaranteed> {
    match ty.kind {
        TyKind::Elementary(..)
        | TyKind::StringLiteral(..)
        | TyKind::IntLiteral(..)
        | TyKind::Mapping(..)
        | TyKind::Contract(..)
        | TyKind::Super(..)
        | TyKind::Udvt(..)
        | TyKind::Enum(..)
        | TyKind::Fn(..)
        | TyKind::DynArray(..) => Ok(Some(U256::from(1))),
        TyKind::Ref(ty, _) => ty_storage_size_upper_bound(ty, gcx),
        TyKind::Array(ty, uint) => {
            // https://github.com/argotorg/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L1800C1-L1806C2
            let Some(elem_size) = ty_storage_size_upper_bound(ty, gcx)? else { return Ok(None) };
            Ok(uint.checked_mul(elem_size))
        }
        TyKind::Struct(struct_id) => {
            // https://github.com/argotorg/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L2303C1-L2309C2
            let mut total_size = U256::from(1);
            for t in gcx.struct_field_types(struct_id) {
                let Some(size_contribution) = ty_storage_size_upper_bound(*t, gcx)? else {
                    return Ok(None);
                };
                let Some(size) = total_size.checked_add(size_contribution) else {
                    return Ok(None);
                };
                total_size = size;
            }
            Ok(Some(total_size))
        }
        TyKind::Err(guar) => Err(guar),

        TyKind::Slice(..)
        | TyKind::Type(..)
        | TyKind::Tuple(..)
        | TyKind::Module(..)
        | TyKind::BuiltinModule(..)
        | TyKind::Variadic
        | TyKind::Event(..)
        | TyKind::Meta(..)
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
            self.gcx.sess.dcx.emit_err(span, msg);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Compiler,
        builtins::Builtin,
        hir::ExprKind,
        ty::{CallGas, CallKind, CallTermination},
    };
    use solar_data_structures::Never;
    use solar_interface::{Session, config::CompileOpts};
    use std::path::PathBuf;

    const SOURCE: &str = r#"
contract C {
    function f(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
"#;
    const SECOND_SOURCE: &str = r#"
contract D {
    function g(uint256 x) public pure returns (uint256) {
        return x * 2;
    }
}
"#;
    const QUERY_SOURCE: &str = r#"
library L {
    function attached(address, uint256) internal pure {}
}

using L for address;

struct Callbacks {
    function(uint256) internal callback;
}

contract C {
    function overloaded(uint256) internal {}
    function overloaded(bytes memory) public {}
    function first(uint256) internal {}
    function second(uint256) internal {}

    function query(
        function(uint256) internal callback,
        address target,
        Callbacks memory callbacks
    ) internal {
        ((target)).attached(1);
        callback(2);
        callbacks.callback(3);
        require(true);
        overloaded(1);
        function(uint256) internal selected = target == address(0) ? first : second;
        selected(4);
        uint256[] memory values = new uint256[](1);
        values;
    }
}
"#;
    const AMBIGUOUS_CALL_SOURCE: &str = r#"
contract C {
    function overloaded(uint256) internal {}
    function overloaded(int256) internal {}

    function query() internal {
        overloaded(1);
    }
}
"#;
    const CALL_INFO_SOURCE: &str = r#"
contract Target {
    function mutate() external {}
    function inspect() external view {}
}

contract C {
    function query(Target target, address payable recipient) public {
        target.mutate();
        target.inspect();
        recipient.call{gas: 10, value: 1}("");
        recipient.staticcall("");
        recipient.send(1);
        recipient.transfer(1);
        address(this).delegatecall("");
        new Target();
    }
}
"#;
    const YUL_CALL_INFO_SOURCE: &str = r#"
contract C {
    function query(address target) external {
        assembly {
            sstore(0, 1)
            tstore(0, 1)
            log0(0, 0)
            pop(create(0, 0, 0))
            pop(call(10, target, 1, 0, 0, 0, 0))
            pop(callcode(10, target, 1, 0, 0, 0, 0))
            pop(staticcall(gas(), target, 0, 0, 0, 0))
            stop()
        }
    }
}
"#;
    const DISPATCHED_CALL_SOURCE: &str = r#"
contract Root {
    uint256 rootState;
    uint256 constant ROOT_CONSTANT = 1;
    function target() internal virtual {}
    function entry(uint256) public virtual {}
    fallback() external virtual {}
    modifier guard() virtual { _; }
}

contract Override is Root {
    function target() internal view virtual override {}
    function entry(uint256) public virtual override {}
    modifier guard() virtual override { _; }
}

contract Caller is Root {
    function direct() internal { target(); }
    function next() internal { super.target(); }
}

contract Leaf is Override, Caller {
    uint256 leafState;
    function target() internal pure override(Root, Override) {}
    function entry(uint256) public override(Root, Override) {}
    modifier guard() override(Root, Override) { _; }
    function run() external guard { direct(); next(); }
    receive() external payable {}
}

contract Other {
    uint256 unrelatedState;
}
"#;

    struct FirstBinaryExpr<'hir> {
        hir: &'hir hir::Hir<'hir>,
    }

    impl<'hir> Visit<'hir> for FirstBinaryExpr<'hir> {
        type BreakValue = &'hir hir::Expr<'hir>;

        fn hir(&self) -> &'hir hir::Hir<'hir> {
            self.hir
        }

        fn visit_expr(&mut self, expr: &'hir hir::Expr<'hir>) -> ControlFlow<Self::BreakValue> {
            if matches!(expr.kind, ExprKind::Binary(..)) {
                ControlFlow::Break(expr)
            } else {
                self.walk_expr(expr)
            }
        }
    }

    struct CallExprs<'hir> {
        hir: &'hir hir::Hir<'hir>,
        calls: Vec<&'hir hir::Expr<'hir>>,
    }

    impl<'hir> Visit<'hir> for CallExprs<'hir> {
        type BreakValue = Never;

        fn hir(&self) -> &'hir hir::Hir<'hir> {
            self.hir
        }

        fn visit_expr(&mut self, expr: &'hir hir::Expr<'hir>) -> ControlFlow<Self::BreakValue> {
            if matches!(expr.kind, ExprKind::Call(..)) {
                self.calls.push(expr);
            }
            self.walk_expr(expr)
        }
    }

    fn binary_expr_types() -> Vec<Option<String>> {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), SOURCE).unwrap();
            pcx.add_file(file);
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("second.sol"), SECOND_SOURCE)
                .unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            gcx.hir
                .source_ids()
                .map(|source| {
                    let mut visitor = FirstBinaryExpr { hir: &gcx.hir };
                    let ControlFlow::Break(expr) = visitor.visit_nested_source(source) else {
                        panic!("missing binary expression")
                    };
                    gcx.type_of_expr(expr.id).map(|ty| ty.display(gcx).to_string())
                })
                .collect()
        })
    }

    #[test]
    fn expression_types_are_available_by_default() {
        assert_eq!(binary_expr_types(), [Some("uint256".to_string()), Some("uint256".into())]);
    }

    #[test]
    fn lint_query_helpers_preserve_call_semantics() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("query.sol"), QUERY_SOURCE)
                .unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let mut visitor = CallExprs { hir: &gcx.hir, calls: Vec::new() };
            let source = gcx.hir.source_ids().next().unwrap();
            assert_eq!(visitor.visit_nested_source(source), ControlFlow::Continue(()));
            let [attached_call, variable_call, field_call, builtin_call, ..] =
                visitor.calls.as_slice()
            else {
                panic!("expected at least four calls, got {}", visitor.calls.len())
            };

            let attached = gcx.resolved_call(attached_call).unwrap();
            assert!(attached.attached);
            assert!(attached.res.as_function().is_some());

            let variable = gcx.resolved_call(variable_call).unwrap();
            assert!(!variable.attached);
            assert!(variable.res.as_variable().is_some());
            assert!(gcx.call_info(variable_call).unwrap().is_indirect_internal());
            assert!(gcx.call_info(variable_call).unwrap().may_write_state());

            let field = gcx.resolved_call(field_call).unwrap();
            assert!(!field.attached);
            let field_id = field.res.as_variable().unwrap();
            assert!(matches!(gcx.hir.variable(field_id).parent, Some(hir::ItemId::Struct(_))));
            assert!(gcx.call_info(field_call).unwrap().is_indirect_internal());
            assert!(gcx.call_info(field_call).unwrap().may_write_state());

            let builtin = gcx.resolved_call(builtin_call).unwrap();
            assert!(!builtin.attached);
            assert_eq!(builtin.res.as_builtin(), Some(Builtin::Require));
            assert!(!gcx.call_info(builtin_call).unwrap().is_indirect_internal());
            assert!(!gcx.call_info(attached_call).unwrap().is_indirect_internal());
            let allocation = visitor
                .calls
                .iter()
                .copied()
                .find(|expr| {
                    let ExprKind::Call(callee, ..) = expr.kind else { return false };
                    matches!(callee.peel_parens().kind, ExprKind::New(_))
                })
                .unwrap();
            assert!(!gcx.call_info(allocation).unwrap().is_indirect_internal());

            let ExprKind::Call(attached_callee, ..) = attached_call.kind else { unreachable!() };
            let ExprKind::Member(receiver, _) = attached_callee.kind else { unreachable!() };
            assert_ne!(receiver.id, receiver.peel_parens().id);
            assert!(gcx.type_of_expr(receiver.id).is_some_and(|ty| ty.is_address()));
            assert!(gcx.types.address_payable.is_address());
            assert!(!gcx.types.bool.is_address());
            assert!(gcx.resolved_call(receiver).is_none());

            let references = gcx.function_reference_index();
            let overloaded = gcx.hir.function_ids().filter(|&id| {
                gcx.hir.function(id).name.is_some_and(|name| name.name.as_str() == "overloaded")
            });
            let mut selected = None;
            let mut unselected = None;
            for function in overloaded {
                if gcx.item_signature(hir::ItemId::Function(function)).contains("uint256") {
                    selected = Some(function);
                } else {
                    unselected = Some(function);
                }
            }
            assert!(references.is_internally_referenced(selected.unwrap()));
            assert!(!references.is_referenced(unselected.unwrap()));

            let targets = visitor
                .calls
                .iter()
                .find_map(|call| {
                    let targets = gcx.indirect_internal_call_targets(call);
                    (targets.known().len() == 2 && !targets.may_be_unknown()).then_some(targets)
                })
                .expect("selected function pointer should have two possible targets");
            let mut names = targets
                .known()
                .iter()
                .map(|target| gcx.hir.function(target.function()).name.unwrap().name.to_string())
                .collect::<Vec<_>>();
            names.sort_unstable();
            assert_eq!(names, ["first".to_string(), "second".to_string()]);
        });
    }

    #[test]
    fn resolved_call_preserves_failed_overload_resolution() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("ambiguous.sol"), AMBIGUOUS_CALL_SOURCE)
                .unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let mut visitor = CallExprs { hir: &gcx.hir, calls: Vec::new() };
            let source = gcx.hir.source_ids().next().unwrap();
            assert_eq!(visitor.visit_nested_source(source), ControlFlow::Continue(()));
            let [call] = visitor.calls.as_slice() else {
                panic!("expected one call, got {}", visitor.calls.len())
            };
            let ExprKind::Call(callee, ..) = call.kind else { unreachable!() };
            assert!(gcx.resolved_callee(callee.id).is_some_and(|resolved| resolved.res.is_err()));
            assert!(gcx.resolved_call(call).is_some_and(|resolved| resolved.res.is_err()));
        });
    }

    #[test]
    fn call_info_classifies_external_interactions() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("call_info.sol"), CALL_INFO_SOURCE)
                .unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let mut visitor = CallExprs { hir: &gcx.hir, calls: Vec::new() };
            let source = gcx.hir.source_ids().next().unwrap();
            assert_eq!(visitor.visit_nested_source(source), ControlFlow::Continue(()));
            let interactions = visitor
                .calls
                .iter()
                .filter_map(|&call| {
                    let info = gcx.call_info(call)?;
                    info.is_external_interaction()
                        .then_some(info.is_state_mutating_external_interaction())
                })
                .collect::<Vec<_>>();

            assert_eq!(interactions, [true, false, true, false, true, true, true, true]);

            let self_delegate = visitor.calls.iter().copied().find(|&call| {
                gcx.call_info(call).is_some_and(|info| info.kind() == CallKind::DelegateCall)
            });
            let self_delegate = self_delegate.unwrap();
            assert!(
                gcx.call_info(self_delegate).unwrap().may_transfer_native_value(gcx, self_delegate)
            );

            let address_call = visitor
                .calls
                .iter()
                .copied()
                .find(|&call| {
                    gcx.call_info(call).and_then(|info| info.builtin())
                        == Some(Builtin::AddressCall)
                })
                .unwrap();
            assert!(gcx.call_gas_limit(address_call).is_some());
            assert!(gcx.call_value(address_call).is_some());
            let gas = gcx.call_gas(address_call).unwrap();
            assert!(matches!(gas, CallGas::Explicit { value_stipend: true, .. }));
            assert!(gas.adds_value_stipend());
            assert!(gas.may_exceed(gcx, 2_309));
            assert!(!gas.may_exceed(gcx, 2_310));

            for builtin in [Builtin::AddressPayableSend, Builtin::AddressPayableTransfer] {
                let call = visitor
                    .calls
                    .iter()
                    .copied()
                    .find(|&call| {
                        gcx.call_info(call).and_then(|info| info.builtin()) == Some(builtin)
                    })
                    .unwrap();
                assert!(gcx.call_gas(call).is_some_and(CallGas::is_stipend));
                assert!(!gcx.call_gas(call).unwrap().may_exceed(gcx, CallGas::STIPEND));
            }

            let forwarded = visitor.calls.iter().copied().find(|&call| {
                gcx.call_info(call).and_then(|info| info.function()).is_some_and(|function| {
                    gcx.hir
                        .function(function)
                        .name
                        .is_some_and(|name| name.name.as_str() == "mutate")
                })
            });
            assert!(gcx.call_gas(forwarded.unwrap()).is_some_and(CallGas::is_forwarded));
        });
    }

    #[test]
    fn call_info_classifies_yul_effects_and_halts() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("yul_call_info.sol"), YUL_CALL_INFO_SOURCE)
                .unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let mut visitor = CallExprs { hir: &gcx.hir, calls: Vec::new() };
            let source = gcx.hir.source_ids().next().unwrap();
            assert_eq!(visitor.visit_nested_source(source), ControlFlow::Continue(()));

            let find = |builtin| {
                visitor
                    .calls
                    .iter()
                    .copied()
                    .find(|&call| {
                        gcx.call_info(call).and_then(|info| info.builtin()) == Some(builtin)
                    })
                    .unwrap()
            };
            let sstore = gcx.call_info(find(Builtin::YulSstore)).unwrap();
            assert!(sstore.may_mutate_state());
            assert!(!sstore.is_external_interaction());
            assert!(Builtin::YulSstore.is_persistent_state_write());

            let tstore = gcx.call_info(find(Builtin::YulTstore)).unwrap();
            assert!(tstore.may_mutate_state());
            assert!(Builtin::YulTstore.is_transient_state_write());

            let log = gcx.call_info(find(Builtin::YulLog0)).unwrap();
            assert!(log.may_mutate_state());
            assert!(Builtin::YulLog0.is_log());

            let call = gcx.call_info(find(Builtin::YulCall)).unwrap();
            assert!(call.is_state_mutating_external_interaction());
            assert_eq!(call.kind(), CallKind::Call);
            assert!(gcx.call_gas_limit(find(Builtin::YulCall)).is_some());
            let gas = gcx.call_gas(find(Builtin::YulCall)).unwrap();
            assert!(gas.adds_value_stipend());
            assert!(gas.may_exceed(gcx, 2_309));
            assert!(!gas.may_exceed(gcx, 2_310));
            assert_eq!(
                gcx.try_eval_const_u256_wrapping(gcx.call_value(find(Builtin::YulCall)).unwrap()),
                Some(alloy_primitives::U256::from(1))
            );
            let staticcall = gcx.call_info(find(Builtin::YulStaticcall)).unwrap();
            assert!(staticcall.is_external_interaction());
            assert!(!staticcall.is_state_mutating_external_interaction());
            assert_eq!(staticcall.kind(), CallKind::StaticCall);
            assert!(gcx.call_gas(find(Builtin::YulStaticcall)).is_some_and(CallGas::is_forwarded));

            let callcode_expr = find(Builtin::YulCallcode);
            let callcode = gcx.call_info(callcode_expr).unwrap();
            assert_eq!(callcode.kind(), CallKind::DelegateCall);
            assert!(gcx.call_value(callcode_expr).is_some());
            assert!(gcx.call_transferred_value(callcode_expr).is_none());
            let callcode_gas = gcx.call_gas(callcode_expr).unwrap();
            assert!(callcode_gas.adds_value_stipend());
            assert!(callcode_gas.may_exceed(gcx, 2_309));
            assert!(!callcode_gas.may_exceed(gcx, 2_310));

            let create = gcx.call_info(find(Builtin::YulCreate)).unwrap();
            assert!(create.is_contract_creation());
            assert_eq!(create.kind(), CallKind::Creation);

            assert_eq!(
                gcx.call_termination(find(Builtin::YulStop)),
                Some(CallTermination::SuccessfulHalt)
            );
        });
    }

    #[test]
    fn call_info_resolves_inherited_dispatch_context() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("dispatch.sol"), DISPATCHED_CALL_SOURCE)
                .unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let mut visitor = CallExprs { hir: &gcx.hir, calls: Vec::new() };
            let source = gcx.hir.source_ids().next().unwrap();
            assert_eq!(visitor.visit_nested_source(source), ControlFlow::Continue(()));
            let [direct_call, super_call, ..] = visitor.calls.as_slice() else {
                panic!("expected at least two calls, got {}", visitor.calls.len())
            };

            let leaf = gcx
                .hir
                .contract_ids()
                .find(|&id| gcx.hir.contract(id).name.name.as_str() == "Leaf")
                .unwrap();
            let function = |canonical_name: &str| {
                gcx.hir
                    .function_ids()
                    .filter(|&id| gcx.hir.function(id).name.is_some())
                    .find(|&id| gcx.item_canonical_name(id).to_string() == canonical_name)
                    .unwrap()
            };

            assert_eq!(
                gcx.call_info(direct_call).unwrap().function(),
                Some(function("Root.target"))
            );
            assert_eq!(
                gcx.call_info_in_contract(direct_call, leaf).unwrap().function(),
                Some(function("Leaf.target"))
            );
            assert!(!gcx.call_info_in_contract(direct_call, leaf).unwrap().may_mutate_state());
            assert_eq!(
                gcx.function_in_contract(function("Root.target"), leaf),
                function("Leaf.target")
            );
            assert_eq!(
                gcx.call_info(super_call).unwrap().function(),
                Some(function("Root.target"))
            );
            assert_eq!(
                gcx.call_info_in_contract(super_call, leaf).unwrap().function(),
                Some(function("Override.target"))
            );
            assert_eq!(
                gcx.modifier_in_contract(function("Root.guard"), leaf),
                function("Leaf.guard")
            );
            let runtime = gcx.runtime_entry_points(leaf);
            assert!(runtime.contains(&function("Leaf.run")));
            assert!(runtime.contains(&function("Leaf.entry")));
            assert!(runtime.iter().any(|&id| gcx.hir.function(id).kind.is_fallback()));
            assert!(runtime.iter().any(|&id| gcx.hir.function(id).kind.is_receive()));
            assert!(!runtime.contains(&function("Root.entry")));
            assert!(!runtime.contains(&function("Override.entry")));

            let state_names: Vec<_> = gcx
                .contract_mutable_state_variables(leaf)
                .iter()
                .filter_map(|&variable| gcx.hir.variable(variable).name)
                .map(|name| name.as_str().to_owned())
                .collect();
            assert_eq!(state_names, ["leafState", "rootState"]);
        });
    }
}
