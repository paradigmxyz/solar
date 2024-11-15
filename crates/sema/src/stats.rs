use std::ops::ControlFlow;

use solar_ast::{
    ast::{self, ItemId},
    visit::Visit,
};
use solar_data_structures::{
    map::{FxHashMap, FxHashSet},
    Never,
};

struct NodeStats {
    count: usize,
    size: usize,
}

impl NodeStats {
    fn new() -> Self {
        Self { count: 0, size: 0 }
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

pub fn print_ast_stats(ast: &ast::SourceUnit<'_>, title: &str, prefix: &str) {
    let mut collector = StatCollector { nodes: FxHashMap::default(), seen: FxHashSet::default() };
    collector.visit_source_unit(ast);
    collector.print(title, prefix)
}

impl StatCollector {
    // Record a top-level node.
    fn record<T>(&mut self, label: &'static str, id: Option<ItemId>, val: &T) {
        self.record_inner(label, None, id, val);
    }

    // Record a two-level entry, with a top-level enum type and a variant.
    fn record_variant<T>(
        &mut self,
        label1: &'static str,
        label2: &'static str,
        id: Option<ItemId>,
        val: &T,
    ) {
        self.record_inner(label1, Some(label2), id, val);
    }

    fn record_inner<T>(
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
        node.stats.size = std::mem::size_of_val(val);

        if let Some(label2) = label2 {
            let subnode = node.subnodes.entry(label2).or_insert(NodeStats::new());
            subnode.count += 1;
            subnode.size = std::mem::size_of_val(val);
        }
    }

    fn print(&self, title: &str, prefix: &str) {
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by_key(|(_, node)| node.stats.count * node.stats.size);

        let total_size = nodes.iter().map(|(_, node)| node.stats.count * node.stats.size).sum();

        eprintln!("{prefix} {title}");
        eprintln!(
            "{} {:<18}{:>18}{:>14}{:>14}",
            prefix, "Name", "Accumulated Size", "Count", "Item Size"
        );
        eprintln!("{prefix} ----------------------------------------------------------------");

        let percent = |m, n| (m * 100) as f64 / n as f64;

        for (label, node) in nodes {
            let size = node.stats.count * node.stats.size;
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
                // We will soon sort, so the initial order does not matter.
                #[allow(rustc::potential_query_instability)]
                let mut subnodes: Vec<_> = node.subnodes.iter().collect();
                subnodes.sort_by_key(|(_, subnode)| subnode.count * subnode.size);

                for (label, subnode) in subnodes {
                    let size = subnode.count * subnode.size;
                    eprintln!(
                        "{} - {:<18}{:>10} ({:4.1}%){:>14}",
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

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, item, item.kind, None, ast, Item, ItemKind),
            [Pragma, Import, Using, Contract, Function, Variable, Struct, Enum, Udvt, Error, Event]
        );
        self.walk_item(item)
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

    fn visit_ty(&mut self, ty: &'ast ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        record_variants!(
            (self, ty, ty.kind, None, ast, Type, TypeKind),
            [Elementary, Array, Function, Mapping, Custom]
        );
        self.walk_ty(ty)
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
