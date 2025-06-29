use crate::{
    hir::{self, Hir},
    ParsedSources,
};
use solar_ast as ast;
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashMap,
    trustme,
};
use solar_interface::{diagnostics::DiagCtxt, Session};
use std::sync::atomic::AtomicUsize;

mod lower;

mod linearize;

pub(crate) mod resolve;
pub(crate) use resolve::{Res, SymbolResolver};

#[instrument(name = "ast_lowering", level = "debug", skip_all)]
pub(crate) fn lower<'sess, 'hir>(
    sess: &'sess Session,
    sources: &ParsedSources<'_>,
    hir_arena: &'hir hir::Arena,
) -> (Hir<'hir>, SymbolResolver<'sess>) {
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
    lcx.assign_constructors();

    let next_id = &AtomicUsize::new(0);
    // Resolve declarations and top-level symbols, and finish lowering to HIR.
    lcx.resolve_symbols(next_id);
    // Resolve constructor base args.
    lcx.resolve_base_args(next_id);

    // Clean up.
    lcx.shrink_to_fit();

    lcx.finish()
}

struct LoweringContext<'sess, 'ast, 'hir> {
    sess: &'sess Session,
    arena: &'hir hir::Arena,
    hir: Hir<'hir>,
    /// Mapping from Hir ItemId to AST Item. Does not include function parameters or bodies.
    hir_to_ast: FxHashMap<hir::ItemId, &'ast ast::Item<'ast>>,

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
            hir_to_ast: FxHashMap::default(),
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
    fn finish(self) -> (Hir<'hir>, SymbolResolver<'sess>) {
        // NOTE: Explicit scope to drop `self` before the span.
        {
            let this = self;
            (this.hir, this.resolver)
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
    sl.get_disjoint_mut([idx_1, idx_2]).unwrap().into()
}
