use super::{EnumVariantSize, Stats};
use solar_ast::{self as ast, ItemId, visit::Visit, yul};
use solar_data_structures::{Never, map::FxHashSet};
use std::ops::ControlFlow;

/// AST stat collector.
struct StatCollector {
    stats: Stats,
    seen: FxHashSet<ItemId>,
}

impl EnumVariantSize for ast::ItemKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Pragma(pragma) => variant_payload_size!(self, pragma),
            Self::Import(import) => variant_payload_size!(self, import),
            Self::Using(using) => variant_payload_size!(self, using),
            Self::Contract(contract) => variant_payload_size!(self, contract),
            Self::Function(function) => variant_payload_size!(self, function),
            Self::Variable(var) => variant_payload_size!(self, var),
            Self::Struct(strukt) => variant_payload_size!(self, strukt),
            Self::Enum(enum_) => variant_payload_size!(self, enum_),
            Self::Udvt(udvt) => variant_payload_size!(self, udvt),
            Self::Error(error) => variant_payload_size!(self, error),
            Self::Event(event) => variant_payload_size!(self, event),
        }
    }
}

impl EnumVariantSize for ast::TypeKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Elementary(ty) => variant_payload_size!(self, ty),
            Self::Array(ty) => variant_payload_size!(self, ty),
            Self::Function(ty) => variant_payload_size!(self, ty),
            Self::Mapping(ty) => variant_payload_size!(self, ty),
            Self::Custom(path) => variant_payload_size!(self, path),
        }
    }
}

impl EnumVariantSize for ast::StmtKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Assembly(assembly) => variant_payload_size!(self, assembly),
            Self::DeclSingle(var) => variant_payload_size!(self, var),
            Self::DeclMulti(vars, expr) => variant_payload_size!(self, vars, expr),
            Self::Block(block) => variant_payload_size!(self, block),
            Self::Break | Self::Continue | Self::Placeholder => variant_payload_size!(self,),
            Self::DoWhile(body, cond) => variant_payload_size!(self, body, cond),
            Self::Emit(path, args) => variant_payload_size!(self, path, args),
            Self::Expr(expr) => variant_payload_size!(self, expr),
            Self::For { init, cond, next, body } => {
                variant_payload_size!(self, init, cond, next, body)
            }
            Self::If(cond, body, else_) => variant_payload_size!(self, cond, body, else_),
            Self::Return(expr) => variant_payload_size!(self, expr),
            Self::Revert(path, args) => variant_payload_size!(self, path, args),
            Self::Try(try_) => variant_payload_size!(self, try_),
            Self::UncheckedBlock(block) => variant_payload_size!(self, block),
            Self::While(cond, body) => variant_payload_size!(self, cond, body),
        }
    }
}

impl EnumVariantSize for ast::ExprKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Array(exprs) => variant_payload_size!(self, exprs),
            Self::Assign(lhs, op, rhs) => variant_payload_size!(self, lhs, op, rhs),
            Self::Binary(lhs, op, rhs) => variant_payload_size!(self, lhs, op, rhs),
            Self::Call(expr, args) => variant_payload_size!(self, expr, args),
            Self::CallOptions(expr, args) => variant_payload_size!(self, expr, args),
            Self::Delete(expr) => variant_payload_size!(self, expr),
            Self::Ident(ident) => variant_payload_size!(self, ident),
            Self::Index(expr, index) => variant_payload_size!(self, expr, index),
            Self::Lit(lit, denomination) => variant_payload_size!(self, lit, denomination),
            Self::Member(expr, ident) => variant_payload_size!(self, expr, ident),
            Self::New(ty) => variant_payload_size!(self, ty),
            Self::Payable(args) => variant_payload_size!(self, args),
            Self::Ternary(cond, true_expr, false_expr) => {
                variant_payload_size!(self, cond, true_expr, false_expr)
            }
            Self::Tuple(exprs) => variant_payload_size!(self, exprs),
            Self::TypeCall(ty) | Self::Type(ty) => variant_payload_size!(self, ty),
            Self::Unary(op, expr) => variant_payload_size!(self, op, expr),
        }
    }
}

impl EnumVariantSize for yul::StmtKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Block(block) => variant_payload_size!(self, block),
            Self::AssignSingle(path, expr) => variant_payload_size!(self, path, expr),
            Self::AssignMulti(paths, expr) => variant_payload_size!(self, paths, expr),
            Self::Expr(expr) => variant_payload_size!(self, expr),
            Self::If(expr, block) => variant_payload_size!(self, expr, block),
            Self::For(for_) => variant_payload_size!(self, for_),
            Self::Switch(switch) => variant_payload_size!(self, switch),
            Self::Leave | Self::Break | Self::Continue => variant_payload_size!(self,),
            Self::FunctionDef(function) => variant_payload_size!(self, function),
            Self::VarDecl(vars, init) => variant_payload_size!(self, vars, init),
        }
    }
}

impl EnumVariantSize for yul::ExprKind<'_> {
    fn variant_payload_size(&self) -> usize {
        match self {
            Self::Path(path) => variant_payload_size!(self, path),
            Self::Call(call) => variant_payload_size!(self, call),
            Self::Lit(lit) => variant_payload_size!(self, lit),
        }
    }
}

pub fn print_ast_stats<'ast>(ast: &'ast ast::SourceUnit<'ast>, title: &str) {
    let mut collector = StatCollector { stats: Stats::new(), seen: FxHashSet::default() };
    let _ = collector.visit_source_unit(ast);
    collector.print(title)
}

impl StatCollector {
    // Record a top-level node.
    fn record<T: ?Sized>(&mut self, label: &'static str, id: Option<ItemId>, val: &T) {
        self.record_inner(label, None, id, val, 0);
    }

    // Record a two-level entry, with a top-level enum type and a variant.
    fn record_variant<T: ?Sized>(
        &mut self,
        label1: &'static str,
        label2: &'static str,
        id: Option<ItemId>,
        val: &T,
        variant_size: usize,
    ) {
        self.record_inner(label1, Some(label2), id, val, variant_size);
    }

    fn record_inner<T: ?Sized>(
        &mut self,
        label1: &'static str,
        label2: Option<&'static str>,
        id: Option<ItemId>,
        val: &T,
        variant_size: usize,
    ) {
        if id.is_some_and(|x| !self.seen.insert(x)) {
            return;
        }

        match label2 {
            Some(label2) => self.stats.record_variant(label1, label2, val, variant_size),
            None => self.stats.record(label1, val),
        }
    }

    fn print(&self, title: &str) {
        self.stats.print(title);
    }
}

// Used to avoid boilerplate for types with many variants.
macro_rules! record_variants {
    (
        ($self:ident, $val:expr, $kind:expr, $id:expr, $mod:ident, $ty:ty, $tykind:ident),
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
                        $id,
                        $val,
                        variant_size,
                    )
                }
            )*
        }
    };
}

impl<'ast> Visit<'ast> for StatCollector {
    type BreakValue = Never;

    fn visit_source_unit(
        &mut self,
        source_unit: &'ast ast::SourceUnit<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("SourceUnit", None, source_unit);
        self.walk_source_unit(source_unit)
    }

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, item, item.kind, None, ast, Item, ItemKind),
            [Pragma, Import, Using, Contract, Function, Variable, Struct, Enum, Udvt, Error, Event]
        );
        self.walk_item(item)
    }

    fn visit_pragma_directive(
        &mut self,
        pragma: &'ast ast::PragmaDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("PragmaDirective", None, pragma);
        self.walk_pragma_directive(pragma)
    }

    fn visit_import_directive(
        &mut self,
        import: &'ast ast::ImportDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ImportDirective", None, import);
        self.walk_import_directive(import)
    }

    fn visit_using_directive(
        &mut self,
        using: &'ast ast::UsingDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("UsingDirective", None, using);
        match &using.list {
            ast::UsingList::Single(path) => {
                self.visit_path(path)?;
            }
            ast::UsingList::Multiple(paths) => {
                for (path, _) in paths.iter() {
                    self.visit_path(path)?;
                }
            }
        }
        // Don't visit ty field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_contract(
        &mut self,
        contract: &'ast ast::ItemContract<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemContract", None, contract);
        if let Some(layout) = &contract.layout {
            self.visit_expr(layout.slot)?;
        }
        for base in contract.bases.iter() {
            self.visit_modifier(base)?;
        }
        for item in contract.body.iter() {
            self.visit_item(item)?;
        }
        // Don't visit name field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_function(
        &mut self,
        function: &'ast ast::ItemFunction<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemFunction", None, function);
        if let Some(body) = &function.body {
            self.visit_block(body)?;
        }
        // Don't visit header field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_struct(
        &mut self,
        strukt: &'ast ast::ItemStruct<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemStruct", None, strukt);
        for field in strukt.fields.iter() {
            self.visit_variable_definition(field)?;
        }
        // Don't visit name field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_enum(
        &mut self,
        enum_: &'ast ast::ItemEnum<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemEnum", None, enum_);
        for variant in enum_.variants.iter() {
            self.visit_ident(variant)?;
        }
        // Don't visit name field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_udvt(
        &mut self,
        udvt: &'ast ast::ItemUdvt<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemUdvt", None, udvt);
        // Don't visit name or ty field since they aren't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_error(
        &mut self,
        error: &'ast ast::ItemError<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemError", None, error);
        self.visit_parameter_list(&error.parameters)?;
        // Don't visit name field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_item_event(
        &mut self,
        event: &'ast ast::ItemEvent<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ItemEvent", None, event);
        self.visit_parameter_list(&event.parameters)?;
        // Don't visit name field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_variable_definition(
        &mut self,
        var: &'ast ast::VariableDefinition<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("VariableDefinition", None, var);
        if let Some(initializer) = &var.initializer {
            self.visit_expr(initializer)?;
        }
        // Don't visit span, ty, name, or initializer since they aren't boxed
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, ty: &'ast ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, ty, ty.kind, None, ast, Type, TypeKind),
            [Elementary, Array, Function, Mapping, Custom]
        );
        self.walk_ty(ty)
    }

    fn visit_function_header(
        &mut self,
        header: &'ast ast::FunctionHeader<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("FunctionHeader", None, header);
        self.visit_parameter_list(&header.parameters)?;
        for modifier in header.modifiers.iter() {
            self.visit_modifier(modifier)?;
        }
        if let Some(returns) = &header.returns {
            self.visit_parameter_list(returns)?;
        }
        // Don't visit ident field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_modifier(
        &mut self,
        modifier: &'ast ast::Modifier<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("Modifier", None, modifier);
        // Don't visit name or arguments field since they aren't boxed
        ControlFlow::Continue(())
    }

    fn visit_call_args(
        &mut self,
        args: &'ast ast::CallArgs<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("CallArgs", None, args);
        self.walk_call_args(args)
    }

    fn visit_named_args(
        &mut self,
        args: &'ast ast::NamedArgList<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("NamedArgList", None, args);
        self.walk_named_args(args)
    }

    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, stmt, stmt.kind, None, ast, Stmt, StmtKind),
            [
                Assembly,
                DeclSingle,
                DeclMulti,
                Block,
                Break,
                Continue,
                DoWhile,
                Emit,
                Expr,
                For,
                If,
                Return,
                Revert,
                Try,
                UncheckedBlock,
                While,
                Placeholder
            ]
        );
        self.walk_stmt(stmt)
    }

    fn visit_stmt_assembly(
        &mut self,
        assembly: &'ast ast::StmtAssembly<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("StmtAssembly", None, assembly);
        self.walk_stmt_assembly(assembly)
    }

    fn visit_stmt_try(&mut self, try_: &'ast ast::StmtTry<'ast>) -> ControlFlow<Self::BreakValue> {
        self.record("StmtTry", None, try_);
        self.walk_stmt_try(try_)
    }

    fn visit_try_catch_clause(
        &mut self,
        catch: &'ast ast::TryCatchClause<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("TryCatchClause", None, catch);
        self.visit_parameter_list(&catch.args)?;
        self.visit_block(&catch.block)?;
        // Don't visit name field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_block(&mut self, block: &'ast ast::Block<'ast>) -> ControlFlow<Self::BreakValue> {
        self.record("Block", None, block);
        self.walk_block(block)
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, expr, expr.kind, None, ast, Expr, ExprKind),
            [
                Array,
                Assign,
                Binary,
                Call,
                CallOptions,
                Delete,
                Ident,
                Index,
                Lit,
                Member,
                New,
                Payable,
                Ternary,
                Tuple,
                TypeCall,
                Type,
                Unary
            ]
        );
        self.walk_expr(expr)
    }

    fn visit_parameter_list(
        &mut self,
        list: &'ast ast::ParameterList<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("ParameterList", None, list);
        self.walk_parameter_list(list)
    }

    fn visit_lit(&mut self, lit: &'ast ast::Lit<'_>) -> ControlFlow<Self::BreakValue> {
        self.record("Lit", None, lit);
        // Don't visit span field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_yul_stmt(&mut self, stmt: &'ast yul::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, stmt, stmt.kind, None, yul, Stmt, StmtKind),
            [
                Block,
                AssignSingle,
                AssignMulti,
                Expr,
                If,
                For,
                Switch,
                Leave,
                Break,
                Continue,
                FunctionDef,
                VarDecl
            ]
        );
        self.walk_yul_stmt(stmt)
    }

    fn visit_yul_block(&mut self, block: &'ast yul::Block<'ast>) -> ControlFlow<Self::BreakValue> {
        self.record("YulBlock", None, block);
        self.walk_yul_block(block)
    }

    fn visit_yul_stmt_switch(
        &mut self,
        switch: &'ast yul::StmtSwitch<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("YulStmtSwitch", None, switch);
        // Don't visit selector field since it isn't boxed
        for case in switch.cases.iter() {
            self.visit_yul_stmt_case(case)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_yul_stmt_case(
        &mut self,
        case: &'ast yul::StmtSwitchCase<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("YulStmtSwitchCase", None, case);
        // Don't visit lit field since it isn't boxed
        self.visit_yul_block(&case.body)?;
        ControlFlow::Continue(())
    }

    fn visit_yul_function(
        &mut self,
        function: &'ast yul::Function<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("YulFunction", None, function);
        // Don't visit ident field since it isn't boxed
        for ident in function.parameters.iter() {
            self.visit_ident(ident)?;
        }
        for ident in function.returns.iter() {
            self.visit_ident(ident)?;
        }
        self.visit_yul_block(&function.body)?;
        ControlFlow::Continue(())
    }

    fn visit_yul_expr(&mut self, expr: &'ast yul::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!((self, expr, expr.kind, None, yul, Expr, ExprKind), [Path, Call, Lit]);
        self.walk_yul_expr(expr)
    }

    fn visit_yul_expr_call(
        &mut self,
        call: &'ast yul::ExprCall<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("YulExprCall", None, call);
        // Don't visit name field since it isn't boxed
        for arg in call.arguments.iter() {
            self.visit_yul_expr(arg)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_doc_comments(
        &mut self,
        doc_comments: &'ast ast::DocComments<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("DocComments", None, doc_comments);
        self.walk_doc_comments(doc_comments)
    }

    fn visit_doc_comment(
        &mut self,
        doc_comment: &'ast ast::DocComment<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.record("DocComment", None, doc_comment);
        // Don't visit span field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_path(&mut self, path: &'ast ast::PathSlice) -> ControlFlow<Self::BreakValue> {
        self.record("PathSlice", None, path);
        self.walk_path(path)
    }

    fn visit_ident(&mut self, ident: &'ast ast::Ident) -> ControlFlow<Self::BreakValue> {
        self.record("Ident", None, ident);
        // Don't visit span field since it isn't boxed
        ControlFlow::Continue(())
    }

    fn visit_span(&mut self, span: &'ast ast::Span) -> ControlFlow<Self::BreakValue> {
        self.record("Span", None, span);
        ControlFlow::Continue(())
    }
}
