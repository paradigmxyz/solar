use super::{EnumVariantSize, Stats};
use crate::hir::{self, Visit as HirVisit};
use solar_data_structures::{Never, map::FxHashSet};
use std::ops::ControlFlow;

/// HIR stat collector.
struct HirStatCollector<'hir> {
    hir: &'hir hir::Hir<'hir>,
    stats: Stats,
    seen_items: FxHashSet<hir::ItemId>,
    seen_vars: FxHashSet<hir::VariableId>,
}

impl EnumVariantSize for hir::StmtKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::DeclSingle(var) => variant_payload_size!(self, var),
            Self::DeclMulti(vars, expr) => variant_payload_size!(self, vars, expr),
            Self::Block(block) => variant_payload_size!(self, block),
            Self::UncheckedBlock(block) => variant_payload_size!(self, block),
            Self::AssemblyBlock(block) => variant_payload_size!(self, block),
            Self::Emit(expr) => variant_payload_size!(self, expr),
            Self::Revert(expr) => variant_payload_size!(self, expr),
            Self::Return(expr) => variant_payload_size!(self, expr),
            Self::Break | Self::Continue | Self::Placeholder => variant_payload_size!(self,),
            Self::Loop(block, source) => variant_payload_size!(self, block, source),
            Self::If(cond, true_, false_) => variant_payload_size!(self, cond, true_, false_),
            Self::Switch(switch) => variant_payload_size!(self, switch),
            Self::Try(try_) => variant_payload_size!(self, try_),
            Self::Expr(expr) => variant_payload_size!(self, expr),
            Self::Err(guar) => variant_payload_size!(self, guar),
        }
    }
}

impl EnumVariantSize for hir::ExprKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Array(exprs) => variant_payload_size!(self, exprs),
            Self::Assign(lhs, op, rhs) => variant_payload_size!(self, lhs, op, rhs),
            Self::Binary(lhs, op, rhs) => variant_payload_size!(self, lhs, op, rhs),
            Self::Call(expr, args, opts) => variant_payload_size!(self, expr, args, opts),
            Self::Delete(expr) => variant_payload_size!(self, expr),
            Self::Ident(res) => variant_payload_size!(self, res),
            Self::Index(expr, index) => variant_payload_size!(self, expr, index),
            Self::Slice(expr, start, end) => variant_payload_size!(self, expr, start, end),
            Self::Lit(lit) => variant_payload_size!(self, lit),
            Self::Member(expr, ident) => variant_payload_size!(self, expr, ident),
            Self::New(ty) => variant_payload_size!(self, ty),
            Self::Payable(expr) => variant_payload_size!(self, expr),
            Self::Ternary(cond, true_, false_) => variant_payload_size!(self, cond, true_, false_),
            Self::Tuple(exprs) => variant_payload_size!(self, exprs),
            Self::TypeCall(ty) | Self::Type(ty) => variant_payload_size!(self, ty),
            Self::Unary(op, expr) => variant_payload_size!(self, op, expr),
            Self::YulMember(expr, ident) => variant_payload_size!(self, expr, ident),
            Self::Err(guar) => variant_payload_size!(self, guar),
        }
    }
}

impl EnumVariantSize for hir::CallArgsKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Unnamed(exprs) => variant_payload_size!(self, exprs),
            Self::Named(args) => variant_payload_size!(self, args),
        }
    }
}

impl EnumVariantSize for hir::TypeKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Elementary(ty) => variant_payload_size!(self, ty),
            Self::Array(ty) => variant_payload_size!(self, ty),
            Self::Function(ty) => variant_payload_size!(self, ty),
            Self::Mapping(ty) => variant_payload_size!(self, ty),
            Self::Custom(item) => variant_payload_size!(self, item),
            Self::Err(guar) => variant_payload_size!(self, guar),
        }
    }
}

pub fn print_hir_stats<'hir>(hir: &'hir hir::Hir<'hir>, title: &str) {
    let mut collector = HirStatCollector {
        hir,
        stats: Stats::new(),
        seen_items: FxHashSet::default(),
        seen_vars: FxHashSet::default(),
    };
    collector.collect();
    collector.print(title);
}

impl<'hir> HirStatCollector<'hir> {
    fn collect(&mut self) {
        self.record("Hir", self.hir);
        for id in self.hir.source_ids() {
            let source = self.hir.source(id);
            self.record("Source", source);
            for using in source.usings {
                self.visit_using_directive(using);
            }
        }
        for id in self.hir.doc_ids() {
            self.record("Doc", self.hir.doc(id));
        }
        for id in self.hir.item_ids() {
            let _ = self.visit_nested_item(id);
        }
        for id in self.hir.variable_ids() {
            let _ = self.visit_nested_var(id);
        }
    }

    fn record<T: ?Sized>(&mut self, label: &'static str, val: &T) {
        self.stats.record(label, val);
    }

    fn record_variant<T: ?Sized>(
        &mut self,
        label1: &'static str,
        label2: &'static str,
        val: &T,
        variant_size: usize,
    ) {
        self.stats.record_variant(label1, label2, val, variant_size);
    }

    fn visit_using_directive(&mut self, using: &'hir hir::UsingDirective<'hir>) {
        self.record("UsingDirective", using);
        if let Some(ty) = &using.ty {
            let _ = self.visit_ty(ty);
        }
        for entry in using.entries {
            self.record("UsingEntry", entry);
        }
    }

    fn visit_block(&mut self, block: &'hir hir::Block<'hir>) -> ControlFlow<Never> {
        self.record("Block", block);
        for stmt in block.stmts {
            self.visit_stmt(stmt)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt_switch(&mut self, switch: &'hir hir::StmtSwitch<'hir>) -> ControlFlow<Never> {
        self.record("StmtSwitch", switch);
        self.visit_expr(switch.selector)?;
        for case in switch.cases {
            self.record("StmtSwitchCase", case);
            if let Some(lit) = case.constant {
                self.record("Lit", lit);
            }
            self.visit_block(&case.body)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt_try(&mut self, try_: &'hir hir::StmtTry<'hir>) -> ControlFlow<Never> {
        self.record("StmtTry", try_);
        self.visit_expr(&try_.expr)?;
        for clause in try_.clauses {
            self.record("TryCatchClause", clause);
            for &var in clause.args {
                self.visit_nested_var(var)?;
            }
            self.visit_block(&clause.block)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_call_options(&mut self, opts: &'hir hir::CallOptions<'hir>) -> ControlFlow<Never> {
        self.record("CallOptions", opts);
        for arg in opts.args {
            self.record("NamedArg", arg);
            self.visit_expr(&arg.value)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_type_array(&mut self, arr: &'hir hir::TypeArray<'hir>) -> ControlFlow<Never> {
        self.record("TypeArray", arr);
        self.visit_ty(&arr.element)?;
        if let Some(size) = arr.size {
            self.visit_expr(size)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_type_function(&mut self, func: &'hir hir::TypeFunction<'hir>) -> ControlFlow<Never> {
        self.record("TypeFunction", func);
        for &param in func.parameters {
            self.visit_nested_var(param)?;
        }
        for &ret in func.returns {
            self.visit_nested_var(ret)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_type_mapping(&mut self, map: &'hir hir::TypeMapping<'hir>) -> ControlFlow<Never> {
        self.record("TypeMapping", map);
        self.visit_ty(&map.key)?;
        self.visit_ty(&map.value)?;
        ControlFlow::Continue(())
    }

    fn print(&self, title: &str) {
        self.stats.print(title);
    }
}

macro_rules! record_hir_variants {
    (
        ($self:ident, $val:expr, $kind:expr, $mod:ident, $ty:ty, $tykind:ident),
        [$($variant:ident),*]
    ) => {
        let kind = &$kind;
        let variant_size = EnumVariantSize::variant_payload_size(kind);
        match kind {
            $(
                $mod::$tykind::$variant { .. } => {
                    $self.record_variant(
                        stringify!($ty),
                        stringify!($variant),
                        $val,
                        variant_size,
                    )
                }
            )*
        }
    };
}

impl<'hir> HirVisit<'hir> for HirStatCollector<'hir> {
    type BreakValue = Never;

    fn hir(&self) -> &'hir hir::Hir<'hir> {
        self.hir
    }

    fn visit_nested_item(&mut self, id: hir::ItemId) -> ControlFlow<Self::BreakValue> {
        if !self.seen_items.insert(id) {
            return ControlFlow::Continue(());
        }
        self.visit_item(self.hir.item(id))
    }

    fn visit_contract(
        &mut self,
        contract: &'hir hir::Contract<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("Contract", contract);
        if let Some(layout) = contract.layout {
            self.visit_expr(layout)?;
        }
        for base in contract.bases_args {
            self.visit_modifier(base)?;
        }
        for using in contract.usings {
            self.visit_using_directive(using);
        }
        for &item in contract.items {
            self.visit_nested_item(item)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_function(&mut self, func: &'hir hir::Function<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Function", func);
        for &param in func.parameters {
            self.visit_nested_var(param)?;
        }
        for modifier in func.modifiers {
            self.visit_modifier(modifier)?;
        }
        for &ret in func.returns {
            self.visit_nested_var(ret)?;
        }
        if let Some(body) = &func.body {
            self.visit_block(body)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_modifier(
        &mut self,
        modifier: &'hir hir::Modifier<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("Modifier", modifier);
        self.visit_call_args(&modifier.args)
    }

    fn visit_struct(&mut self, strukt: &'hir hir::Struct<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Struct", strukt);
        for &field in strukt.fields {
            self.visit_nested_var(field)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_enum(&mut self, enum_: &'hir hir::Enum<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Enum", enum_);
        for variant in enum_.variants {
            self.record("Ident", variant);
        }
        ControlFlow::Continue(())
    }

    fn visit_udvt(&mut self, udvt: &'hir hir::Udvt<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Udvt", udvt);
        self.visit_ty(&udvt.ty)
    }

    fn visit_error(&mut self, error: &'hir hir::Error<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Error", error);
        for &param in error.parameters {
            self.visit_nested_var(param)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_event(&mut self, event: &'hir hir::Event<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Event", event);
        for &param in event.parameters {
            self.visit_nested_var(param)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_nested_var(&mut self, id: hir::VariableId) -> ControlFlow<Self::BreakValue> {
        if !self.seen_vars.insert(id) {
            return ControlFlow::Continue(());
        }
        self.visit_var(self.hir.variable(id))
    }

    fn visit_var(&mut self, var: &'hir hir::Variable<'hir>) -> ControlFlow<Self::BreakValue> {
        self.record("Variable", var);
        self.visit_ty(&var.ty)?;
        if let Some(expr) = var.initializer {
            self.visit_expr(expr)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_expr(&mut self, expr: &'hir hir::Expr<'hir>) -> ControlFlow<Self::BreakValue> {
        record_hir_variants!(
            (self, expr, expr.kind, hir, Expr, ExprKind),
            [
                Array, Assign, Binary, Call, Delete, Ident, Index, Slice, Lit, Member, New,
                Payable, Ternary, Tuple, TypeCall, Type, Unary, YulMember, Err
            ]
        );
        match &expr.kind {
            hir::ExprKind::Call(expr, args, opts) => {
                self.visit_expr(expr)?;
                if let Some(opts) = opts {
                    self.visit_call_options(opts)?;
                }
                self.visit_call_args(args)?;
            }
            hir::ExprKind::Delete(expr)
            | hir::ExprKind::Member(expr, _)
            | hir::ExprKind::Payable(expr)
            | hir::ExprKind::Unary(_, expr)
            | hir::ExprKind::YulMember(expr, _) => self.visit_expr(expr)?,
            hir::ExprKind::Assign(lhs, _, rhs) | hir::ExprKind::Binary(lhs, _, rhs) => {
                self.visit_expr(lhs)?;
                self.visit_expr(rhs)?;
            }
            hir::ExprKind::Index(expr, index) => {
                self.visit_expr(expr)?;
                if let Some(index) = index {
                    self.visit_expr(index)?;
                }
            }
            hir::ExprKind::Slice(expr, start, end) => {
                self.visit_expr(expr)?;
                if let Some(start) = start {
                    self.visit_expr(start)?;
                }
                if let Some(end) = end {
                    self.visit_expr(end)?;
                }
            }
            hir::ExprKind::Ternary(cond, true_, false_) => {
                self.visit_expr(cond)?;
                self.visit_expr(true_)?;
                self.visit_expr(false_)?;
            }
            hir::ExprKind::Array(exprs) => {
                for expr in *exprs {
                    self.visit_expr(expr)?;
                }
            }
            hir::ExprKind::Tuple(exprs) => {
                exprs.iter().copied().flatten().try_for_each(|expr| self.visit_expr(expr))?;
            }
            hir::ExprKind::Lit(lit) => self.record("Lit", *lit),
            hir::ExprKind::New(ty) | hir::ExprKind::TypeCall(ty) | hir::ExprKind::Type(ty) => {
                self.visit_ty(ty)?;
            }
            hir::ExprKind::Ident(_) | hir::ExprKind::Err(_) => {}
        }
        ControlFlow::Continue(())
    }

    fn visit_call_args(
        &mut self,
        args: &'hir hir::CallArgs<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        record_hir_variants!(
            (self, args, args.kind, hir, CallArgs, CallArgsKind),
            [Unnamed, Named]
        );
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => {
                for expr in exprs {
                    self.visit_expr(expr)?;
                }
            }
            hir::CallArgsKind::Named(args) => {
                for arg in args {
                    self.record("NamedArg", arg);
                    self.visit_expr(&arg.value)?;
                }
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'hir hir::Stmt<'hir>) -> ControlFlow<Self::BreakValue> {
        record_hir_variants!(
            (self, stmt, stmt.kind, hir, Stmt, StmtKind),
            [
                DeclSingle,
                DeclMulti,
                Block,
                UncheckedBlock,
                AssemblyBlock,
                Emit,
                Revert,
                Return,
                Break,
                Continue,
                Loop,
                If,
                Switch,
                Try,
                Expr,
                Placeholder,
                Err
            ]
        );
        match &stmt.kind {
            hir::StmtKind::DeclSingle(var) => self.visit_nested_var(*var)?,
            hir::StmtKind::DeclMulti(vars, expr) => {
                for &var in *vars {
                    if let Some(var) = var {
                        self.visit_nested_var(var)?;
                    }
                }
                self.visit_expr(expr)?;
            }
            hir::StmtKind::Block(block)
            | hir::StmtKind::UncheckedBlock(block)
            | hir::StmtKind::AssemblyBlock(block)
            | hir::StmtKind::Loop(block, _) => self.visit_block(block)?,
            hir::StmtKind::Emit(expr) | hir::StmtKind::Revert(expr) => self.visit_expr(expr)?,
            hir::StmtKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(expr)?;
                }
            }
            hir::StmtKind::Break | hir::StmtKind::Continue => {}
            hir::StmtKind::If(cond, true_, false_) => {
                self.visit_expr(cond)?;
                self.visit_stmt(true_)?;
                if let Some(false_) = false_ {
                    self.visit_stmt(false_)?;
                }
            }
            hir::StmtKind::Switch(switch) => self.visit_stmt_switch(switch)?,
            hir::StmtKind::Try(try_) => self.visit_stmt_try(try_)?,
            hir::StmtKind::Expr(expr) => self.visit_expr(expr)?,
            hir::StmtKind::Placeholder | hir::StmtKind::Err(_) => {}
        }
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, ty: &'hir hir::Type<'hir>) -> ControlFlow<Self::BreakValue> {
        record_hir_variants!(
            (self, ty, ty.kind, hir, Type, TypeKind),
            [Elementary, Array, Function, Mapping, Custom, Err]
        );
        match &ty.kind {
            hir::TypeKind::Array(arr) => self.visit_type_array(arr)?,
            hir::TypeKind::Function(func) => self.visit_type_function(func)?,
            hir::TypeKind::Mapping(map) => self.visit_type_mapping(map)?,
            hir::TypeKind::Elementary(_) | hir::TypeKind::Custom(_) | hir::TypeKind::Err(_) => {}
        }
        ControlFlow::Continue(())
    }
}
