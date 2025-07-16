//! Constant and mutable AST visitor trait definitions.

use crate::ast::*;
use solar_data_structures::trustme;
use solar_interface::{Ident, Span, Spanned};
use solar_macros::declare_visitors;
use std::ops::ControlFlow;

declare_visitors! {
    /// AST traversal.
    pub trait Visit VisitMut <'ast> {
        /// The value returned when breaking from the traversal.
        ///
        /// This can be [`Never`](solar_data_structures::Never) to indicate that the traversal
        /// should never break.
        type BreakValue;

        fn visit_source_unit(&mut self, source_unit: &#mut SourceUnit<'ast>) -> ControlFlow<Self::BreakValue> {
            // TODO: SAFETY: Idk
            let source_unit = unsafe { trustme::decouple_lt #_mut(source_unit) };
            let SourceUnit { items } = source_unit;
            for item in items.iter #_mut() {
                self.visit_item #_mut(item)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_item(&mut self, item: &'ast #mut Item<'ast>) -> ControlFlow<Self::BreakValue> {
            let Item { docs, span, kind } = item;
            self.visit_span #_mut(span)?;
            self.visit_doc_comments #_mut(docs)?;
            match kind {
                ItemKind::Pragma(item) => self.visit_pragma_directive #_mut(item)?,
                ItemKind::Import(item) => self.visit_import_directive #_mut(item)?,
                ItemKind::Using(item) => self.visit_using_directive #_mut(item)?,
                ItemKind::Contract(item) => self.visit_item_contract #_mut(item)?,
                ItemKind::Function(item) => self.visit_item_function #_mut(item)?,
                ItemKind::Variable(item) => self.visit_variable_definition #_mut(item)?,
                ItemKind::Struct(item) => self.visit_item_struct #_mut(item)?,
                ItemKind::Enum(item) => self.visit_item_enum #_mut(item)?,
                ItemKind::Udvt(item) => self.visit_item_udvt #_mut(item)?,
                ItemKind::Error(item) => self.visit_item_error #_mut(item)?,
                ItemKind::Event(item) => self.visit_item_event #_mut(item)?,
            }
            ControlFlow::Continue(())
        }

        fn visit_pragma_directive(&mut self, pragma: &'ast #mut PragmaDirective<'ast>) -> ControlFlow<Self::BreakValue> {
            // noop by default.
            let PragmaDirective { tokens: _ } = pragma;
            ControlFlow::Continue(())
        }

        fn visit_import_directive(&mut self, import: &'ast #mut ImportDirective<'ast>) -> ControlFlow<Self::BreakValue> {
            let ImportDirective { path, items } = import;
            let _ = path; // TODO: ?
            match items {
                ImportItems::Plain(alias) => {
                    if let Some(alias) = alias {
                        self.visit_ident #_mut(alias)?;
                    }
                }
                ImportItems::Aliases(paths) => {
                    for (import, alias) in paths.iter #_mut() {
                        self.visit_ident #_mut(import)?;
                        if let Some(alias) = alias {
                            self.visit_ident #_mut(alias)?;
                        }
                    }
                }
                ImportItems::Glob(alias) => {
                    self.visit_ident #_mut(alias)?;
                }
            }
            ControlFlow::Continue(())
        }

        fn visit_using_directive(&mut self, using: &'ast #mut UsingDirective<'ast>) -> ControlFlow<Self::BreakValue> {
            let UsingDirective { list, ty, global: _ } = using;
            match list {
                UsingList::Single(path) => {
                    self.visit_path #_mut(path)?;
                }
                UsingList::Multiple(paths) => {
                    for (path, _) in paths.iter #_mut() {
                        self.visit_path #_mut(path)?;
                    }
                }
            }
            if let Some(ty) = ty {
                self.visit_ty #_mut(ty)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_item_contract(&mut self, contract: &'ast #mut ItemContract<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemContract { kind: _, name, layout, bases, body } = contract;
            self.visit_ident #_mut(name)?;
            if let Some(StorageLayoutSpecifier { span, slot }) = layout {
                self.visit_span #_mut(span)?;
                self.visit_expr #_mut(slot)?;
            }
            for base in bases.iter #_mut() {
                self.visit_modifier #_mut(base)?;
            }
            for item in body.iter #_mut() {
                self.visit_item #_mut(item)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_item_function(&mut self, function: &'ast #mut ItemFunction<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemFunction { kind: _, header, body, body_span } = function;
            self.visit_function_header #_mut(header)?;
            if let Some(body) = body {
                self.visit_block #_mut(body)?;
            }
            self.visit_span #_mut(body_span)?;
            ControlFlow::Continue(())
        }

        fn visit_item_struct(&mut self, strukt: &'ast #mut ItemStruct<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemStruct { name, fields } = strukt;
            self.visit_ident #_mut(name)?;
            for field in fields.iter #_mut() {
                self.visit_variable_definition #_mut(field)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_item_enum(&mut self, enum_: &'ast #mut ItemEnum<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemEnum { name, variants } = enum_;
            self.visit_ident #_mut(name)?;
            for variant in variants.iter #_mut() {
                self.visit_ident #_mut(variant)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_item_udvt(&mut self, udvt: &'ast #mut ItemUdvt<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemUdvt { name, ty } = udvt;
            self.visit_ident #_mut(name)?;
            self.visit_ty #_mut(ty)?;
            ControlFlow::Continue(())
        }

        fn visit_item_error(&mut self, error: &'ast #mut ItemError<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemError { name, parameters } = error;
            self.visit_ident #_mut(name)?;
            self.visit_parameter_list #_mut(parameters)?;
            ControlFlow::Continue(())
        }

        fn visit_item_event(&mut self, event: &'ast #mut ItemEvent<'ast>) -> ControlFlow<Self::BreakValue> {
            let ItemEvent { name, parameters, anonymous: _ } = event;
            self.visit_ident #_mut(name)?;
            self.visit_parameter_list #_mut(parameters)?;
            ControlFlow::Continue(())
        }

        fn visit_variable_definition(&mut self, var: &'ast #mut VariableDefinition<'ast>) -> ControlFlow<Self::BreakValue> {
            let VariableDefinition {
                span,
                ty,
                visibility: _,
                mutability: _,
                data_location: _,
                override_: _,
                indexed: _,
                name,
                initializer,
            } = var;
            self.visit_span #_mut(span)?;
            self.visit_ty #_mut(ty)?;
            if let Some(name) = name {
                self.visit_ident #_mut(name)?;
            }
            if let Some(initializer) = initializer {
                self.visit_expr #_mut(initializer)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_ty(&mut self, ty: &'ast #mut Type<'ast>) -> ControlFlow<Self::BreakValue> {
            let Type { span, kind } = ty;
            self.visit_span #_mut(span)?;
            match kind {
                TypeKind::Elementary(_) => {}
                TypeKind::Array(array) => {
                    let TypeArray { element, size: _ } = &#mut **array;
                    self.visit_ty #_mut(element)?;
                }
                TypeKind::Function(function) => {
                    let TypeFunction { parameters, visibility: _, state_mutability: _, returns } = &#mut **function;
                    self.visit_parameter_list #_mut(parameters)?;
                    if let Some(returns) = returns {
                        self.visit_parameter_list #_mut(returns)?;
                    }
                }
                TypeKind::Mapping(mapping) => {
                    let TypeMapping { key, key_name, value, value_name } = &#mut **mapping;
                    self.visit_ty #_mut(key)?;
                    if let Some(key_name) = key_name {
                        self.visit_ident #_mut(key_name)?;
                    }
                    self.visit_ty #_mut(value)?;
                    if let Some(value_name) = value_name {
                        self.visit_ident #_mut(value_name)?;
                    }
                }
                TypeKind::Custom(path) => {
                    self.visit_path #_mut(path)?;
                }
            }
            ControlFlow::Continue(())
        }

        fn visit_function_header(&mut self, header: &'ast #mut FunctionHeader<'ast>) -> ControlFlow<Self::BreakValue> {
            let FunctionHeader {
                span,
                name,
                parameters,
                visibility,
                state_mutability,
                modifiers,
                virtual_: _,
                override_,
                returns,
            } = header;
            self.visit_span #_mut(span)?;
            if let Some(name) = name {
                self.visit_ident #_mut(name)?;
            }
            self.visit_parameter_list #_mut(parameters)?;
            if let Some(vis) = visibility {
                let Spanned { span: vis_span, .. } = vis;
                self.visit_span #_mut(vis_span)?;
            }
            if let Some(state_mut) = state_mutability {
                let Spanned { span: state_mut_span, .. } = state_mut;
                self.visit_span #_mut(state_mut_span)?;
            }
            for modifier in modifiers.iter #_mut() {
                self.visit_modifier #_mut(modifier)?;
            }
            if let Some(returns) = returns {
                self.visit_parameter_list #_mut(returns)?;
            }
            if let Some(override_) = override_ {
                self.visit_override #_mut(override_)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_modifier(&mut self, modifier: &'ast #mut Modifier<'ast>) -> ControlFlow<Self::BreakValue> {
            let Modifier { name, arguments } = modifier;
            self.visit_path #_mut(name)?;
            self.visit_call_args #_mut(arguments)?;
            ControlFlow::Continue(())
        }

        fn visit_override(&mut self, override_: &'ast #mut Override<'ast>) -> ControlFlow<Self::BreakValue> {
            let Override { span, paths } = override_;
            self.visit_span #_mut(span)?;
            for path in paths.iter #_mut() {
                self.visit_path #_mut(path)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_call_args(&mut self, args: &'ast #mut CallArgs<'ast>) -> ControlFlow<Self::BreakValue> {
            let CallArgs { span, kind } = args;
            self.visit_span #_mut(span)?;
            match kind {
                CallArgsKind::Named(named) => {
                    self.visit_named_args #_mut(named)?;
                }
                CallArgsKind::Unnamed(unnamed) => {
                    for arg in unnamed.iter #_mut() {
                        self.visit_expr #_mut(arg)?;
                    }
                }
            }
            ControlFlow::Continue(())
        }

        fn visit_named_args(&mut self, args: &'ast #mut NamedArgList<'ast>) -> ControlFlow<Self::BreakValue> {
            for NamedArg { name, value } in args.iter #_mut() {
                self.visit_ident #_mut(name)?;
                self.visit_expr #_mut(value)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_stmt(&mut self, stmt: &'ast #mut Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
            let Stmt { docs, span, kind } = stmt;
            self.visit_doc_comments #_mut(docs)?;
            self.visit_span #_mut(span)?;
            match kind {
                StmtKind::Assembly(assembly) => {
                    self.visit_stmt_assembly #_mut(assembly)?;
                }
                StmtKind::DeclSingle(var) => {
                    self.visit_variable_definition #_mut(var)?;
                }
                StmtKind::DeclMulti(vars, expr) => {
                    for var in vars.iter #_mut().flatten() {
                        self.visit_variable_definition #_mut(var)?;
                    }
                    self.visit_expr #_mut(expr)?;
                }
                StmtKind::Block(block) => {
                    self.visit_block #_mut(block)?;
                }
                StmtKind::Break => {}
                StmtKind::Continue => {}
                StmtKind::DoWhile(stmt, expr) => {
                    self.visit_stmt #_mut(stmt)?;
                    self.visit_expr #_mut(expr)?;
                }
                StmtKind::Emit(path, args) => {
                    self.visit_path #_mut(path)?;
                    self.visit_call_args #_mut(args)?;
                }
                StmtKind::Expr(expr) => {
                    self.visit_expr #_mut(expr)?;
                }
                StmtKind::For { init, cond, next, body } => {
                    if let Some(init) = init {
                        self.visit_stmt #_mut(init)?;
                    }
                    if let Some(cond) = cond {
                        self.visit_expr #_mut(cond)?;
                    }
                    if let Some(next) = next {
                        self.visit_expr #_mut(next)?;
                    }
                    self.visit_stmt #_mut(body)?;
                }
                StmtKind::If(cond, then, else_) => {
                    self.visit_expr #_mut(cond)?;
                    self.visit_stmt #_mut(then)?;
                    if let Some(else_) = else_ {
                        self.visit_stmt #_mut(else_)?;
                    }
                }
                StmtKind::Return(expr) => {
                    if let Some(expr) = expr {
                        self.visit_expr #_mut(expr)?;
                    }
                }
                StmtKind::Revert(path, args) => {
                    self.visit_path #_mut(path)?;
                    self.visit_call_args #_mut(args)?;
                }
                StmtKind::Try(try_) => {
                    self.visit_stmt_try #_mut(try_)?;
                }
                StmtKind::UncheckedBlock(block) => {
                    self.visit_block #_mut(block)?;
                }
                StmtKind::While(cond, stmt) => {
                    self.visit_expr #_mut(cond)?;
                    self.visit_stmt #_mut(stmt)?;
                }
                StmtKind::Placeholder => {}
            }
            ControlFlow::Continue(())
        }

        fn visit_stmt_assembly(&mut self, assembly: &'ast #mut StmtAssembly<'ast>) -> ControlFlow<Self::BreakValue> {
            let StmtAssembly { dialect: _, flags: _, block } = assembly;
            self.visit_yul_block #_mut(block)?;
            ControlFlow::Continue(())
        }

        fn visit_stmt_try(&mut self, try_: &'ast #mut StmtTry<'ast>) -> ControlFlow<Self::BreakValue> {
            let StmtTry { expr, clauses } = try_;
            self.visit_expr #_mut(expr)?;
            for catch in clauses.iter #_mut() {
                self.visit_try_catch_clause #_mut(catch)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_try_catch_clause(&mut self, catch: &'ast #mut TryCatchClause<'ast>) -> ControlFlow<Self::BreakValue> {
            let TryCatchClause { span, name, args, block } = catch;
            self.visit_span #_mut(span)?;
            if let Some(name) = name {
                self.visit_ident #_mut(name)?;
            }
            self.visit_parameter_list #_mut(args)?;
            self.visit_block #_mut(block)?;
            ControlFlow::Continue(())
        }

        fn visit_block(&mut self, block: &'ast #mut Block<'ast>) -> ControlFlow<Self::BreakValue> {
            let Block { span, stmts } = block;
            self.visit_span #_mut(span)?;
            for stmt in stmts.iter #_mut() {
                self.visit_stmt #_mut(stmt)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_expr(&mut self, expr: &'ast #mut Expr<'ast>) -> ControlFlow<Self::BreakValue> {
            let Expr { span, kind } = expr;
            self.visit_span #_mut(span)?;
            match kind {
                ExprKind::Array(exprs) => {
                    for expr in exprs.iter #_mut() {
                        self.visit_expr #_mut(expr)?;
                    }
                }
                ExprKind::Assign(lhs, _op, rhs) => {
                    self.visit_expr #_mut(lhs)?;
                    self.visit_expr #_mut(rhs)?;
                }
                ExprKind::Binary(lhs, _op, rhs) => {
                    self.visit_expr #_mut(lhs)?;
                    self.visit_expr #_mut(rhs)?;
                }
                ExprKind::Call(lhs, args) => {
                    self.visit_expr #_mut(lhs)?;
                    self.visit_call_args #_mut(args)?;
                }
                ExprKind::CallOptions(lhs, args) => {
                    self.visit_expr #_mut(lhs)?;
                    self.visit_named_args #_mut(args)?;
                }
                ExprKind::Delete(expr) => {
                    self.visit_expr #_mut(expr)?;
                }
                ExprKind::Ident(ident) => {
                    self.visit_ident #_mut(ident)?;
                }
                ExprKind::Index(lhs, kind) => {
                    self.visit_expr #_mut(lhs)?;
                    match kind {
                        IndexKind::Index(expr) => {
                            if let Some(expr) = expr {
                                self.visit_expr #_mut(expr)?;
                            }
                        }
                        IndexKind::Range(start, end) => {
                            if let Some(start) = start {
                                self.visit_expr #_mut(start)?;
                            }
                            if let Some(end) = end {
                                self.visit_expr #_mut(end)?;
                            }
                        }
                    }
                }
                ExprKind::Lit(lit, _sub) => {
                    self.visit_lit #_mut(lit)?;
                }
                ExprKind::Member(expr, member) => {
                    self.visit_expr #_mut(expr)?;
                    self.visit_ident #_mut(member)?;
                }
                ExprKind::New(ty) => {
                    self.visit_ty #_mut(ty)?;
                }
                ExprKind::Payable(args) => {
                    self.visit_call_args #_mut(args)?;
                }
                ExprKind::Ternary(cond, true_, false_) => {
                    self.visit_expr #_mut(cond)?;
                    self.visit_expr #_mut(true_)?;
                    self.visit_expr #_mut(false_)?;
                }
                ExprKind::Tuple(exprs) => {
                    for expr in exprs.iter #_mut().flatten() {
                        self.visit_expr #_mut(expr)?;
                    }
                }
                ExprKind::TypeCall(ty) => {
                    self.visit_ty #_mut(ty)?;
                }
                ExprKind::Type(ty) => {
                    self.visit_ty #_mut(ty)?;
                }
                ExprKind::Unary(_op, expr) => {
                    self.visit_expr #_mut(expr)?;
                }
            }
            ControlFlow::Continue(())
        }

        fn visit_parameter_list(&mut self, list: &'ast #mut ParameterList<'ast>) -> ControlFlow<Self::BreakValue> {
            let ParameterList { span, vars } = list;
            for param in vars.iter #_mut() {
                self.visit_variable_definition #_mut(param)?;
            }
            self.visit_span #_mut(span)?;
            ControlFlow::Continue(())
        }

        fn visit_lit(&mut self, lit: &'ast #mut Lit) -> ControlFlow<Self::BreakValue> {
            let Lit { span, symbol: _, kind: _ } = lit;
            self.visit_span #_mut(span)?;
            ControlFlow::Continue(())
        }

        fn visit_yul_stmt(&mut self, stmt: &'ast #mut yul::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::Stmt { docs, span, kind } = stmt;
            self.visit_doc_comments #_mut(docs)?;
            self.visit_span #_mut(span)?;
            match kind {
                yul::StmtKind::Block(block) => {
                    self.visit_yul_block #_mut(block)?;
                }
                yul::StmtKind::AssignSingle(path, expr) => {
                    self.visit_path #_mut(path)?;
                    self.visit_yul_expr #_mut(expr)?;
                }
                yul::StmtKind::AssignMulti(paths, call) => {
                    for path in paths.iter #_mut() {
                        self.visit_path #_mut(path)?;
                    }
                    self.visit_yul_expr_call #_mut(call)?;
                }
                yul::StmtKind::Expr(call) => {
                    self.visit_yul_expr_call #_mut(call)?;
                }
                yul::StmtKind::If(expr, block) => {
                    self.visit_yul_expr #_mut(expr)?;
                    self.visit_yul_block #_mut(block)?;
                }
                yul::StmtKind::For { init, cond, step, body } => {
                    self.visit_yul_block #_mut(init)?;
                    self.visit_yul_expr #_mut(cond)?;
                    self.visit_yul_block #_mut(step)?;
                    self.visit_yul_block #_mut(body)?;
                }
                yul::StmtKind::Switch(switch) => {
                    self.visit_yul_stmt_switch #_mut(switch)?;
                }
                yul::StmtKind::Leave => {}
                yul::StmtKind::Break => {}
                yul::StmtKind::Continue => {}
                yul::StmtKind::FunctionDef(function) => {
                    self.visit_yul_function #_mut(function)?;
                }
                yul::StmtKind::VarDecl(idents, expr) => {
                    for ident in idents.iter #_mut() {
                        self.visit_ident #_mut(ident)?;
                    }
                    if let Some(expr) = expr {
                        self.visit_yul_expr #_mut(expr)?;
                    }
                }
            }
            ControlFlow::Continue(())
        }

        fn visit_yul_block(&mut self, block: &'ast #mut yul::Block<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::Block { span, stmts } = block;
            self.visit_span #_mut(span)?;
            for stmt in stmts.iter #_mut() {
                self.visit_yul_stmt #_mut(stmt)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_yul_stmt_switch(&mut self, switch: &'ast #mut yul::StmtSwitch<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::StmtSwitch { selector, branches, default_case } = switch;
            self.visit_yul_expr #_mut(selector)?;
            for case in branches.iter #_mut() {
                self.visit_yul_stmt_case #_mut(case)?;
            }
            if let Some(case) = default_case {
                self.visit_yul_block #_mut(case)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_yul_stmt_case(&mut self, case: &'ast #mut yul::StmtSwitchCase<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::StmtSwitchCase { constant, body } = case;
            self.visit_lit #_mut(constant)?;
            self.visit_yul_block #_mut(body)?;
            ControlFlow::Continue(())
        }

        fn visit_yul_function(&mut self, function: &'ast #mut yul::Function<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::Function { name, parameters, returns, body } = function;
            self.visit_ident #_mut(name)?;
            for ident in parameters.iter #_mut() {
                self.visit_ident #_mut(ident)?;
            }
            for ident in returns.iter #_mut() {
                self.visit_ident #_mut(ident)?;
            }
            self.visit_yul_block #_mut(body)?;
            ControlFlow::Continue(())
        }

        fn visit_yul_expr(&mut self, expr: &'ast #mut yul::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::Expr { span, kind } = expr;
            self.visit_span #_mut(span)?;
            match kind {
                yul::ExprKind::Path(path) => {
                    self.visit_path #_mut(path)?;
                }
                yul::ExprKind::Call(call) => {
                    self.visit_yul_expr_call #_mut(call)?;
                }
                yul::ExprKind::Lit(lit) => {
                    self.visit_lit #_mut(lit)?;
                }
            }
            ControlFlow::Continue(())
        }

        fn visit_yul_expr_call(&mut self, call: &'ast #mut yul::ExprCall<'ast>) -> ControlFlow<Self::BreakValue> {
            let yul::ExprCall { name, arguments } = call;
            self.visit_ident #_mut(name)?;
            for arg in arguments.iter #_mut() {
                self.visit_yul_expr #_mut(arg)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_doc_comments(&mut self, doc_comments: &'ast #mut DocComments<'ast>) -> ControlFlow<Self::BreakValue> {
            for doc_comment in doc_comments.iter #_mut() {
                self.visit_doc_comment #_mut(doc_comment)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_doc_comment(&mut self, doc_comment: &'ast #mut DocComment) -> ControlFlow<Self::BreakValue> {
            let DocComment { kind: _, span, symbol: _ } = doc_comment;
            self.visit_span #_mut(span)?;
            ControlFlow::Continue(())
        }

        fn visit_path(&mut self, path: &'ast #mut PathSlice) -> ControlFlow<Self::BreakValue> {
            for ident in path.segments #_mut() {
                self.visit_ident #_mut(ident)?;
            }
            ControlFlow::Continue(())
        }

        fn visit_ident(&mut self, ident: &'ast #mut Ident) -> ControlFlow<Self::BreakValue> {
            let Ident { name: _, span } = ident;
            self.visit_span #_mut(span)?;
            ControlFlow::Continue(())
        }

        fn visit_span(&mut self, span: &'ast #mut Span) -> ControlFlow<Self::BreakValue> {
            // Nothing to do.
            let _ = span;
            ControlFlow::Continue(())
        }
    }
}
