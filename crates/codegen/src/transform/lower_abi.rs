//! ABI phase lowering: materialize calldata decode / returndata encode as MIR.
//!
//! In `built`/`optimized` MIR an external function takes typed MIR arguments and
//! returns typed values; the calldata decode and returndata encode happen
//! implicitly in the backend. This pass makes that explicit, moving the ABI
//! boundary into MIR itself (the ABI phase of the sketch in [`MirPhase`]).
//!
//! For each external entry `f(x0: T0, .., xn: Tn)` whose body hands no MIR
//! return values back to a caller, it:
//!
//! 1. copies the original into a fresh internal function `f.body` with its parameter list preserved
//!    when there are internal callers, and
//! 2. strips `f`'s MIR parameter list, keeping its selector and its `Value::Arg` entries: in a
//!    selector-bearing wrapper, `argN` denotes the ABI head word at calldata offset `4 + 32*N`,
//!    which the backend rematerializes per use as a `calldataload`; the body keeps its fused
//!    external termination (`RETURN`/`REVERT`/`STOP`).
//!
//! The head-word convention is uniform for every parameter type: the word at
//! the parameter's fixed calldata offset is the scalar itself for static
//! types and the head offset for dynamic ones — exactly the word the
//! external-form body already expects and further decodes itself. Returns
//! are different: the wrappers do not implement returndata encoding at all,
//! so they rely on the external lowering having fused the encode into the
//! body (every value-carrying `ret` already rewritten to `returndata`). A
//! function whose body still carries a live value-`Return` terminator makes
//! the whole pass bail. Internal call sites that targeted a wrapped function
//! are retargeted to its extracted body, so internal calls to public
//! functions keep their semantics.
//!
//! The phase transition is all-or-nothing: if any bodied external function
//! still returns MIR values, the module is left untouched and does not
//! advance, so an `abi`-phase module always means every external function is
//! a wrapper.
//!
//! Together with [`super::LowerDispatchPass`], which routes a selector switch
//! to these argument-free wrappers, this moves dispatch and ABI handling out of
//! the backend. Both passes run by default in the codegen pipeline (opt out
//! with `-Zno-mir-dispatch`); a module where this pass bails keeps its phase
//! and is dispatched by the backend instead.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, FunctionId, InstKind, MirPhase, Module, Terminator},
    pass::ModulePass,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_interface::{Ident, Symbol};

/// Statistics from ABI wrapper lowering.
#[derive(Clone, Debug, Default)]
pub(crate) struct LowerAbiStats {
    /// Number of external functions wrapped.
    pub wrapped: usize,
    /// Number of external functions with returns, which the wrappers cannot
    /// encode yet. Any non-zero count makes the whole pass bail: the phase
    /// transition is all-or-nothing.
    pub skipped_dynamic: usize,
    /// Number of internal call sites retargeted from a wrapped function to its
    /// extracted body.
    pub retargeted_calls: usize,
    /// Number of wrappers that received a prologue callvalue check because
    /// the dispatch entry cannot hoist one.
    pub injected_checks: usize,
}

/// ABI phase lowering pass.
#[derive(Debug, Default)]
pub(crate) struct LowerAbiPass {
    stats: LowerAbiStats,
}

impl LowerAbiPass {
    fn run(&mut self, module: &mut Module) -> bool {
        self.stats = LowerAbiStats::default();

        // Idempotent: only `built`/`optimized` modules have an implicit ABI
        // boundary to materialize.
        if module.phase >= MirPhase::Abi {
            return false;
        }

        // Snapshot the ids to wrap first; wrapping appends new functions, and we
        // must not revisit them.
        let mut targets = Vec::new();
        let mut internally_called = DenseBitSet::new_empty(module.functions.len());
        let mut callvalue = super::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if is_wrappable_external(func) {
                targets.push(id);
                self.stats.skipped_dynamic += usize::from(has_live_value_return(func));
            }
            for function in func.instructions.iter().filter_map(|inst| {
                if let InstKind::InternalCall { function, .. } = &inst.kind {
                    Some(*function)
                } else {
                    None
                }
            }) {
                internally_called.insert(function);
            }
        }

        // All-or-nothing: `abi` means *every* bodied external function is a
        // wrapper. If any signature is outside the static-word scope, leave the
        // module untouched instead of advancing to a phase the content does not
        // satisfy.
        if self.stats.skipped_dynamic != 0 || targets.is_empty() {
            return false;
        }

        // Most external functions are never called internally. Only those
        // that are need a second, parameterized body; cloning every wrapper
        // needlessly grows the MIR consumed by all subsequent lowering and
        // backend passes.
        // When the dispatch entry cannot hoist a single callvalue check,
        // each rejecting wrapper carries its own, exactly like the backend
        // dispatcher's per-wrapper payable check: the check belongs to the
        // wrapper's prologue (falling through into the body) rather than to a
        // guard block in the selector switch, which would pay an extra jump
        // per case. `lower-dispatch` shares the predicate and routes selector
        // cases unguarded.
        let hoist_callvalue = callvalue.hoists();

        let mut body_of_wrapper = FxHashMap::default();
        for id in targets {
            if let Some(body_id) = self.wrap_function(module, id, internally_called.contains(id)) {
                body_of_wrapper.insert(id, body_id);
            }
            self.stats.wrapped += 1;
            if !hoist_callvalue && super::rejects_callvalue(module.function(id)) {
                Self::inject_callvalue_check(module.function_mut(id));
                self.stats.injected_checks += 1;
            }
        }

        // Internal calls to a wrapped public/external function must keep the
        // original call semantics: retarget them to the extracted body. The
        // wrappers' own calls already target the bodies and are not affected.
        if !body_of_wrapper.is_empty() {
            for func in module.functions.iter_mut() {
                for inst in func.instructions.iter_mut() {
                    if let InstKind::InternalCall { function, .. } = &mut inst.kind
                        && let Some(&body_id) = body_of_wrapper.get(function)
                    {
                        *function = body_id;
                        self.stats.retargeted_calls += 1;
                    }
                }
            }
        }

        module.advance_phase(MirPhase::Abi);
        true
    }

    /// Rewrites one external function into a self-decoding form, keeping a
    /// pristine copy for internal callers.
    ///
    /// The original function keeps its selector and loses its MIR parameter
    /// list, but its `Value::Arg` entries stay in place: in a selector-bearing
    /// wrapper, `argN` denotes the ABI head word at calldata offset
    /// `4 + 32*N`, and the backend rematerializes each use as a
    /// `calldataload` of that offset — the same lazy per-use materialization
    /// the backend dispatcher path uses, so wrapper arguments never spill.
    /// Materializing the loads as eager MIR instructions instead was measured
    /// to cost real bytes: an instruction result is not rematerializable, so
    /// every multi-use or cross-block argument bought spill traffic the
    /// `Arg` form avoids. The explicit-decode representation returns when
    /// dynamic types force real decoding (slices); head words do not need it.
    /// The fused encode and `RETURN` stay intact, and no internal call is
    /// introduced on the external path. When the function has internal
    /// callers, a pristine `.body` copy with parameters preserved is appended
    /// and those callers are retargeted to it.
    fn wrap_function(
        &mut self,
        module: &mut Module,
        wrapper_id: FunctionId,
        needs_body: bool,
    ) -> Option<FunctionId> {
        // The copy must precede wrapper mutation and callvalue injection so
        // internal callers keep the original function semantics.
        let body_id = needs_body.then(|| {
            let mut body = module.function(wrapper_id).clone();
            body.name = Ident::with_dummy_span(Symbol::intern(&format!("{}.body", body.name)));
            body.selector = None;
            body.attributes.visibility = solar_sema::hir::Visibility::Internal;
            module.add_function(body)
        });

        // The wrapper takes no MIR arguments; its `Arg` values now read the
        // calldata head words directly.
        module.function_mut(wrapper_id).params.clear();
        body_id
    }

    /// Prepends `if callvalue() != 0 { revert(0, 0) }` to a wrapper.
    ///
    /// The new guard block becomes the entry and falls through into the old
    /// body, so the check costs no extra jump — the backend dispatcher's
    /// per-wrapper payable-check shape. Injected after the `.body` copy is
    /// taken: internal callers never pay the check.
    fn inject_callvalue_check(func: &mut Function) {
        let old_entry = BlockId::ENTRY;
        let mut builder = FunctionBuilder::new(func);
        let guard = builder.create_block();
        let revert = builder.create_block();
        builder.switch_to_block(guard);
        let value = builder.callvalue();
        builder.branch(value, revert, old_entry);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);

        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        crate::mir::utils::remap_block_order(func, &order);
    }
}

impl ModulePass for LowerAbiPass {
    fn run(&mut self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        Self::run(self, module)
    }
}

/// An external entry with a body and a selector — the shape a wrapper is built
/// for. Receive/fallback entries have no selector and are left to the backend.
fn is_wrappable_external(func: &Function) -> bool {
    func.selector.is_some() && !func.attributes.is_constructor
}

/// Absorption relies on the fused encode: the body must produce its own
/// returndata, so a function that would hand MIR return values back to a
/// caller (which would then need to encode them) makes the whole pass bail.
/// The signature's `returns` list is not the test: external lowering fuses
/// the encode and rewrites every value-carrying `ret` into `returndata`,
/// leaving the signature stale. What matters is whether any value-carrying
/// `Return` terminator is still live in the body. Parameters of any type and
/// any count are fine: each stays an `Arg` head word the backend
/// rematerializes lazily.
fn has_live_value_return(func: &Function) -> bool {
    func.blocks.iter().any(|block| {
        matches!(&block.terminator, Some(Terminator::Return { values }) if !values.is_empty())
    })
}
