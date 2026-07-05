use crate::proto;
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range, Url};
use solar_interface::data_structures::{Never, map::FxHashMap};
use solar_sema::{
    Gcx,
    hir::{self, CallArgsKind, ExprKind, ItemId, Res, Visit},
    ty::{CallableSignature, Ty, TyKind},
};
use std::ops::ControlFlow;

#[derive(Clone, Debug, Default)]
pub(crate) struct InlayHintIndex {
    by_file: FxHashMap<Url, Vec<StoredInlayHint>>,
}

#[derive(Clone, Debug)]
struct StoredInlayHint {
    position: Position,
    label: String,
    kind: StoredInlayHintKind,
    padding_left: bool,
    padding_right: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        for (uri, mut hints) in other.by_file {
            self.by_file.entry(uri).or_default().append(&mut hints);
        }
        self.sort();
    }

    pub(crate) fn hints(&self, uri: &Url, range: Range) -> Vec<InlayHint> {
        let Some(hints) = self.by_file.get(uri) else {
            return Vec::new();
        };
        let start = hints.partition_point(|hint| hint.position < range.start);
        hints[start..]
            .iter()
            .take_while(|hint| position_before_range_end(hint.position, range))
            .filter(|hint| range_contains_position(range, hint.position))
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
    fn parameter(position: Position, label: String) -> Self {
        Self {
            position,
            label,
            kind: StoredInlayHintKind::Parameter,
            padding_left: false,
            padding_right: true,
        }
    }

    fn call_type(position: Position, label: String) -> Self {
        Self {
            position,
            label,
            kind: StoredInlayHintKind::CallType,
            padding_left: true,
            padding_right: false,
        }
    }

    fn to_lsp(&self) -> InlayHint {
        InlayHint {
            position: self.position,
            label: InlayHintLabel::String(self.label.clone()),
            kind: Some(self.kind.lsp_kind()),
            text_edits: None,
            tooltip: None,
            padding_left: Some(self.padding_left),
            padding_right: Some(self.padding_right),
            data: None,
        }
    }
}

impl StoredInlayHintKind {
    fn lsp_kind(self) -> InlayHintKind {
        match self {
            Self::Parameter => InlayHintKind::PARAMETER,
            Self::CallType => InlayHintKind::TYPE,
        }
    }

    fn sort_key(self) -> u8 {
        match self {
            Self::Parameter => 0,
            Self::CallType => 1,
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
        signature: CallableSignature<'gcx>,
    ) {
        let CallArgsKind::Unnamed(exprs) = args.kind else {
            return;
        };
        let Some(param_source) = signature.param_source else {
            return;
        };
        let param_names = self.gcx.callable_param_names(param_source);
        for ((arg, _param_ty), param_name) in
            exprs.iter().zip(signature.parameters).zip(param_names)
        {
            let Some(param_name) = param_name else {
                continue;
            };
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

    fn push_call_type_hint(&mut self, expr: &'gcx hir::Expr<'gcx>, callee: &'gcx hir::Expr<'gcx>) {
        if self.is_explicit_cast_call(callee) {
            return;
        }
        let Some(ty) = self.gcx.type_of_expr(expr.id) else {
            return;
        };
        if !show_call_type_hint(ty) {
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

    fn call_signature(&self, callee: &'gcx hir::Expr<'gcx>) -> Option<CallableSignature<'gcx>> {
        if let Some(callee) = self.gcx.resolved_callee(callee.id) {
            return self.gcx.callable_signature_of_res(callee.res, callee.attached);
        }
        self.gcx.type_of_expr(callee.id).and_then(|ty| self.gcx.callable_signature_of_ty(ty))
    }

    fn is_explicit_cast_call(&self, callee: &'gcx hir::Expr<'gcx>) -> bool {
        if self.gcx.resolved_callee(callee.id).is_some() {
            return false;
        }
        let Some(ty) = self.gcx.type_of_expr(callee.id) else {
            return false;
        };
        matches!(ty.kind, TyKind::Type(to) if !matches!(to.kind, TyKind::Struct(_)))
    }

    fn modifier_signature(
        &self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> Option<CallableSignature<'gcx>> {
        let res = match modifier.id {
            ItemId::Contract(id) => self.gcx.hir.contract(id).ctor.map(ItemId::Function)?,
            id => id,
        };
        self.gcx.callable_signature_of_res(Res::Item(res), false)
    }
}

impl<'gcx> Visit<'gcx> for InlayHintCollector<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            ExprKind::Call(callee, ref args, opts) => {
                self.visit_expr(callee)?;
                if let Some(opts) = opts {
                    for arg in opts.args {
                        self.visit_expr(&arg.value)?;
                    }
                }
                if let Some(signature) = self.call_signature(callee) {
                    self.push_parameter_hints(args, signature);
                }
                self.push_call_type_hint(expr, callee);
                self.visit_call_args(args)?;
            }
            _ => {
                hir::Visit::walk_expr(self, expr)?;
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let Some(signature) = self.modifier_signature(modifier) {
            self.push_parameter_hints(&modifier.args, signature);
        }
        self.visit_call_args(&modifier.args)
    }
}

fn show_call_type_hint(ty: Ty<'_>) -> bool {
    !ty.is_unit() && !ty.references_error()
}

fn hint_sort_key(hint: &StoredInlayHint) -> (u32, u32, u8, &str) {
    (hint.position.line, hint.position.character, hint.kind.sort_key(), hint.label.as_str())
}

fn range_contains_position(range: Range, position: Position) -> bool {
    if range.start == range.end {
        return position == range.start;
    }
    position >= range.start && position < range.end
}

fn position_before_range_end(position: Position, range: Range) -> bool {
    if range.start == range.end {
        return position <= range.end;
    }
    position < range.end
}
