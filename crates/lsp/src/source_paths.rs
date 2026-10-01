//! Recovers source path segments shared by navigation and rename indexing.
//!
//! HIR sometimes keeps only a path's terminal resolution and whole source span. Lexing that
//! span recovers exact identifier ranges without mistaking comments for segments. Qualifiers
//! are resolved in source order, retaining the source that owns each import binding. Callers
//! keep the terminal target selected by type checking, including overload resolution.

use solar_interface::{Ident, Span};
use solar_parse::Lexer;
use solar_sema::{
    Gcx,
    hir::{self, Res},
};

pub(crate) struct SourcePath {
    pub(crate) final_ident: Ident,
    pub(crate) final_source: hir::SourceId,
    pub(crate) qualifiers: Vec<PathQualifier>,
}

pub(crate) struct PathQualifier {
    pub(crate) ident: Ident,
    pub(crate) source: hir::SourceId,
    pub(crate) resolutions: Vec<Res>,
}

impl SourcePath {
    pub(crate) fn resolve(
        gcx: Gcx<'_>,
        span: Span,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
    ) -> Option<Self> {
        let identifiers = identifiers_in_span(gcx, span);
        let (&final_ident, qualifiers) = identifiers.split_last()?;
        let mut path = Self { final_ident, final_source: source, qualifiers: Vec::new() };
        if !qualifiers.is_empty()
            && let Some(resolutions) = gcx.source_path_resolutions(&identifiers, source, contract)
        {
            for (&ident, resolutions) in qualifiers.iter().zip(resolutions) {
                let source = path.final_source;
                if let [Res::Namespace(namespace)] = resolutions.as_slice() {
                    path.final_source = *namespace;
                }
                path.qualifiers.push(PathQualifier { ident, source, resolutions });
            }
        }
        Some(path)
    }
}

pub(crate) fn identifiers_in_span(gcx: Gcx<'_>, span: Span) -> Vec<Ident> {
    let Ok(source) = gcx.sess.source_map().span_to_snippet(span) else { return Vec::new() };
    Lexer::with_start_pos(gcx.sess, &source, span.lo()).filter_map(|token| token.ident()).collect()
}
