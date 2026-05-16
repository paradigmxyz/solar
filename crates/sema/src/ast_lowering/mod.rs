use crate::{
    hir::{self, Hir, IdCounter},
    parse::Sources,
    ty::{Gcx, GcxMut},
};
use solar_ast as ast;
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashMap,
};
use solar_interface::{Session, diagnostics::DiagCtxt};

mod lower;

mod linearize;

pub(crate) mod resolve;
pub(crate) use resolve::{Res, SymbolResolver};

#[instrument(name = "ast_lowering", level = "debug", skip_all)]
pub(crate) fn lower(mut gcx: GcxMut<'_>) {
    let mut lcx = LoweringContext::new(gcx.get());

    // Lower AST to HIR.
    lcx.lower_sources();

    // Resolve source scopes.
    lcx.collect_exports();
    lcx.perform_imports();

    // Resolve contract scopes.
    lcx.collect_contract_declarations();
    lcx.resolve_base_contracts();
    lcx.linearize_contracts();
    lcx.assign_constructors();

    let mut rcx = resolve::ResolveContext::new(lcx);
    // Resolve declarations and top-level symbols, and finish lowering to HIR.
    rcx.resolve_symbols();
    // Resolve constructor base args.
    rcx.resolve_base_args();
    let mut lcx = rcx.lcx;

    // Clean up.
    lcx.shrink_to_fit();

    let gcx = gcx.get_mut();
    (gcx.hir, gcx.symbol_resolver) = lcx.finish();
}

struct LoweringContext<'gcx> {
    sess: &'gcx Session,
    arena: &'gcx hir::Arena,
    hir: Hir<'gcx>,

    sources: &'gcx Sources<'gcx>,
    /// Mapping from Hir ItemId to AST Item. Does not include function parameters or bodies.
    hir_to_ast: FxHashMap<hir::ItemId, &'gcx ast::Item<'gcx>>,

    /// Current source being lowered.
    current_source_id: hir::SourceId,
    /// Current contract being lowered.
    current_contract_id: Option<hir::ContractId>,

    resolver: SymbolResolver<'gcx>,
    next_id: IdCounter,
}

impl<'gcx> LoweringContext<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            sess: gcx.sess,
            arena: gcx.arena(),
            sources: &gcx.sources,
            hir: Hir::new(),
            current_source_id: hir::SourceId::MAX,
            current_contract_id: None,
            hir_to_ast: FxHashMap::default(),
            resolver: SymbolResolver::new(&gcx.sess.dcx),
            next_id: IdCounter::new(),
        }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    #[instrument(level = "debug", skip_all)]
    fn shrink_to_fit(&mut self) {
        self.hir.shrink_to_fit();
    }

    #[instrument(name = "drop_lcx", level = "debug", skip_all)]
    fn finish(self) -> (Hir<'gcx>, SymbolResolver<'gcx>) {
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
