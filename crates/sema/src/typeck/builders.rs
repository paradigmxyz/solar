//! Encapsulation of the builders of `solar:core/v1/Buffers.sol`.
//!
//! A `ByteBuilder` or `WordBuilder` keeps what was written apart from the capacity behind it:
//! `append` writes below `used`, and `finish` returns exactly the written part. Code outside the
//! module that reads or writes a field, or makes a builder from parts, directly or with
//! `abi.decode`, could see bytes that were never written or break that length, so this compiler
//! rejects it. Other compilers read the fields' documentation instead, which says the same.
//!
//! A builder is also a value of its own, never part of another type: code generation follows
//! builders as values to reject a use after `finish`, which it could not do through a struct
//! field, an array element, or a mapping value.
//!
//! Encoding or storing a whole builder would copy its capacity as well, bytes that were never
//! written, so a builder also stays in memory and out of the ABI: no state or storage variable,
//! no parameter or return value of a public or external function or of an external function
//! type, no event, error or `try` clause parameter, and no argument of `abi.encode` and its
//! variants holds one.

use crate::{
    builtins::Builtin,
    hir::{self, ExprKind, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_ast::DataLocation;
use solar_data_structures::Never;
use solar_interface::{Span, source_map::FileName};
use std::ops::ControlFlow;

/// The module that owns the builders.
const BUFFERS: &str = "solar:core/v1/Buffers.sol";

pub(super) fn check(gcx: Gcx<'_>) {
    for source in gcx.hir.source_ids() {
        if !is_buffers(gcx, source) {
            let _ = BuilderFields { gcx }.visit_nested_source(source);
        }
    }
    check_declarations(gcx);
}

/// Rejects every declaration outside `Buffers` whose type holds a builder inside another type,
/// or that would store a builder or pass one through the ABI.
fn check_declarations(gcx: Gcx<'_>) {
    let fields = BuilderFields { gcx };
    for id in gcx.hir.variable_ids() {
        let variable = gcx.hir.variable(id);
        if is_buffers(gcx, variable.source) {
            continue;
        }
        let ty = gcx.type_of_item(id.into());
        // A field holds its type inside the struct; any other variable holds only what its type
        // nests.
        let nested = match variable.kind {
            hir::VarKind::Struct => fields.holds_builder(ty),
            _ => fields.nests_builder(ty),
        };
        if let Some(id) = nested {
            gcx.dcx()
                .err(format!(
                    "a `{}` cannot be a struct field, an array element, or a mapping value",
                    gcx.hir.strukt(id).name
                ))
                .span(variable.ty.span)
                .note(
                    "builders are followed as values to reject a use after `finish`, which \
                     memory would hide",
                )
                .emit();
        } else if let Some(id) = fields.builder(ty)
            && let Some(place) = escaping_place(gcx, variable)
        {
            fields.report_escape(id, variable.ty.span, place);
        } else if let Some(id) = fields.external_signature_builder(ty) {
            fields.report_escape(id, variable.ty.span, "an external function type");
        }
    }
}

/// Where a builder declared as `variable` would be stored or encoded, when it would be.
fn escaping_place(gcx: Gcx<'_>, variable: &hir::Variable<'_>) -> Option<&'static str> {
    if matches!(variable.data_location, Some(DataLocation::Storage | DataLocation::Calldata))
        || matches!(variable.kind, hir::VarKind::State | hir::VarKind::Global)
    {
        return Some("storage");
    }
    match variable.kind {
        hir::VarKind::Event => Some("an event"),
        hir::VarKind::Error => Some("an error"),
        hir::VarKind::TryCatch => Some("a `try` clause"),
        hir::VarKind::FunctionParam | hir::VarKind::FunctionReturn => {
            let Some(hir::ItemId::Function(function)) = variable.parent else { return None };
            let function = gcx.hir.function(function);
            (function.visibility >= hir::Visibility::Public
                || function.kind == hir::FunctionKind::Constructor)
                .then_some("a public or external function")
        }
        _ => None,
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

    /// The builder `ty` is or holds as array elements or mapping values, when there is one.
    fn holds_builder(&self, ty: crate::ty::Ty<'gcx>) -> Option<hir::StructId> {
        self.builder(ty).or_else(|| self.nests_builder(ty))
    }

    /// The builder `ty` holds as array elements or mapping values, when there is one.
    fn nests_builder(&self, ty: crate::ty::Ty<'gcx>) -> Option<hir::StructId> {
        match ty.peel_refs().kind {
            TyKind::Array(element, _) | TyKind::DynArray(element) | TyKind::Slice(element) => {
                self.holds_builder(element)
            }
            TyKind::Mapping(_, value) => self.holds_builder(value),
            _ => None,
        }
    }

    /// The builder an external function type `ty` takes or returns, when there is one.
    fn external_signature_builder(&self, ty: Ty<'gcx>) -> Option<hir::StructId> {
        let TyKind::Fn(function) = ty.peel_refs().kind else { return None };
        if !function.is_external() {
            return None;
        }
        function.parameters.iter().chain(function.returns).find_map(|&ty| self.holds_builder(ty))
    }

    /// The builder `ty` holds, including as a tuple component, when there is one.
    fn encoded_builder(&self, ty: Ty<'gcx>) -> Option<hir::StructId> {
        match ty.kind {
            TyKind::Tuple(components) => {
                components.iter().find_map(|&component| self.encoded_builder(component))
            }
            _ => self.holds_builder(ty),
        }
    }

    /// Reports a builder that `place` would store or encode.
    fn report_escape(&self, id: hir::StructId, span: Span, place: &str) {
        let name = self.gcx.hir.strukt(id).name;
        self.gcx
            .dcx()
            .err(format!("a `{name}` cannot be stored or encoded"))
            .span(span)
            .note(format!(
                "{place} would copy the builder's capacity, bytes that were never written, as well"
            ))
            .help("use what `Buffers.finish` returns instead")
            .emit();
    }

    /// The first type among the `abi.decode` target `types` that holds a builder, with its span.
    fn decoded_builder(&self, types: &hir::Expr<'_>) -> Option<(Span, hir::StructId)> {
        let types = match types.peel_parens().kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![types.peel_parens()],
        };
        types.into_iter().find_map(|ty_expr| {
            let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind) else {
                return None;
            };
            self.holds_builder(ty).map(|id| (ty_expr.span, id))
        })
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
            ExprKind::Call(..)
                if let Some((callee, args, _)) = expr.as_call()
                    && self.gcx.resolved_builtin(callee) == Some(Builtin::AbiDecode)
                    && let Some(types) = args.exprs().nth(1)
                    && let Some((span, id)) = self.decoded_builder(types) =>
            {
                let name = self.gcx.hir.strukt(id).name;
                self.gcx
                    .dcx()
                    .err(format!("a `{name}` can only be made by `Buffers`"))
                    .span(span)
                    .note("a decoded builder could claim bytes that were never written")
                    .emit();
            }
            ExprKind::Call(..)
                if let Some((callee, args, _)) = expr.as_call()
                    && matches!(
                        self.gcx.resolved_builtin(callee),
                        Some(
                            Builtin::AbiEncode
                                | Builtin::AbiEncodePacked
                                | Builtin::AbiEncodeWithSelector
                                | Builtin::AbiEncodeWithSignature
                                | Builtin::AbiEncodeCall
                        )
                    ) =>
            {
                for argument in args.exprs() {
                    if let Some(ty) = self.gcx.type_of_expr(argument.id)
                        && let Some(id) = self.encoded_builder(ty)
                    {
                        self.report_escape(id, argument.span, "the encoding");
                    }
                }
            }
            ExprKind::Call(..)
                if let Some((callee, _, _)) = expr.as_call()
                    && let Some(TyKind::Type(ty)) =
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
