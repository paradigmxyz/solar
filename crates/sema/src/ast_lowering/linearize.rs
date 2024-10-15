//! Performs the [C3 linearization algorithm] on all contracts in the HIR.
//!
//! Modified from [`solc`].
//!
//! See also: <https://docs.soliditylang.org/en/latest/contracts.html#multiple-inheritance-and-linearization>
//!
//! [C3 linearization algorithm]: https://en.wikipedia.org/wiki/C3_linearization
//! [`solc`]: https://github.com/ethereum/solidity/blob/2694190d1dbbc90b001aa76f8d7bd0794923c343/libsolidity/analysis/NameAndTypeResolver.cpp#L403

use super::Res;
use crate::hir;

impl super::LoweringContext<'_, '_, '_> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn linearize_contracts(&mut self) {
        // Must iterate in source order.
        let mut linearizer = ContractLinearizer::new();
        for source in &self.hir.sources {
            for contract_id in source.items.iter().filter_map(hir::ItemId::as_contract) {
                self.linearize_contract(contract_id, &mut linearizer);
                if linearizer.result.is_empty() {
                    let msg = "linearization of inheritance graph impossible";
                    self.dcx().err(msg).span(self.hir.contract(contract_id).name.span).emit();
                    continue;
                }
                let linearized_bases = &*self.arena.alloc_slice_copy(&linearizer.result);
                self.hir.contracts[contract_id].linearized_bases = linearized_bases;

                // Import inherited scopes.
                // https://github.com/ethereum/solidity/blob/2694190d1dbbc90b001aa76f8d7bd0794923c343/libsolidity/analysis/NameAndTypeResolver.cpp#L352
                for &base_id in &linearized_bases[1..] {
                    let (base_scope, contract_scope) = super::get_two_mut_idx(
                        &mut self.resolver.contract_scopes,
                        base_id,
                        contract_id,
                    );
                    for (&name, decls) in &base_scope.declarations {
                        for &decl in decls {
                            // Import if it was declared in the base, is not the constructor and is
                            // visible in derived classes.
                            let Res::Item(decl_item_id) = decl.kind else { continue };
                            let decl_item = self.hir.item(decl_item_id);
                            if decl_item.contract() != Some(base_id) {
                                continue;
                            }
                            if !decl_item.is_visible_in_derived_contracts() {
                                continue;
                            }

                            if let Err(conflict) = contract_scope.try_declare(&self.hir, name, decl)
                            {
                                use hir::ItemId::*;
                                use Res::*;

                                let Item(conflict_id) = conflict.kind else { continue };
                                match (decl_item_id, conflict_id) {
                                    // Usual shadowing is not an error.
                                    (Function(a), Function(b)) => {
                                        let a = self.hir.function(a);
                                        let b = self.hir.function(b);
                                        if a.kind.is_modifier() && b.kind.is_modifier() {
                                            continue;
                                        }
                                    }
                                    // Public state variable can override functions.
                                    (Function(_a), Variable(b)) => {
                                        let v = self.hir.variable(b);
                                        if v.is_state_variable() && v.is_public() {
                                            continue;
                                        }
                                    }
                                    _ => {}
                                }

                                super::resolve::report_conflict(
                                    &self.hir, self.sess, name, conflict, decl,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn linearize_contract(
        &self,
        contract_id: hir::ContractId,
        linearizer: &mut ContractLinearizer,
    ) {
        let contract = self.hir.contract(contract_id);
        if contract.bases.is_empty() {
            linearizer.result.clear();
            linearizer.result.push(contract_id);
            return;
        }

        linearizer.reset();

        for &base_id in contract.bases {
            linearizer.insert(base_id);
            let base = self.hir.contract(base_id);
            let base_bases = base.linearized_bases;
            if base_bases.is_empty() {
                let msg = "definition of base has to precede definition of derived contract";
                self.dcx().err(msg).span(contract.name.span).emit();
                continue;
            }
            linearizer.insert_bases(base_bases);
        }
        linearizer.insert(contract_id);
        linearizer.c3_merge();
    }
}

struct ContractLinearizer {
    to_merge: List<List<hir::ContractId>>,
    result: Vec<hir::ContractId>,
}

impl ContractLinearizer {
    fn new() -> Self {
        Self { to_merge: List::new(), result: Vec::with_capacity(16) }
    }

    fn reset(&mut self) {
        self.to_merge.clear();
        self.to_merge.push_back(List::new());
        self.result.clear();
    }

    fn insert(&mut self, id: hir::ContractId) {
        self.to_merge.back_mut().unwrap().push_front(id);
    }

    fn insert_bases(&mut self, ids: &[hir::ContractId]) {
        self.to_merge.push_front(ids.iter().copied().collect());
    }

    fn c3_merge(&mut self) {
        self.remove_empty();
        while !self.is_empty() {
            let Some(candidate) = self.next_candidate() else {
                self.result.clear();
                return;
            };
            self.result.push(candidate);
            self.remove_candidate(candidate);
            self.remove_empty();
        }
    }

    /// Removes the given contract from all lists.
    fn remove_candidate(&mut self, candidate: hir::ContractId) {
        for list in self.iter_mut() {
            list.retain(|c| *c != candidate);
        }
    }

    /// Returns the next candidate to append to the linearized list, if any.
    fn next_candidate(&self) -> Option<hir::ContractId> {
        for base in self.iter() {
            let candidate = *base.front().unwrap();
            if self.appears_only_at_head(candidate) {
                return Some(candidate);
            }
        }
        None
    }

    /// Returns `true` if `candidate` appears only as last element of the lists.
    fn appears_only_at_head(&self, candidate: hir::ContractId) -> bool {
        for list in self.iter() {
            let mut list = list.iter();
            list.next().unwrap();
            if list.any(|c| *c == candidate) {
                return false;
            }
        }
        true
    }

    fn remove_empty(&mut self) {
        self.to_merge.retain(|list| !list.is_empty());
    }

    fn iter(&self) -> impl Iterator<Item = &List<hir::ContractId>> {
        self.to_merge.iter()
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut List<hir::ContractId>> {
        self.to_merge.iter_mut()
    }

    fn is_empty(&self) -> bool {
        self.to_merge.is_empty()
    }
}

// LinkedList is actually ~30% faster than VecDeque due to `retain` being very hot, however
// `LinkedList::retain` is unstable.
// My measurements weren't really scientific, so not really running with this.
// #[cfg(feature = "nightly")]
// type List<T> = std::collections::LinkedList<T>;
// #[cfg(not(feature = "nightly"))]
type List<T> = std::collections::VecDeque<T>;
