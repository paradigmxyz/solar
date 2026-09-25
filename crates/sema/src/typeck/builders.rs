//! Encapsulation of the builders of `solar:core/v1/Buffers.sol`.
//!
//! A `ByteBuilder` or `WordBuilder` keeps what was written apart from the capacity behind it:
//! `append` writes below `used`, and `finish` returns exactly the written part. Code outside the
//! module that reads or writes a field, or makes a builder from parts, could see bytes that were
//! never written or break that length, so this compiler rejects it. Other compilers read the
//! fields' documentation instead, which says the same.

use crate::{
    hir::{self, ExprKind, Visit},
    ty::{Gcx, TyKind},
};
use solar_data_structures::Never;
use solar_interface::source_map::FileName;
use std::ops::ControlFlow;

/// The module that owns the builders.
const BUFFERS: &str = "solar:core/v1/Buffers.sol";

pub(super) fn check(gcx: Gcx<'_>) {
    for source in gcx.hir.source_ids() {
        if !is_buffers(gcx, source) {
            let _ = BuilderFields { gcx }.visit_nested_source(source);
        }
    }
}

fn is_buffers(gcx: Gcx<'_>, source: hir::SourceId) -> bool {
    matches!(&gcx.hir.source(source).file.name, FileName::Custom(path) if path == BUFFERS)
}

/// Reports every field access and construction of a builder.
struct BuilderFields<'gcx> {
    gcx: Gcx<'gcx>,
}

impl<'gcx> BuilderFields<'gcx> {
    /// The builder struct `ty` is, when it is one.
    fn builder(&self, ty: crate::ty::Ty<'gcx>) -> Option<hir::StructId> {
        let TyKind::Struct(id) = ty.peel_refs().kind else { return None };
        is_buffers(self.gcx, self.gcx.hir.strukt(id).source).then_some(id)
    }
}

impl<'gcx> Visit<'gcx> for BuilderFields<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            // A member that is a function is one attached with `using for`.
            ExprKind::Member(base, field)
                if let Some(id) =
                    self.gcx.type_of_expr(base.id).and_then(|ty| self.builder(ty))
                    && !self
                        .gcx
                        .type_of_expr(expr.id)
                        .is_some_and(|ty| matches!(ty.kind, TyKind::Fn(_))) =>
            {
                let name = self.gcx.hir.strukt(id).name;
                self.gcx
                    .dcx()
                    .err(format!("the fields of `{name}` belong to `Buffers`"))
                    .span(field.span)
                    .note(
                        "a builder keeps what was written apart from its capacity, which a \
                         field read could expose and a field write could break",
                    )
                    .help("use `Buffers.length` and `Buffers.finish`")
                    .emit();
            }
            ExprKind::Call(callee, ..)
                if let Some(TyKind::Type(ty)) =
                    self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
                    && let Some(id) = self.builder(ty) =>
            {
                let name = self.gcx.hir.strukt(id).name;
                self.gcx
                    .dcx()
                    .err(format!("a `{name}` can only be made by `Buffers`"))
                    .span(expr.span)
                    .note("a builder made from parts could claim bytes that were never written")
                    .help("use `Buffers.create` or `Buffers.createWords`")
                    .emit();
            }
            _ => {}
        }
        self.walk_expr(expr)
    }
}
