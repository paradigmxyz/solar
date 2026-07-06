use crate::proto;
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range, Url};
use solar_interface::{
    Symbol,
    data_structures::{Never, map::FxHashMap},
};
use solar_sema::{
    Gcx,
    hir::{self, CallArgsKind, ExprKind, ItemId, Visit},
    ty::{CallableParamSource, TyKind},
};
use std::ops::ControlFlow;

#[derive(Clone, Debug, Default)]
pub(crate) struct InlayHintIndex {
    by_file: FxHashMap<Url, Vec<StoredInlayHint>>,
}

#[derive(Clone, Debug)]
struct StoredInlayHint {
    position: Position,
    // Labels are built once during analysis and only copied into LSP responses,
    // so store the fixed text without String's unused capacity.
    label: Box<str>,
    kind: StoredInlayHintKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StoredInlayHintKind {
    Parameter,
    CallType,
}

impl InlayHintIndex {
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut collector = InlayHintCollector { gcx, index: Self::default() };
        for source_id in gcx.hir.source_ids() {
            let _ = collector.visit_nested_source(source_id);
        }
        collector.index.sort();
        collector.index
    }

    pub(crate) fn extend(&mut self, other: Self) {
        for (uri, hints) in other.by_file {
            self.by_file.entry(uri).or_default().extend(hints);
        }
        self.sort();
    }

    pub(crate) fn hints(&self, uri: &Url, range: Range) -> Vec<InlayHint> {
        let Some(hints) = self.by_file.get(uri) else {
            return Vec::new();
        };
        let start = hints.partition_point(|hint| hint.position < range.start);
        let include_end = range.start == range.end;
        hints[start..]
            .iter()
            .take_while(|hint| {
                hint.position < range.end || (include_end && hint.position == range.end)
            })
            .map(StoredInlayHint::to_lsp)
            .collect()
    }

    fn push(&mut self, uri: Url, hint: StoredInlayHint) {
        self.by_file.entry(uri).or_default().push(hint);
    }

    fn sort(&mut self) {
        for hints in self.by_file.values_mut() {
            hints.sort_by(|a, b| hint_sort_key(a).cmp(&hint_sort_key(b)));
        }
    }
}

impl StoredInlayHint {
    fn parameter(position: Position, label: impl Into<Box<str>>) -> Self {
        Self { position, label: label.into(), kind: StoredInlayHintKind::Parameter }
    }

    fn call_type(position: Position, label: impl Into<Box<str>>) -> Self {
        Self { position, label: label.into(), kind: StoredInlayHintKind::CallType }
    }

    fn to_lsp(&self) -> InlayHint {
        let (kind, padding_left, padding_right) = self.kind.lsp_fields();
        InlayHint {
            position: self.position,
            label: InlayHintLabel::String(self.label.to_string()),
            kind: Some(kind),
            text_edits: None,
            tooltip: None,
            padding_left: Some(padding_left),
            padding_right: Some(padding_right),
            data: None,
        }
    }
}

impl StoredInlayHintKind {
    fn lsp_fields(self) -> (InlayHintKind, bool, bool) {
        match self {
            Self::Parameter => (InlayHintKind::PARAMETER, false, true),
            Self::CallType => (InlayHintKind::TYPE, true, false),
        }
    }
}

struct InlayHintCollector<'gcx> {
    gcx: Gcx<'gcx>,
    index: InlayHintIndex,
}

impl<'gcx> InlayHintCollector<'gcx> {
    fn push_parameter_hints(
        &mut self,
        args: &hir::CallArgs<'gcx>,
        param_source: Option<CallableParamSource>,
    ) {
        let (CallArgsKind::Unnamed(exprs), Some(param_source)) = (args.kind, param_source) else {
            return;
        };
        let param_names = self.gcx.callable_param_names(param_source);
        for (arg, param_name) in exprs.iter().zip(param_names) {
            let Some(param_name) = param_name else {
                continue;
            };
            if self.argument_name_matches_param(arg, param_name) {
                continue;
            }
            let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), arg.span)
            else {
                continue;
            };
            self.index.push(
                location.uri,
                StoredInlayHint::parameter(location.range.start, format!("{param_name}:")),
            );
        }
    }

    fn argument_name_matches_param(&self, arg: &'gcx hir::Expr<'gcx>, param_name: Symbol) -> bool {
        let arg = arg.peel_parens();
        self.gcx
            .sess
            .source_map()
            .span_to_snippet(arg.span)
            .is_ok_and(|snippet| snippet == param_name.as_str())
    }

    fn push_call_type_hint(&mut self, expr: &'gcx hir::Expr<'gcx>, callee: &'gcx hir::Expr<'gcx>) {
        if self.is_explicit_cast_call(callee) {
            return;
        }
        let Some(ty) = self.gcx.type_of_expr(expr.id) else {
            return;
        };
        if ty.is_unit() || ty.references_error() {
            return;
        }
        let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), expr.span) else {
            return;
        };
        self.index.push(
            location.uri,
            StoredInlayHint::call_type(location.range.end, format!(": {}", ty.display(self.gcx))),
        );
    }

    fn call_param_source(&self, callee: &'gcx hir::Expr<'gcx>) -> Option<CallableParamSource> {
        if let ExprKind::New(hir_ty) = &callee.kind
            && let TyKind::Contract(id) = self.gcx.type_of_hir_ty(hir_ty).kind
            && let Some(ctor) = self.gcx.hir.contract(id).ctor
        {
            return Some(CallableParamSource::Function { id: ctor, skips_receiver: false });
        }

        let signature =
            self.gcx.type_of_expr(callee.id).and_then(|ty| self.gcx.callable_signature_of_ty(ty));
        signature.and_then(|signature| signature.param_source)
    }

    fn is_explicit_cast_call(&self, callee: &'gcx hir::Expr<'gcx>) -> bool {
        self.gcx.type_of_expr(callee.id).is_some_and(
            |ty| matches!(ty.kind, TyKind::Type(to) if !matches!(to.kind, TyKind::Struct(_))),
        )
    }

    fn modifier_param_source(
        &self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> Option<CallableParamSource> {
        let id = match modifier.id {
            ItemId::Contract(id) => self.gcx.hir.contract(id).ctor?,
            ItemId::Function(id) => id,
            _ => return None,
        };
        Some(CallableParamSource::Function { id, skips_receiver: false })
    }
}

impl<'gcx> Visit<'gcx> for InlayHintCollector<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let ExprKind::Call(callee, ref args, _) = expr.kind {
            self.push_parameter_hints(args, self.call_param_source(callee));
            self.push_call_type_hint(expr, callee);
        }
        hir::Visit::walk_expr(self, expr)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push_parameter_hints(&modifier.args, self.modifier_param_source(modifier));
        hir::Visit::walk_modifier(self, modifier)
    }
}

fn hint_sort_key(hint: &StoredInlayHint) -> (Position, StoredInlayHintKind, &str) {
    (hint.position, hint.kind, hint.label.as_ref())
}
