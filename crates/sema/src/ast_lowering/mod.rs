use crate::{
    hir::{self, Hir},
    ParsedSources,
};
use solar_ast::ast;
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxIndexMap,
    trustme,
};
use solar_interface::{diagnostics::DiagCtxt, Session};

mod lower;

mod linearize;

mod resolve;
use resolve::{DeclarationKind, SymbolResolver};

// TODO: Use another arena for temporary allocations, like resolver scopes.

#[instrument(name = "ast_lowering", level = "debug", skip_all)]
pub(crate) fn lower<'hir>(
    sess: &Session,
    sources: &ParsedSources<'_>,
    hir_arena: &'hir hir::Arena,
) -> Hir<'hir> {
    let mut lcx = LoweringContext::new(sess, hir_arena);

    // Lower AST to HIR.
    // SAFETY: `sources` outlives `lcx`, which does not outlive this function.
    let sources = unsafe { trustme::decouple_lt(sources) };
    lcx.lower_sources(sources);

    // Resolve source scopes.
    lcx.collect_exports();
    lcx.perform_imports(sources);

    // Resolve contract scopes.
    lcx.collect_contract_declarations();
    lcx.resolve_base_contracts();
    lcx.linearize_contracts();

    lcx.resolve();

    // Clean up.
    lcx.shrink_to_fit();

    lcx.finish()
}

struct LoweringContext<'sess, 'ast, 'hir> {
    sess: &'sess Session,
    arena: &'hir hir::Arena,
    hir: Hir<'hir>,
    /// Mapping from Hir ItemId to AST Item. Does not include function parameters or bodies.
    hir_to_ast: FxIndexMap<hir::ItemId, &'ast ast::Item<'ast>>,

    /// Current source being lowered.
    current_source_id: hir::SourceId,
    /// Current contract being lowered.
    current_contract_id: Option<hir::ContractId>,

    resolver: SymbolResolver<'sess>,
}

impl<'sess, 'hir> LoweringContext<'sess, '_, 'hir> {
    fn new(sess: &'sess Session, arena: &'hir hir::Arena) -> Self {
        Self {
            sess,
            arena,
            hir: Hir::new(),
            current_source_id: hir::SourceId::MAX,
            current_contract_id: None,
            hir_to_ast: FxIndexMap::default(),
            resolver: SymbolResolver::new(&sess.dcx),
        }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    #[instrument(level = "debug", skip_all)]
    fn shrink_to_fit(&mut self) {
        self.hir.shrink_to_fit();
    }

    #[instrument(name = "drop_lcx", level = "debug", skip_all)]
    fn finish(self) -> Hir<'hir> {
        // NOTE: Explicit scope to drop `self` before the span.
        {
            let this = self;
            this.hir
        }
    }
}

#[inline]
#[track_caller]
fn get_two_mut_idx<I: Idx, T>(sl: &mut IndexVec<I, T>, idx_1: I, idx_2: I) -> (&mut T, &mut T) {
    get_two_mut(&mut sl.raw, idx_1.index(), idx_2.index())
}

#[inline]
#[track_caller]
fn get_two_mut<T>(sl: &mut [T], idx_1: usize, idx_2: usize) -> (&mut T, &mut T) {
    // TODO: `sl.get_many_mut([idx_1, idx_2])` once stable.

    assert!(idx_1 != idx_2 && idx_1 < sl.len() && idx_2 < sl.len());
    let ptr = sl.as_mut_ptr();
    unsafe { (&mut *ptr.add(idx_1), &mut *ptr.add(idx_2)) }
}
