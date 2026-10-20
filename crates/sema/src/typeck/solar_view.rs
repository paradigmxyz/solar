//! `@custom:solar-view`: values a calldata `abi.decode` makes, read where they are.
//!
//! The tag documents a declaration initialized by `abi.decode` of `bytes` in calldata:
//!
//! ```solidity
//! /// @custom:solar-view
//! (uint256 id, bytes memory payload) = abi.decode(data, (uint256, bytes));
//! ```
//!
//! Every compiler gives the `bytes` and `string` variables copies of their bytes. Other compilers
//! read the tag as documentation and make the copies; this compiler binds each such variable, a
//! view, to its bytes where the decode found them in calldata, which never change. The view is
//! equivalent to the copy as long as nothing can tell the two apart, so type checking turns the
//! tag into a requirement on the program: a view can only be read in place. Its `.length`, an
//! index read, `keccak256`, `abi.decode`, and the ABI encodings that copy their arguments into
//! their output (`abi.encode` and its variants, `abi.encodePacked`, `bytes.concat` and
//! `string.concat`) read it, also through a conversion between `bytes` and `string`, and a tagged
//! decode of a view makes views of the same calldata. Any other use, such as an assignment, a
//! write through the view, a return, a call argument, or inline assembly, could keep the view or
//! write through it, and is an error at the use.
//!
//! The decoded types may be value types, `bytes` and `string`. The copying decode allocates every
//! `bytes` and `string` value it makes, and the view's decode replays those allocations' checks
//! against the free memory pointer in the same order, so it fails exactly where the copy fails.
//! An array or a struct would need allocations of its own, so it is an error in the decoded types.

use crate::{
    builtins::Builtin,
    hir::{self, ExprKind, StmtKind, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_ast::DataLocation;
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Span, kw, sym};
use std::ops::ControlFlow;

/// Checks every `@custom:solar-view` declaration and the uses of its views.
pub(super) fn check(gcx: Gcx<'_>) {
    if !gcx.hir.has_solar_views() {
        return;
    }
    for id in gcx.hir.function_ids() {
        let mut checker = Checker {
            gcx,
            views: FxHashMap::default(),
            in_place: FxHashSet::default(),
            writes: FxHashSet::default(),
        };
        let _ = checker.visit_nested_function(id);
    }
}

struct Checker<'gcx> {
    gcx: Gcx<'gcx>,
    /// The views declared so far, with the spans of their variables.
    views: FxHashMap<hir::VariableId, Span>,
    /// Expressions that read a view in place.
    in_place: FxHashSet<hir::ExprId>,
    /// Index expressions that are written, not read.
    writes: FxHashSet<hir::ExprId>,
}

impl<'gcx> Checker<'gcx> {
    /// Checks a tagged declaration of `ids` initialized by `initializer`, and records its views.
    fn declaration(
        &mut self,
        ids: &[Option<hir::VariableId>],
        initializer: Option<&hir::Expr<'_>>,
        tag: Span,
        span: Span,
    ) {
        let decode = initializer
            .and_then(|initializer| initializer.peel_parens().as_call())
            .filter(|(callee, ..)| self.gcx.resolved_builtin(callee) == Some(Builtin::AbiDecode));
        let Some((_, args, _)) = decode else {
            self.gcx
                .dcx()
                .err("`@custom:solar-view` requires a declaration initialized by `abi.decode`")
                .span(span)
                .span_note(tag, "the tag is here")
                .help("declare the views as `(bytes memory payload) = abi.decode(data, (bytes));`")
                .emit();
            return;
        };
        let mut exprs = args.exprs();
        let (Some(data), Some(types)) = (exprs.next(), exprs.next()) else { return };
        if let Some(ty) = self.gcx.type_of_expr(data.id)
            && !is_calldata_bytes(ty)
            && !self.is_view(data)
        {
            self.gcx
                .dcx()
                .err("`@custom:solar-view` decodes only `bytes` held in calldata")
                .span(data.span)
                .span_note(tag, "the tag is here")
                .note("calldata cannot change, so a view of it always reads what a copy would hold")
                .emit();
            return;
        }
        let types = match types.kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![types],
        };
        for ty_expr in types {
            if let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind)
                && is_reference(ty)
                && !is_bytes_or_string(ty)
            {
                self.gcx
                    .dcx()
                    .err("`@custom:solar-view` decodes only value types, `bytes`, and `string`")
                    .span(ty_expr.span)
                    .span_note(tag, "the tag is here")
                    .help("decode the arrays and structs in a declaration without the tag")
                    .emit();
                return;
            }
        }
        let views = ids
            .iter()
            .flatten()
            .filter(|&&id| is_bytes_or_string(self.gcx.type_of_item(id.into())))
            .copied()
            .collect::<Vec<_>>();
        if views.is_empty() {
            self.gcx
                .dcx()
                .err("`@custom:solar-view` declares no `bytes` or `string` variable")
                .span(span)
                .span_note(tag, "the tag is here")
                .help("only `bytes` and `string` variables are views; drop the tag")
                .emit();
            return;
        }
        for id in views {
            self.views.insert(id, self.gcx.hir.variable(id).span);
        }
    }

    /// Whether `expr` names a view, through parentheses and conversions between `bytes` and
    /// `string`.
    fn is_view(&self, mut expr: &hir::Expr<'_>) -> bool {
        loop {
            expr = expr.peel_parens();
            match expr.kind {
                ExprKind::Call(callee, ref args)
                    if matches!(callee.kind, ExprKind::Type(_))
                        && self.gcx.type_of_expr(expr.id).is_some_and(is_bytes_or_string) =>
                {
                    let Some(value) = args.exprs().next() else { return false };
                    expr = value;
                }
                _ => {
                    return self
                        .gcx
                        .resolved_variable(expr)
                        .is_some_and(|id| self.views.contains_key(&id));
                }
            }
        }
    }

    /// Marks `expr` as reading a view in place when it names one, through parentheses.
    fn read(&mut self, expr: &hir::Expr<'_>) {
        self.in_place.insert(expr.peel_parens().id);
    }
}

impl<'gcx> Visit<'gcx> for Checker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        match stmt.kind {
            StmtKind::DeclSingle(id) => {
                if let Some(tag) = self.gcx.hir.solar_view(id) {
                    let initializer = self.gcx.hir.variable(id).initializer;
                    self.declaration(&[Some(id)], initializer, tag, stmt.span);
                }
            }
            StmtKind::DeclMulti(ids, expr) => {
                if let Some(tag) = ids.iter().flatten().find_map(|&id| self.gcx.hir.solar_view(id))
                {
                    self.declaration(ids, Some(expr), tag, stmt.span);
                }
            }
            _ => {}
        }
        self.walk_stmt(stmt)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            // A write through an index, or one that reads and writes.
            ExprKind::Assign(lhs, ..) | ExprKind::Delete(lhs) => {
                self.writes.insert(lhs.peel_parens().id);
            }
            ExprKind::Unary(op, operand) if op.kind.has_side_effects() => {
                self.writes.insert(operand.peel_parens().id);
            }
            ExprKind::Member(receiver, name) if name.name == sym::length => self.read(receiver),
            ExprKind::Index(receiver, Some(_)) if !self.writes.contains(&expr.id) => {
                self.read(receiver);
            }
            ExprKind::Call(callee, ref args) => {
                let mut exprs = args.exprs();
                match self.gcx.resolved_builtin(callee) {
                    Some(Builtin::Keccak256 | Builtin::AbiDecode) => {
                        if let Some(data) = exprs.next() {
                            self.read(data);
                        }
                    }
                    Some(
                        Builtin::AbiEncode
                        | Builtin::AbiEncodePacked
                        | Builtin::BytesConcat
                        | Builtin::StringConcat,
                    ) => exprs.for_each(|arg| self.read(arg)),
                    Some(Builtin::AbiEncodeWithSelector | Builtin::AbiEncodeWithSignature) => {
                        exprs.skip(1).for_each(|arg| self.read(arg));
                    }
                    Some(Builtin::AbiEncodeCall) => {
                        if let Some(arguments) = exprs.nth(1) {
                            match arguments.peel_parens().kind {
                                ExprKind::Tuple(values) => {
                                    values.iter().flatten().for_each(|arg| self.read(arg));
                                }
                                _ => self.read(arguments),
                            }
                        }
                    }
                    // A conversion between `bytes` and `string` reads what the conversion's
                    // own use reads.
                    _ if self.in_place.contains(&expr.id)
                        && matches!(callee.kind, ExprKind::Type(_))
                        && self.gcx.type_of_expr(expr.id).is_some_and(is_bytes_or_string) =>
                    {
                        if let Some(value) = exprs.next() {
                            self.read(value);
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::Ident(_) => {
                if let Some(id) = self.gcx.resolved_variable(expr)
                    && let Some(&declaration) = self.views.get(&id)
                    && !self.in_place.contains(&expr.id)
                {
                    let name = self.gcx.hir.variable(id).name.map_or(kw::Empty, |name| name.name);
                    self.gcx
                        .dcx()
                        .err(format!("the view `{name}` can only be read in place"))
                        .span(expr.span)
                        .span_note(declaration, "the view is declared here")
                        .help(
                            "read it with `.length`, an index, `keccak256`, `abi.decode`, or an \
                             ABI encoding, or declare it without `@custom:solar-view` to hold a \
                             copy",
                        )
                        .emit();
                }
            }
            _ => {}
        }
        self.walk_expr(expr)
    }
}

/// Whether `ty` is `bytes`, or a slice of it, in calldata.
fn is_calldata_bytes(ty: Ty<'_>) -> bool {
    let ty = match ty.kind {
        TyKind::Slice(inner) => inner,
        _ => ty,
    };
    ty.is_ref_at(DataLocation::Calldata)
        && matches!(ty.peel_refs().kind, TyKind::Elementary(hir::ElementaryType::Bytes))
}

/// Whether `ty` is a reference type, which the copying decode allocates.
fn is_reference(ty: Ty<'_>) -> bool {
    matches!(ty.kind, TyKind::Ref(..))
        || matches!(
            ty.kind,
            TyKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String)
                | TyKind::Array(..)
                | TyKind::DynArray(..)
                | TyKind::Struct(..)
        )
}

/// Whether `ty` is `bytes` or `string`, in any location.
fn is_bytes_or_string(ty: Ty<'_>) -> bool {
    matches!(
        ty.peel_refs().kind,
        TyKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String)
    )
}
