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
//! A context may keep operands from changing sides: the e-graph keeps constants on the
//! right, so it swaps only pairs without a constant, and a rule whose operand can bind a
//! constant writes that operand last. A context first matches the node as written, and
//! spells it further only when its root, or a definition the match read, has operands
//! to swap.
//!
//! Spellings run with the written order first, then the ones that swap later operands,
//! and swapping the root last, so a rule builds its results in the order its permutation
//! rules did.

use crate::mir::{Op, ValueId};
use smallvec::SmallVec;

/// Operands whose definitions a rule reads with their commutative operands swapped.
pub(super) type Commuted = SmallVec<[ValueId; 3]>;

/// Returns whether `op` is commutative over two operands that may change sides.
pub(super) fn swappable(op: &Op, movable: &impl Fn(ValueId) -> bool) -> bool {
    op.commutative_pair().is_some_and(|(lhs, rhs)| lhs != rhs && movable(lhs) && movable(rhs))
}

/// The orders in which the matcher sees one node.
pub(super) struct Spellings {
    swap_root: bool,
    sites: Commuted,
}

impl Spellings {
    /// Collects the spellings of `root`. `definition` returns the definition of an operand when it
    /// may be commutative, and `movable` tells whether a value may change sides of its operation.
    pub(super) fn new(
        root: &Op,
        definition: impl Fn(ValueId) -> Option<Op>,
        movable: impl Fn(ValueId) -> bool,
    ) -> Self {
        let mut sites = Commuted::new();
        let _ = root.map_values(|operand| {
            if !sites.contains(&operand)
                && definition(operand).is_some_and(|definition| swappable(&definition, &movable))
            {
                sites.push(operand);
            }
            operand
        });
        Self { swap_root: swappable(root, &movable), sites }
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
