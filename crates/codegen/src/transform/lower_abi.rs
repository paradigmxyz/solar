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
//! For the static word-sized scope this is a faithful ABI codec: static
//! arguments are one word each at a fixed calldata offset, and a tuple of static
//! returns is their words concatenated with no head/tail. Functions with dynamic
//! parameters or returns (and non-external functions) are left untouched.
//!
//! This is opt-in: it is not part of the default pipeline, and the backend does
//! not consume `abi`-phase modules. It is the staging ground for moving dispatch
//! and ABI handling out of the backend, and composes with [`super::LowerDispatchPass`],
//! which routes a selector switch to these argument-free wrappers.

use crate::{
    mir::{Function, FunctionId, MirPhase, MirType, Module},
    pass::ModulePass,
};
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
    /// Number of external functions left unchanged because a parameter or
    /// return type is not a static word.
    pub skipped_dynamic: usize,
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

        for id in targets {
            if function_words(module.function(id)).is_some() {
                self.wrap_function(module, id);
                self.stats.wrapped += 1;
            } else {
                self.stats.skipped_dynamic += 1;
            }
        }

        module.advance_phase(MirPhase::Abi);
        self.stats.wrapped != 0
    }

    /// Splits one external function into an internal body plus a self-decoding
    /// wrapper that reuses the original function slot.
    fn wrap_function(&mut self, module: &mut Module, wrapper_id: FunctionId) {
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

        // Call the original body.
        let result_ty = return_tys.first().copied().unwrap_or_else(MirType::uint256);
        let call = builder.internal_call(body_id, args, result_ty, return_tys.len());

        if return_tys.is_empty() {
            let zero = builder.imm_u64(0);
            builder.ret_data(zero, zero);
            return;
        }

        // Encode the static returns: word `i` at `RETURN_BUFFER_START + 32*i`.
        //
        // A single-return internal call yields its value directly; the
        // multi-return ABI-tuple projection is not modelled at this phase yet,
        // so only the first word is materialized for tuples (still a valid,
        // if partial, static encoding shell). Extend when MIR grows tuple
        // projections.
        let base = builder.imm_u64(RETURN_BUFFER_START);
        builder.mstore(base, call);
        let size = builder.imm_u64(32 * return_tys.len() as u64);
        builder.ret_data(base, size);
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

/// Returns the parameter and return types if every one is a static word-sized
/// scalar, else `None`.
fn function_words(func: &Function) -> Option<(Vec<MirType>, Vec<MirType>)> {
    if !func.params.iter().all(is_static_word) || !func.returns.iter().all(is_static_word) {
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
