use crate::hir;

impl<'gcx> super::super::LoweringContext<'gcx> {
    pub(super) fn items_have_matching_signature(
        &self,
        item_id: hir::ItemId,
        base_item_id: hir::ItemId,
    ) -> bool {
        match (item_id, base_item_id) {
            (hir::ItemId::Function(id), hir::ItemId::Function(base_id)) => {
                let item = self.hir.function(id);
                let base = self.hir.function(base_id);
                item.kind == base.kind
                    && self.variable_types_match(item.parameters, base.parameters)
            }
            (hir::ItemId::Variable(_), hir::ItemId::Variable(_)) => true,
            _ => false,
        }
    }

    fn variable_types_match(&self, a: &[hir::VariableId], b: &[hir::VariableId]) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(&a, &b)| {
                self.hir_types_match(&self.hir.variable(a).ty, &self.hir.variable(b).ty)
            })
    }

    fn hir_types_match(&self, a: &hir::Type<'_>, b: &hir::Type<'_>) -> bool {
        match (&a.kind, &b.kind) {
            (hir::TypeKind::Elementary(a), hir::TypeKind::Elementary(b)) => a == b,
            (hir::TypeKind::Custom(a), hir::TypeKind::Custom(b)) => a == b,
            (hir::TypeKind::Array(a), hir::TypeKind::Array(b)) => {
                self.hir_types_match(&a.element, &b.element)
                    && self.array_sizes_match(a.size, b.size)
            }
            (hir::TypeKind::Function(a), hir::TypeKind::Function(b)) => {
                a.visibility == b.visibility
                    && a.state_mutability == b.state_mutability
                    && self.variable_types_match(a.parameters, b.parameters)
                    && self.variable_types_match(a.returns, b.returns)
            }
            (hir::TypeKind::Mapping(a), hir::TypeKind::Mapping(b)) => {
                self.hir_types_match(&a.key, &b.key) && self.hir_types_match(&a.value, &b.value)
            }
            _ => false,
        }
    }

    fn array_sizes_match(&self, a: Option<&hir::Expr<'_>>, b: Option<&hir::Expr<'_>>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => self
                .sess
                .source_map()
                .span_to_snippet(a.span)
                .is_ok_and(|a| self.sess.source_map().span_to_snippet(b.span) == Ok(a)),
            _ => false,
        }
    }
}
