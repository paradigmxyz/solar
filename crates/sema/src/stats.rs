use solar_ast::{self as ast, ItemId, visit::Visit, yul};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
};
use std::ops::ControlFlow;

struct NodeStats {
    count: usize,
    size: usize,
}

impl NodeStats {
    fn new() -> Self {
        Self { count: 0, size: 0 }
    }

    fn accum_size(&self) -> usize {
        self.count * self.size
    }
}

struct Node {
    stats: NodeStats,
    subnodes: FxHashMap<&'static str, NodeStats>,
}

impl Node {
    fn new() -> Self {
        Self { stats: NodeStats::new(), subnodes: FxHashMap::default() }
    }
}

/// Stat collector.
struct StatCollector {
    nodes: FxHashMap<&'static str, Node>,
    seen: FxHashSet<ItemId>,
}

pub fn print_ast_stats<'ast>(ast: &'ast ast::SourceUnit<'ast>, title: &str, prefix: &str) {
    let mut collector = StatCollector { nodes: FxHashMap::default(), seen: FxHashSet::default() };
    let _ = collector.visit_source_unit(ast);
    collector.print(title, prefix)
}

impl StatCollector {
    // Record a top-level node.
    fn record<T: ?Sized>(&mut self, label: &'static str, id: Option<ItemId>, val: &T) {
        self.record_inner(label, None, id, val);
    }

    // Record a two-level entry, with a top-level enum type and a variant.
    fn record_variant<T: ?Sized>(
        &mut self,
        label1: &'static str,
        label2: &'static str,
        id: Option<ItemId>,
        val: &T,
    ) {
        self.record_inner(label1, Some(label2), id, val);
    }

    fn record_inner<T: ?Sized>(
        &mut self,
        label1: &'static str,
        label2: Option<&'static str>,
        id: Option<ItemId>,
        val: &T,
    ) {
        if id.is_some_and(|x| !self.seen.insert(x)) {
            return;
        }

        let node = self.nodes.entry(label1).or_insert(Node::new());
        node.stats.count += 1;
        node.stats.size = size_of_val(val);

        if let Some(label2) = label2 {
            let subnode = node.subnodes.entry(label2).or_insert(NodeStats::new());
            subnode.count += 1;
            subnode.size = size_of_val(val);
        }
    }

    fn print(&self, title: &str, prefix: &str) {
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by_cached_key(|(label, node)| (node.stats.accum_size(), label.to_string()));

        let total_size = nodes.iter().map(|(_, node)| node.stats.accum_size()).sum();

        eprintln!("{prefix} {title}");
        eprintln!(
            "{} {:<18}{:>18}{:>14}{:>14}",
            prefix, "Name", "Accumulated Size", "Count", "Item Size"
        );
        eprintln!("{prefix} ----------------------------------------------------------------");

        let percent = |m, n| (m * 100) as f64 / n as f64;

        for (label, node) in nodes {
            let size = node.stats.accum_size();
            eprintln!(
                "{} {:<18}{:>10} ({:4.1}%){:>14}{:>14}",
                prefix,
                label,
                to_readable_str(size),
                percent(size, total_size),
                to_readable_str(node.stats.count),
                to_readable_str(node.stats.size)
            );
            if !node.subnodes.is_empty() {
                let mut subnodes: Vec<_> = node.subnodes.iter().collect();
                subnodes.sort_by_cached_key(|(label, subnode)| {
                    (subnode.accum_size(), label.to_string())
                });

                for (label, subnode) in subnodes {
                    let size = subnode.accum_size();
                    eprintln!(
                        "{} - {:<16}{:>10} ({:4.1}%){:>14}",
                        prefix,
                        label,
                        to_readable_str(size),
                        percent(size, total_size),
                        to_readable_str(subnode.count),
                    );
                }
            }
        }
        eprintln!("{prefix} ----------------------------------------------------------------");
        eprintln!("{} {:<18}{:>10}", prefix, "Total", to_readable_str(total_size));
        eprintln!("{prefix}");
    }
}

// Used to avoid boilerplate for types with many variants.
macro_rules! record_variants {
    (
        ($self:ident, $val:expr, $kind:expr, $id:expr, $mod:ident, $ty:ty, $tykind:ident),
        [$($variant:ident),*]
    ) => {
        match $kind {
            $(
                $mod::$tykind::$variant { .. } => {
                    $self.record_variant(stringify!($ty), stringify!($variant), $id, $val)
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
        doc_comment: &'ast ast::DocComment,
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

pub fn to_readable_str(mut val: usize) -> String {
    let mut groups = vec![];
    loop {
        let group = val % 1000;
        val /= 1000;
        if val == 0 {
            groups.push(group.to_string());
            break;
        } else {
            groups.push(format!("{group:03}"));
        }
    }
    groups.reverse();
    groups.join("_")
}
