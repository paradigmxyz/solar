use crate::{
    builtins::{Builtin, Builtin::*},
    hir::{self, ExprKind, ItemId, StmtKind, Visit},
    ty::{Gcx, TyKind},
};
use rayon::prelude::*;
use solar_ast::{DataLocation, StateMutability, Visibility};
use solar_data_structures::{Never, map::FxHashMap};
use solar_interface::{
    Span,
    diagnostics::{Diag, Level},
    error_code,
};
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }
    let yul_functions = collect_yul_functions(gcx);
    let modifier_mutability = gcx
        .hir
        .par_functions_enumerated()
        .filter_map(|(id, function)| {
            function.kind.is_modifier().then(|| {
                (id, ViewPureChecker::new(gcx, None, &yul_functions).infer_modifier(function))
            })
        })
        .collect::<FxHashMap<_, _>>();
    let diagnostics = gcx
        .hir
        .par_functions()
        .map(|function| {
            if function.kind.is_modifier() || function.is_getter() || function.is_yul {
                Vec::new()
            } else {
                ViewPureChecker::new(gcx, Some(&modifier_mutability), &yul_functions)
                    .check_function(function)
            }
        })
        .collect::<Vec<_>>();
    for diagnostic in diagnostics.into_iter().flatten() {
        let _ = gcx.dcx().emit_diagnostic(diagnostic);
    }
}

#[derive(Clone, Copy)]
struct MutabilityAndLocation {
    mutability: StateMutability,
    location: Span,
}

type BlockKey = (Span, usize, usize);
type YulFunctions<'gcx> = FxHashMap<BlockKey, Vec<&'gcx hir::Function<'gcx>>>;

struct BlockCollector<'gcx> {
    hir: &'gcx hir::Hir<'gcx>,
    blocks: Vec<hir::Block<'gcx>>,
}

impl<'gcx> Visit<'gcx> for BlockCollector<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        self.hir
    }

    fn visit_block(&mut self, block: hir::Block<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.blocks.push(block);
        self.walk_block(block)
    }

    fn visit_expr(&mut self, _expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }
}

fn collect_yul_functions(gcx: Gcx<'_>) -> YulFunctions<'_> {
    let mut collector = BlockCollector { hir: &gcx.hir, blocks: Vec::new() };
    for (_, function) in gcx.hir.functions_enumerated() {
        if let Some(body) = function.body {
            let _ = collector.visit_block(body);
        }
    }

    let mut functions = YulFunctions::default();
    for (_, function) in gcx.hir.functions_enumerated().filter(|(_, function)| function.is_yul) {
        let block = collector
            .blocks
            .iter()
            .filter(|block| {
                block.span.contains(function.span)
                    && !block.stmts.iter().any(|stmt| stmt.span.contains(function.span))
            })
            .min_by_key(|block| block.span.hi().to_u32() - block.span.lo().to_u32());
        if let Some(&block) = block {
            functions.entry(block_key(block)).or_default().push(function);
        }
    }
    for functions in functions.values_mut() {
        functions.sort_by_key(|function| function.span.lo());
    }
    functions
}

fn block_key(block: hir::Block<'_>) -> BlockKey {
    // Lowered Yul blocks can share spans, so include the statement slice identity.
    (block.span, block.stmts.as_ptr() as usize, block.stmts.len())
}

struct ViewPureChecker<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    current_function: Option<&'gcx hir::Function<'gcx>>,
    best: MutabilityAndLocation,
    modifier_mutability: Option<&'a FxHashMap<hir::FunctionId, MutabilityAndLocation>>,
    yul_functions: &'a YulFunctions<'gcx>,
    writing: bool,
    diagnostics: Vec<Diag>,
}

impl<'gcx, 'a> ViewPureChecker<'gcx, 'a> {
    fn new(
        gcx: Gcx<'gcx>,
        modifier_mutability: Option<&'a FxHashMap<hir::FunctionId, MutabilityAndLocation>>,
        yul_functions: &'a YulFunctions<'gcx>,
    ) -> Self {
        Self {
            gcx,
            current_function: None,
            best: MutabilityAndLocation {
                mutability: StateMutability::Pure,
                location: Span::DUMMY,
            },
            modifier_mutability,
            yul_functions,
            writing: false,
            diagnostics: Vec::new(),
        }
    }

    fn infer_modifier(mut self, function: &'gcx hir::Function<'gcx>) -> MutabilityAndLocation {
        self.best =
            MutabilityAndLocation { mutability: StateMutability::Pure, location: function.span };
        let _ = self.visit_function(function);
        self.best
    }

    fn check_function(mut self, function: &'gcx hir::Function<'gcx>) -> Vec<Diag> {
        let has_body = function.body.is_some_and(|body| !body.is_empty());
        let can_suggest = function.state_mutability != StateMutability::Payable
            && has_body
            && !function.is_constructor()
            && !function.is_special()
            && !function.virtual_;
        let must_validate = mutability_rank(function.state_mutability)
            <= mutability_rank(StateMutability::View)
            || function.is_constructor()
            || function.visibility >= Visibility::Public;
        if function.state_mutability == StateMutability::Payable
            || !has_body && function.modifiers.is_empty()
            || !can_suggest && !must_validate
        {
            return self.diagnostics;
        }

        self.current_function = Some(function);
        self.best =
            MutabilityAndLocation { mutability: StateMutability::Pure, location: function.span };
        let _ = self.visit_function(function);

        let suggested_mutability = self.best.mutability;
        if can_suggest
            && mutability_rank(suggested_mutability) < mutability_rank(function.state_mutability)
        {
            let mut diagnostic = Diag::new(
                Level::Warning,
                format!("function state mutability can be restricted to {suggested_mutability}"),
            );
            diagnostic.code(error_code!(2018)).span(function.span);
            self.diagnostics.push(diagnostic);
        }
        self.diagnostics
    }

    fn visit_expr_with_writing(&mut self, expr: &'gcx hir::Expr<'gcx>, writing: bool) {
        let previous = std::mem::replace(&mut self.writing, writing);
        let _ = self.visit_expr(expr);
        self.writing = previous;
    }

    fn visit_block_in_source_order(&mut self, block: hir::Block<'gcx>) -> ControlFlow<Never> {
        let functions = self.yul_functions.get(&block_key(block)).map(Vec::as_slice).unwrap_or(&[]);
        let mut stmt_index = 0;
        let mut function_index = 0;
        while stmt_index < block.stmts.len() || function_index < functions.len() {
            match (block.stmts.get(stmt_index), functions.get(function_index).copied()) {
                (Some(stmt), Some(function)) if stmt.span.lo() <= function.span.lo() => {
                    let _ = self.visit_stmt(stmt);
                    stmt_index += 1;
                }
                (_, Some(function)) => {
                    if let Some(body) = function.body {
                        self.visit_block(body)?;
                    }
                    function_index += 1;
                }
                (Some(stmt), None) => {
                    let _ = self.visit_stmt(stmt);
                    stmt_index += 1;
                }
                (None, None) => break,
            }
        }
        ControlFlow::Continue(())
    }

    fn report_call_expr(&mut self, expr: &'gcx hir::Expr<'gcx>, callee: &'gcx hir::Expr<'gcx>) {
        let calls_yul_function = self
            .gcx
            .resolved_callee(callee.id)
            .and_then(|callee| callee.res.as_function())
            .is_some_and(|id| self.gcx.hir.function(id).is_yul);
        if calls_yul_function {
            // Yul function definitions are checked as part of their containing assembly.
        } else if let Some(builtin) = self.gcx.builtin_callee(callee.id) {
            if builtin.is_yul() {
                self.report_yul_builtin(builtin, expr.span);
            } else if matches!(
                builtin,
                Builtin::ArrayPush0 | Builtin::ArrayPush | Builtin::ArrayPop
            ) {
                self.report(StateMutability::NonPayable, expr.span, None);
            } else if let Some(mutability) =
                self.gcx.type_of_expr(callee.id).and_then(|ty| ty.state_mutability())
            {
                self.report_call(mutability, expr.span);
            }
        } else if let Some(mutability) =
            self.gcx.type_of_expr(callee.id).and_then(|ty| ty.state_mutability())
        {
            self.report_call(mutability, expr.span);
        }
    }

    fn report_operator_call(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        if let Some(callee) = self.gcx.resolved_callee(expr.id)
            && let Some(id) = callee.res.as_function()
        {
            self.report_call(self.gcx.hir.function(id).state_mutability, expr.span);
        }
    }

    fn report_res(&mut self, res: hir::Res, span: Span, writing: bool) {
        match res {
            hir::Res::Item(ItemId::Variable(id)) => {
                let var = self.gcx.hir.variable(id);
                if var.is_immutable() {
                    let literal_initializer = var.initializer.is_some_and(|expr| {
                        self.gcx
                            .type_of_expr(expr.id)
                            .is_some_and(|ty| matches!(ty.kind, TyKind::IntLiteral(..)))
                    });
                    if !literal_initializer {
                        self.report(StateMutability::View, span, None);
                    }
                } else if var.is_state_variable() && !var.is_constant() {
                    self.report(read_or_write(writing), span, None);
                }
            }
            hir::Res::Builtin(Builtin::This) => self.report(StateMutability::View, span, None),
            _ => {}
        }
    }

    fn report_member(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        receiver: &'gcx hir::Expr<'gcx>,
        writing: bool,
    ) {
        let Some(res) = self.gcx.resolved_member(expr.id) else {
            if self.in_storage(receiver) {
                self.report(read_or_write(writing), expr.span, None);
            }
            return;
        };
        match res {
            hir::Res::Item(ItemId::Variable(id)) => {
                let var = self.gcx.hir.variable(id);
                if var.is_state_variable() && !var.is_constant() || self.in_storage(receiver) {
                    self.report(read_or_write(writing), expr.span, None);
                }
            }
            hir::Res::Builtin(Builtin::MsgValue) => {
                self.report(StateMutability::Payable, expr.span, None);
            }
            hir::Res::Builtin(
                Builtin::AddressBalance
                | Builtin::AddressCode
                | Builtin::AddressCodehash
                | Builtin::BlockCoinbase
                | Builtin::BlockTimestamp
                | Builtin::BlockDifficulty
                | Builtin::BlockPrevrandao
                | Builtin::BlockNumber
                | Builtin::BlockGaslimit
                | Builtin::BlockChainid
                | Builtin::BlockBasefee
                | Builtin::BlockBlobbasefee
                | Builtin::MsgSender
                | Builtin::MsgGas
                | Builtin::TxOrigin
                | Builtin::TxGasPrice,
            ) => self.report(StateMutability::View, expr.span, None),
            hir::Res::Builtin(Builtin::ArrayLength) if self.is_dynamic_storage(receiver) => {
                self.report(StateMutability::View, expr.span, None);
            }
            _ => {}
        }
    }

    fn report_yul_builtin(&mut self, builtin: Builtin, span: Span) {
        let mutability = match builtin {
            YulSstore | YulTstore | YulLog0 | YulLog1 | YulLog2 | YulLog3 | YulLog4 | YulCreate
            | YulCreate2 | YulCall | YulCallcode | YulDelegatecall | YulSelfdestruct
            | YulExtcall | YulExtdelegatecall => StateMutability::NonPayable,
            YulSload | YulTload | YulGas | YulAddress | YulBalance | YulSelfbalance | YulCaller
            | YulExtcodesize | YulExtcodecopy | YulExtcodehash | YulStaticcall
            | YulExtstaticcall | YulChainid | YulBasefee | YulBlobbasefee | YulBlobhash
            | YulCoinbase | YulDifficulty | YulPrevrandao | YulGaslimit | YulNumber
            | YulTimestamp | YulGasprice | YulOrigin | YulBlockhash => StateMutability::View,
            YulCallvalue => StateMutability::View,
            _ => StateMutability::Pure,
        };
        self.report(mutability, span, None);
    }

    fn report_call(&mut self, mutability: StateMutability, span: Span) {
        let mutability = if mutability == StateMutability::Payable {
            StateMutability::NonPayable
        } else {
            mutability
        };
        self.report(mutability, span, None);
    }

    fn report(
        &mut self,
        mutability: StateMutability,
        location: Span,
        nested_location: Option<Span>,
    ) {
        if mutability_rank(mutability) > mutability_rank(self.best.mutability) {
            self.best = MutabilityAndLocation { mutability, location };
        }

        let Some(function) = self.current_function else { return };
        if mutability_rank(mutability) <= mutability_rank(function.state_mutability) {
            return;
        }
        if mutability == StateMutability::View
            || mutability == StateMutability::Payable
                && function.state_mutability == StateMutability::Pure
        {
            let mut diagnostic = Diag::new(
                Level::Error,
                "function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`",
            );
            diagnostic.code(error_code!(2527)).span(location);
            self.diagnostics.push(diagnostic);
        } else if mutability == StateMutability::NonPayable {
            let mut diagnostic = Diag::new(
                Level::Error,
                format!(
                    "function cannot be declared as {} because this expression (potentially) modifies the state",
                    function.state_mutability
                ),
            );
            diagnostic.code(error_code!(8961)).span(location);
            self.diagnostics.push(diagnostic);
        } else if mutability == StateMutability::Payable
            && (function.is_constructor() || function.visibility >= Visibility::Public)
            && !function.contract.is_some_and(|id| self.gcx.hir.contract(id).kind.is_library())
        {
            if let Some(nested_location) = nested_location {
                let mut diagnostic = Diag::new(
                    Level::Error,
                    if function.is_constructor() {
                        "this modifier uses `msg.value` or `callvalue()` and thus the constructor has to be payable"
                    } else {
                        "this modifier uses `msg.value` or `callvalue()` and thus the function has to be payable or internal"
                    },
                );
                diagnostic.code(error_code!(4006)).span(location).span_note(
                    nested_location,
                    "`msg.value` or `callvalue()` appear here inside the modifier",
                );
                self.diagnostics.push(diagnostic);
            } else {
                let mut diagnostic = Diag::new(
                    Level::Error,
                    if function.is_constructor() {
                        "`msg.value` and `callvalue()` can only be used in payable constructors; make the constructor `payable` to avoid this error"
                    } else {
                        "`msg.value` and `callvalue()` can only be used in payable public functions; make the function `payable` or use an internal function to avoid this error"
                    },
                );
                diagnostic.code(error_code!(5887)).span(location);
                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn in_storage(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.type_of_expr(expr.id).is_some_and(|ty| {
            matches!(ty.loc(), Some(DataLocation::Storage | DataLocation::Transient))
        })
    }

    fn is_dynamic_storage(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.type_of_expr(expr.id).is_some_and(|ty| {
            matches!(ty.loc(), Some(DataLocation::Storage | DataLocation::Transient))
                && ty.peel_refs().is_dynamically_sized()
        })
    }

    fn is_this_function_selector(&self, expr: &hir::Expr<'_>) -> bool {
        if self.gcx.builtin_member(expr.id) != Some(Builtin::FunctionSelector) {
            return false;
        }
        let ExprKind::Member(receiver, _) = expr.kind else { return false };
        let ExprKind::Member(base, _) = receiver.peel_parens().kind else { return false };
        matches!(base.peel_parens().kind, ExprKind::Ident([hir::Res::Builtin(Builtin::This)]))
    }
}

impl<'gcx, 'a> Visit<'gcx> for ViewPureChecker<'gcx, 'a> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        self.walk_modifier(modifier)?;
        if let Some(modifier_mutability) = self.modifier_mutability
            && let ItemId::Function(id) = modifier.id
            && let Some(&inferred) = modifier_mutability.get(&id)
        {
            self.report(inferred.mutability, modifier.span, Some(inferred.location));
        }
        ControlFlow::Continue(())
    }

    fn visit_block(&mut self, block: hir::Block<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.visit_block_in_source_order(block)
    }

    fn visit_var(&mut self, var: &'gcx hir::Variable<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let Some(initializer) = var.initializer {
            self.visit_expr(initializer)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.walk_stmt(stmt)?;
        if let StmtKind::Emit(expr) = stmt.kind {
            let ExprKind::Call(callee, ref args, _) = expr.kind else { unreachable!() };
            self.report(StateMutability::NonPayable, callee.span.to(args.span), None);
        }
        ControlFlow::Continue(())
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        let writing = std::mem::replace(&mut self.writing, false);
        match expr.kind {
            ExprKind::Assign(lhs, _, rhs) => {
                self.visit_expr_with_writing(lhs, true);
                self.visit_expr_with_writing(rhs, false);
            }
            ExprKind::Delete(inner) => self.visit_expr_with_writing(inner, true),
            ExprKind::Member(_, _) if self.is_this_function_selector(expr) => {}
            ExprKind::Ternary(cond, then_, else_) => {
                self.visit_expr_with_writing(cond, false);
                self.visit_expr_with_writing(then_, writing);
                self.visit_expr_with_writing(else_, writing);
            }
            ExprKind::Tuple(exprs) => {
                for expr in exprs.iter().flatten() {
                    self.visit_expr_with_writing(expr, writing);
                }
            }
            ExprKind::Unary(op, inner) => {
                self.visit_expr_with_writing(inner, op.kind.has_side_effects());
            }
            ExprKind::YulMember(_, _)
            | ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Type(_) => {}
            _ => {
                let _ = self.walk_expr(expr);
            }
        }

        match expr.kind {
            ExprKind::Binary(_, _, _) | ExprKind::Unary(_, _) => self.report_operator_call(expr),
            ExprKind::Call(callee, _, _) => self.report_call_expr(expr, callee),
            ExprKind::Ident(resolutions) => {
                let mut variables = resolutions.iter().filter(|res| res.as_variable().is_some());
                if let Some(variable) = variables.next()
                    && variables.next().is_none()
                {
                    self.report_res(*variable, expr.span, writing);
                } else if let [res] = resolutions {
                    self.report_res(*res, expr.span, writing);
                }
            }
            ExprKind::Index(base, _) | ExprKind::Slice(base, _, _) if self.in_storage(base) => {
                self.report(read_or_write(writing), expr.span, None);
            }
            ExprKind::Member(receiver, _) if !self.is_this_function_selector(expr) => {
                self.report_member(expr, receiver, writing);
            }
            _ => {}
        }
        self.writing = writing;
        ControlFlow::Continue(())
    }
}

fn read_or_write(writing: bool) -> StateMutability {
    if writing { StateMutability::NonPayable } else { StateMutability::View }
}

fn mutability_rank(mutability: StateMutability) -> u8 {
    match mutability {
        StateMutability::Pure => 0,
        StateMutability::View => 1,
        StateMutability::NonPayable => 2,
        StateMutability::Payable => 3,
    }
}
