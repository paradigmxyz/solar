//! ABI phase lowering: materialize calldata decode / returndata encode as MIR.
//!
//! In `built`/`optimized` MIR an external function takes typed MIR arguments and
//! returns typed values; the calldata decode and returndata encode happen
//! implicitly in the backend. This pass makes that explicit, moving the ABI
//! boundary into MIR itself (the ABI phase of the sketch in [`MirPhase`]).
//!
//! For each external entry `f(x0: T0, .., xn: Tn) -> (R0, .., Rm)` whose
//! parameters and returns are all static word-sized scalars, it:
//!
//! 1. moves the original body into a fresh internal function `f.body`, and
//! 2. rewrites `f` into a self-decoding wrapper that takes no MIR arguments, keeps its selector,
//!    decodes each argument from `calldataload(4 + 32*i)`, calls `f.body` with the decoded values,
//!    writes the returns to memory, and returns them with `RETURN(offset, 32*m)`.
//!
//! Argument decoding is uniform for every parameter type: the ABI head word at
//! the parameter's fixed calldata offset is passed through, which is the scalar
//! itself for static types and the head offset for dynamic ones — exactly the
//! word the external-form body already expects and further decodes itself.
//! Return encoding is the wrapper's job, so returns must be static words: a
//! tuple of static returns is their words concatenated with no head/tail, and a
//! dynamic return makes the whole pass bail. Internal call sites
//! that targeted a wrapped function are retargeted to its extracted body, so
//! internal calls to public functions keep their semantics.
//!
//! The phase transition is all-or-nothing: if any bodied external function has a
//! dynamic (non-word) parameter or return, the module is left untouched and does
//! not advance, so an `abi`-phase module always means every external function is
//! a wrapper.
//!
//! This is opt-in: it is not part of the default pipeline, and the backend does
//! not consume `abi`-phase modules. It is the staging ground for moving dispatch
//! and ABI handling out of the backend, and composes with [`super::LowerDispatchPass`],
//! which routes a selector switch to these argument-free wrappers.

use crate::{
    mir::{Function, FunctionId, InstKind, MirPhase, MirType, Module},
    pass::ModulePass,
};
use solar_data_structures::map::FxHashMap;
use solar_interface::{Ident, Symbol};

/// Base memory offset for the wrapper's return buffer.
///
/// The wrapper is self-contained and does not share memory with a caller, so
/// any non-scratch offset is valid; `0x80` is the conventional free-memory
/// start.
const RETURN_BUFFER_START: u64 = 0x80;

/// Statistics from ABI wrapper lowering.
#[derive(Clone, Debug, Default)]
pub struct LowerAbiStats {
    /// Number of external functions wrapped.
    pub wrapped: usize,
    /// Number of external functions with a non-word return type. Any non-zero
    /// count makes the whole pass bail: the phase transition is all-or-nothing.
    pub skipped_dynamic: usize,
    /// Number of internal call sites retargeted from a wrapped function to its
    /// extracted body.
    pub retargeted_calls: usize,
}

/// ABI phase lowering pass.
#[derive(Debug, Default)]
pub struct LowerAbiPass {
    stats: LowerAbiStats,
}

impl LowerAbiPass {
    /// Returns statistics for the most recent run.
    #[must_use]
    pub const fn stats(&self) -> &LowerAbiStats {
        &self.stats
    }

    fn run(&mut self, module: &mut Module) -> bool {
        self.stats = LowerAbiStats::default();

        // Idempotent: only `built`/`optimized` modules have an implicit ABI
        // boundary to materialize.
        if module.phase >= MirPhase::Abi {
            return false;
        }

        // Snapshot the ids to wrap first; wrapping appends new functions, and we
        // must not revisit them.
        let targets: Vec<FunctionId> = module
            .functions
            .iter_enumerated()
            .filter(|(_, func)| is_wrappable_external(func))
            .map(|(id, _)| id)
            .collect();

        // All-or-nothing: `abi` means *every* bodied external function is a
        // wrapper. If any signature is outside the static-word scope, leave the
        // module untouched instead of advancing to a phase the content does not
        // satisfy.
        self.stats.skipped_dynamic =
            targets.iter().filter(|&&id| function_words(module.function(id)).is_none()).count();
        if self.stats.skipped_dynamic != 0 || targets.is_empty() {
            return false;
        }

        let mut body_of_wrapper = FxHashMap::default();
        for id in targets {
            let body_id = self.wrap_function(module, id);
            body_of_wrapper.insert(id, body_id);
            self.stats.wrapped += 1;
        }

        // Internal calls to a wrapped public/external function must keep the
        // original call semantics: retarget them to the extracted body. The
        // wrappers' own calls already target the bodies and are not affected.
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

        module.advance_phase(MirPhase::Abi);
        true
    }

    /// Splits one external function into an internal body plus a self-decoding
    /// wrapper that reuses the original function slot, returning the id of the
    /// extracted body.
    fn wrap_function(&mut self, module: &mut Module, wrapper_id: FunctionId) -> FunctionId {
        let (param_tys, return_tys) =
            function_words(module.function(wrapper_id)).expect("caller checked word-sized");

        // Move the original function (body, params, returns) into a new internal
        // function, leaving a fresh wrapper shell in its place.
        let wrapper_name = module.function(wrapper_id).name;
        let selector = module.function(wrapper_id).selector;
        let body_name = Ident::with_dummy_span(Symbol::intern(&format!("{wrapper_name}.body")));

        let mut body =
            std::mem::replace(module.function_mut(wrapper_id), Function::new(wrapper_name));
        body.name = body_name;
        body.selector = None;
        body.attributes.visibility = solar_sema::hir::Visibility::Internal;
        let body_id = module.add_function(body);

        // Rebuild the wrapper shell: same name and selector, no MIR arguments.
        let wrapper = module.function_mut(wrapper_id);
        wrapper.selector = selector;

        let mut builder = crate::mir::FunctionBuilder::new(wrapper);

        // Decode each static argument: one word at `4 + 32*i`.
        let mut args = Vec::with_capacity(param_tys.len());
        for i in 0..param_tys.len() {
            let offset = builder.imm_u64(4 + 32 * i as u64);
            args.push(builder.calldataload(offset));
        }

        if return_tys.is_empty() {
            builder.internal_call_void(body_id, args, 0);
            let zero = builder.imm_u64(0);
            builder.ret_data(zero, zero);
            return body_id;
        }

        // Call the original body. The first return value is the call result; any
        // remaining return words follow the existing multi-return call convention
        // and are materialized in scratch memory at `32`, `64`, ...
        let call = builder.internal_call(body_id, args, return_tys[0], return_tys.len());

        // Encode the static returns: word `i` at `RETURN_BUFFER_START + 32*i`.
        let base = builder.imm_u64(RETURN_BUFFER_START);
        builder.mstore(base, call);
        for i in 1..return_tys.len() {
            let src = builder.imm_u64((i * 32) as u64);
            let word = builder.mload(src);
            let dst = builder.imm_u64(RETURN_BUFFER_START + (i * 32) as u64);
            builder.mstore(dst, word);
        }
        let size = builder.imm_u64(32 * return_tys.len() as u64);
        builder.ret_data(base, size);
        body_id
    }
}

impl ModulePass for LowerAbiPass {
    fn name(&self) -> &str {
        "lower-abi"
    }

    fn run(&mut self, module: &mut Module) -> bool {
        Self::run(self, module)
    }
}

/// An external entry with a body and a selector — the shape a wrapper is built
/// for. Receive/fallback entries have no selector and are left to the backend.
fn is_wrappable_external(func: &Function) -> bool {
    func.selector.is_some() && !func.attributes.is_constructor && !func.blocks.is_empty()
}

/// Returns the parameter and return types if the function is wrappable.
///
/// Parameters of any type are decoded by passing the raw calldata word at
/// `4 + 32*i` through: the external-form body interprets its argument as that
/// word — the scalar itself for static types, the ABI head offset for dynamic
/// ones — and performs any further decoding itself. Returns must be static
/// words, since the wrapper encodes them; a dynamic return makes the whole
/// pass bail.
fn function_words(func: &Function) -> Option<(Vec<MirType>, Vec<MirType>)> {
    if !func.returns.iter().all(is_static_word) {
        return None;
    }
    Some((func.params.clone(), func.returns.clone()))
}

/// Whether a type occupies exactly one ABI word with no head/tail, so it decodes
/// from a single `calldataload` and encodes with a single `mstore`.
fn is_static_word(ty: &MirType) -> bool {
    matches!(
        ty,
        MirType::UInt(_)
            | MirType::Int(_)
            | MirType::Bool
            | MirType::Address
            | MirType::FixedBytes(_)
    )
}
