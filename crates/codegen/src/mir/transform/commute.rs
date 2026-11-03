//! Commuted spellings of a rule root and its operands.
//!
//! A rule states each fact once, in one operand order, as LLVM's `m_c_*` matchers do.
//! The rule contexts run the generated matcher once per spelling of the node: the root
//! itself, with the operands of a commutative root swapped, and each operand whose
//! definition is commutative, read with that definition's operands swapped. A rule that
//! names an operand in a guard binds it after the swap, so it always guards the operand
//! it matched. Commutative operations nested deeper than the operands of the root are
//! not swapped, so a rule over them states each order itself.
//!
//! Every node a rule sees is in the order of [`Function::canonical_op`], and a pair that
//! holds a constant keeps it: constants stay on the right, so a rule whose operand can bind
//! a constant writes that operand last, and two constants stay in the order of their values.
//! A context first matches the node as written, and spells it further only when its root, or
//! a definition the match read, has operands to swap.
//!
//! The spellings swap every pair of non-constants, not only pairs of equal rank, because a
//! rule can name one value in two commutative nodes, as in `(x & y) | (x & z)`. With
//! `x` an argument and `y` and `z` loads, the nodes are `and(y, x)` and `and(z, x)` in
//! canonical order, and only a swap of both lines `x` up for the rule.
//!
//! Spellings run with the written order first, then the ones that swap later operands,
//! and swapping the root last.

use crate::mir::{Function, Op, OpTraits, ValueId};
use smallvec::SmallVec;

/// Operands whose definitions a rule reads with their commutative operands swapped.
pub(super) type Commuted = SmallVec<[ValueId; 3]>;

/// What a rule context has read while it matches one spelling of a node.
#[derive(Default)]
pub(super) struct Reading {
    /// Operands whose definitions the current spelling of the root reads commuted.
    pub(super) commuted: Commuted,
    /// Whether a definition read by the current match has operands to swap.
    pub(super) read_commutative: bool,
}

impl Reading {
    /// Returns the definition `op` of `value` as the current spelling presents it to a rule: in
    /// canonical order, then swapped when the spelling commutes `value`.
    pub(super) fn view(&mut self, func: &Function, value: ValueId, op: Op) -> Op {
        let op = func.canonical_op(op);
        self.read_commutative |= swappable(func, &op);
        if self.commuted.contains(&value) { op.commuted().unwrap_or(op) } else { op }
    }
}

/// Returns the commutative definition of `value`, if it has one.
pub(super) fn commutative_definition(func: &Function, value: ValueId) -> Option<Op> {
    let kind = func.value_inst_kind(value)?;
    kind.traits().contains(OpTraits::COMMUTATIVE).then(|| kind.op())
}

/// Returns whether `op` is commutative over two operands that may change sides: constants stay on
/// the right.
pub(super) fn swappable(func: &Function, op: &Op) -> bool {
    let movable = |value| func.value(value).as_immediate().is_none();
    op.commutative_pair().is_some_and(|(lhs, rhs)| lhs != rhs && movable(lhs) && movable(rhs))
}

/// The orders in which the matcher sees one node.
pub(super) struct Spellings {
    swap_root: bool,
    sites: Commuted,
}

impl Spellings {
    /// Collects the spellings of `root`, which must be in canonical order. `definition` returns the
    /// definition of an operand when it may be commutative.
    pub(super) fn new(
        func: &Function,
        root: &Op,
        definition: impl Fn(ValueId) -> Option<Op>,
    ) -> Self {
        let mut sites = Commuted::new();
        let _ = root.map_values(|operand| {
            if !sites.contains(&operand)
                && definition(operand).is_some_and(|definition| swappable(func, &definition))
            {
                sites.push(operand);
            }
            operand
        });
        Self { swap_root: swappable(func, root), sites }
    }

    /// Returns the spellings after the written one, each as the root to match and the operands
    /// whose definitions to read swapped.
    pub(super) fn rest(&self, root: &Op) -> impl Iterator<Item = (Op, Commuted)> + '_ {
        let root = *root;
        let sites = self.sites.len();
        (1..(1 + usize::from(self.swap_root)) << sites).map(move |index| {
            let swapped_root = if index >> sites != 0 { root.commuted() } else { None };
            let swapped_operands = self
                .sites
                .iter()
                .enumerate()
                .filter(|&(site, _)| index >> (sites - 1 - site) & 1 == 1)
                .map(|(_, &operand)| operand)
                .collect();
            (swapped_root.unwrap_or(root), swapped_operands)
        })
    }
}
