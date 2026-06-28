use crate::hir::{self, Visit as HirVisit};
use solar_ast::{self as ast, ItemId, visit::Visit, yul};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
};
use std::{alloc::Layout, mem::size_of_val, ops::ControlFlow};

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

/// HIR stat collector.
struct HirStatCollector<'hir> {
    hir: &'hir hir::Hir<'hir>,
    nodes: FxHashMap<&'static str, Node>,
    seen_items: FxHashSet<hir::ItemId>,
    seen_vars: FxHashSet<hir::VariableId>,
}

trait EnumVariantSize {
    fn variant_payload_size(&self) -> usize;
}

fn layout_of<T>(x: &T) -> Layout {
    Layout::for_value(x)
}

fn fields_layout_size(this: Layout, fields: &[Layout]) -> usize {
    let mut layout = Layout::from_size_align(0, this.align()).unwrap();
    for field in fields {
        let (next, _) = layout.extend(*field).expect("variant layout should fit in usize");
        layout = next;
    }
    layout.pad_to_align().size()
}

macro_rules! variant_payload_size {
    ($self:expr, $($field:expr),* $(,)?) => {
        fields_layout_size(layout_of($self), &[$(layout_of($field)),*])
    };
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

pub fn print_ast_stats<'ast>(ast: &'ast ast::SourceUnit<'ast>, title: &str, prefix: &str) {
    let mut collector = StatCollector { nodes: FxHashMap::default(), seen: FxHashSet::default() };
    let _ = collector.visit_source_unit(ast);
    collector.print(title, prefix)
}

pub fn print_hir_stats<'hir>(hir: &'hir hir::Hir<'hir>, title: &str, prefix: &str) {
    let mut collector = HirStatCollector {
        hir,
        nodes: FxHashMap::default(),
        seen_items: FxHashSet::default(),
        seen_vars: FxHashSet::default(),
    };
    collector.collect();
    collector.print(title, prefix);
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

        let node = self.nodes.entry(label1).or_insert(Node::new());
        node.stats.count += 1;
        node.stats.size = size_of_val(val);

        if let Some(label2) = label2 {
            let subnode = node.subnodes.entry(label2).or_insert(NodeStats::new());
            subnode.count += 1;
            subnode.size = variant_size;
        }
    }

    fn print(&self, title: &str, prefix: &str) {
        print_stats(&self.nodes, title, prefix);
    }
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
        let node = self.nodes.entry(label).or_insert(Node::new());
        node.stats.count += 1;
        node.stats.size = size_of_val(val);
    }

    fn record_variant<T: ?Sized>(
        &mut self,
        label1: &'static str,
        label2: &'static str,
        val: &T,
        variant_size: usize,
    ) {
        let node = self.nodes.entry(label1).or_insert(Node::new());
        node.stats.count += 1;
        node.stats.size = size_of_val(val);

        let subnode = node.subnodes.entry(label2).or_insert(NodeStats::new());
        subnode.count += 1;
        subnode.size = variant_size;
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

    fn print(&self, title: &str, prefix: &str) {
        print_stats(&self.nodes, title, prefix);
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

fn print_stats(nodes: &FxHashMap<&'static str, Node>, title: &str, prefix: &str) {
    let mut nodes: Vec<_> = nodes.iter().collect();
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
            subnodes
                .sort_by_cached_key(|(label, subnode)| (subnode.accum_size(), label.to_string()));

            for (label, subnode) in subnodes {
                let size = subnode.accum_size();
                eprintln!(
                    "{} - {:<16}{:>10} ({:4.1}%){:>14}{:>14}",
                    prefix,
                    label,
                    to_readable_str(size),
                    percent(size, total_size),
                    to_readable_str(subnode.count),
                    to_readable_str(subnode.size),
                );
            }
        }
    }
    eprintln!("{prefix} ----------------------------------------------------------------");
    eprintln!("{} {:<18}{:>10}", prefix, "Total", to_readable_str(total_size));
    eprintln!("{prefix}");
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
