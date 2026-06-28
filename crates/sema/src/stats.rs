use comfy_table::{Cell, CellAlignment, Table, presets::ASCII_FULL_CONDENSED};
use solar_data_structures::map::FxHashMap;
use std::{alloc::Layout, mem::size_of_val};

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

struct Stats {
    nodes: FxHashMap<&'static str, Node>,
}

impl Stats {
    fn new() -> Self {
        Self { nodes: FxHashMap::default() }
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

    fn print(&self, title: &str) {
        print_stats(&self.nodes, title);
    }
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
        super::fields_layout_size(super::layout_of($self), &[$(super::layout_of($field)),*])
    };
}

mod ast;
mod hir;

pub use ast::print_ast_stats;
pub use hir::print_hir_stats;

fn print_stats(nodes: &FxHashMap<&'static str, Node>, title: &str) {
    let mut nodes: Vec<_> = nodes.iter().collect();
    nodes.sort_by_cached_key(|(label, node)| (node.stats.accum_size(), label.to_string()));

    let total_size = nodes.iter().map(|(_, node)| node.stats.accum_size()).sum();

    eprintln!("{title}");

    let percent = |m, n| (m * 100) as f64 / n as f64;
    fn right(value: impl ToString) -> Cell {
        Cell::new(value).set_alignment(CellAlignment::Right)
    }

    let mut table = Table::new();
    table.load_preset(ASCII_FULL_CONDENSED);
    table.set_header([
        Cell::new("Name"),
        right("Accumulated Size"),
        right("%"),
        right("Count"),
        right("Item Size"),
    ]);

    for (label, node) in nodes {
        let size = node.stats.accum_size();
        table.add_row([
            Cell::new(label),
            right(to_readable_str(size)),
            right(format!("{:.1}", percent(size, total_size))),
            right(to_readable_str(node.stats.count)),
            right(to_readable_str(node.stats.size)),
        ]);
        if !node.subnodes.is_empty() {
            let mut subnodes: Vec<_> = node.subnodes.iter().collect();
            subnodes
                .sort_by_cached_key(|(label, subnode)| (subnode.accum_size(), label.to_string()));

            for (label, subnode) in subnodes {
                let size = subnode.accum_size();
                table.add_row([
                    Cell::new(format!("- {label}")),
                    right(to_readable_str(size)),
                    right(format!("{:.1}", percent(size, total_size))),
                    right(to_readable_str(subnode.count)),
                    right(to_readable_str(subnode.size)),
                ]);
            }
        }
    }

    table.add_row([
        Cell::new("Total"),
        right(to_readable_str(total_size)),
        right(""),
        right(""),
        right(""),
    ]);

    eprintln!("{table}");
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
