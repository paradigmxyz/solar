//! Opacity of the `CodeView` of `solar:core/Code.sol`.
//!
//! `Code.slice` makes a view only of a range that lies inside an account's code, and packs the
//! account and the range into one word. Code outside the module that wraps a word into a view or
//! decodes one could hold a range nobody checked, and code that unwraps one would depend on the
//! packing, so this compiler rejects both; other compilers read the type's documentation, which
//! says the same. A view that arrives from outside, as an external function's parameter, is still
//! safe to read: every read checks the range against the code again, since code can change after a
//! view is made.

use crate::{
    builtins::Builtin,
    hir::{self, ExprKind, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_data_structures::{Never, map::FxHashSet};
use solar_interface::{Span, source_map::FileName};
use std::ops::ControlFlow;

/// The module that owns `CodeView`.
const CODE: &str = "solar:core/Code.sol";

pub(super) fn check(gcx: Gcx<'_>) {
    // Most compilations never import the module.
    if !gcx.hir.source_ids().any(|source| is_code(gcx, source)) {
        return;
    }
    for source in gcx.hir.source_ids() {
        if !is_code(gcx, source) {
            let _ = CodeViews { gcx }.visit_nested_source(source);
        }
    }
}

fn is_code(gcx: Gcx<'_>, source: hir::SourceId) -> bool {
    matches!(&gcx.hir.source(source).file.name, FileName::Custom(path) if path == CODE)
}

/// Reports every construction and unpacking of a `CodeView` outside `Code`.
struct CodeViews<'gcx> {
    gcx: Gcx<'gcx>,
}

impl<'gcx> CodeViews<'gcx> {
    /// Whether `ty` is `CodeView`.
    fn is_view(&self, ty: Ty<'gcx>) -> bool {
        matches!(ty.peel_refs().kind, TyKind::Udvt(_, id) if is_code(self.gcx, self.gcx.hir.udvt(id).source))
    }

    /// Whether `ty` holds a `CodeView`, as itself, an element, or a field.
    fn holds_view(&self, ty: Ty<'gcx>) -> bool {
        self.holds_view_in(ty, &mut FxHashSet::default())
    }

    /// [`Self::holds_view`], looking into each struct once: a struct that contains itself holds a
    /// view only through its other fields, which the first visit searches.
    fn holds_view_in(&self, ty: Ty<'gcx>, structs: &mut FxHashSet<hir::StructId>) -> bool {
        match ty.peel_refs().kind {
            _ if self.is_view(ty) => true,
            TyKind::Array(element, _) | TyKind::DynArray(element) => {
                self.holds_view_in(element, structs)
            }
            TyKind::Struct(id) => {
                structs.insert(id)
                    && self
                        .gcx
                        .struct_field_types(id)
                        .iter()
                        .any(|&field| self.holds_view_in(field, structs))
            }
            _ => false,
        }
    }

    /// The span of the first `abi.decode` target type among `types` that holds a `CodeView`.
    fn decoded_view(&self, types: &hir::Expr<'_>) -> Option<Span> {
        let types = match types.peel_parens().kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![types.peel_parens()],
        };
        types.into_iter().find_map(|ty_expr| {
            let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind) else {
                return None;
            };
            self.holds_view(ty).then_some(ty_expr.span)
        })
    }

    fn report_made(&self, span: Span, note: &str) {
        self.gcx
            .dcx()
            .err("a `CodeView` can only be made by `Code`")
            .span(span)
            .note(note.to_string())
            .help("use `Code.slice`")
            .emit();
    }
}

impl<'gcx> Visit<'gcx> for CodeViews<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let Some((callee, args, _)) = expr.as_call() {
            match self.gcx.resolved_builtin(callee) {
                Some(builtin @ (Builtin::UdvtWrap | Builtin::UdvtUnwrap))
                    if let ExprKind::Member(base, _) = callee.kind
                        && let Some(TyKind::Type(ty)) =
                            self.gcx.type_of_expr(base.id).map(|ty| ty.kind)
                        && self.is_view(ty) =>
                {
                    if builtin == Builtin::UdvtWrap {
                        self.report_made(
                            expr.span,
                            "a view made from a word could hold a range nobody checked",
                        );
                    } else {
                        self.gcx
                            .dcx()
                            .err("the contents of a `CodeView` belong to `Code`")
                            .span(expr.span)
                            .help("use `Code.account`, `Code.offset` and `Code.length`")
                            .emit();
                    }
                }
                Some(Builtin::AbiDecode)
                    if let Some(types) = args.exprs().nth(1)
                        && let Some(span) = self.decoded_view(types) =>
                {
                    self.report_made(span, "a decoded view could hold a range nobody checked");
                }
                _ => {}
            }
        }
        self.walk_expr(expr)
    }
}
