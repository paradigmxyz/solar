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

    /// Rewrites one external function into a self-decoding form, keeping a
    /// pristine copy for internal callers.
    ///
    /// The original function keeps its selector and loses its MIR parameters:
    /// each `Value::Arg` entry in its value table is redefined as a fresh
    /// `calldataload(4 + 32*i)` instruction prepended to the entry block, so
    /// every use follows automatically and the function decodes itself exactly
    /// the way the backend used to materialize its arguments. Its fused encode
    /// and `RETURN` stay intact, and no internal call is introduced on the
    /// external path. A pristine `.body` copy with parameters preserved is
    /// appended for internal callers, which are retargeted to it.
    fn wrap_function(&mut self, module: &mut Module, wrapper_id: FunctionId) -> FunctionId {
        // Pristine copy for internal callers.
        let mut body = module.function(wrapper_id).clone();
        body.name = Ident::with_dummy_span(Symbol::intern(&format!("{}.body", body.name)));
        body.selector = None;
        body.attributes.visibility = solar_sema::hir::Visibility::Internal;
        let body_id = module.add_function(body);

        // Absorb the arguments into the external original.
        let func = module.function_mut(wrapper_id);
        let params = std::mem::take(&mut func.params);
        let mut loads = Vec::with_capacity(params.len());
        for index in 0..params.len() {
            let offset = func.alloc_value(crate::mir::Value::Immediate(
                crate::mir::Immediate::uint256(alloy_primitives::U256::from(4 + 32 * index as u64)),
            ));
            let inst = func.alloc_inst(crate::mir::Instruction::new(
                InstKind::CalldataLoad(offset),
                Some(MirType::uint256()),
            ));
            loads.push(inst);
        }
        let entry = func.entry_block;
        for (position, inst) in loads.iter().enumerate() {
            func.blocks[entry].instructions.insert(position, *inst);
        }
        // Redefine each argument value as its load: every use follows.
        let arg_values: Vec<_> = func
            .values
            .iter_enumerated()
            .filter_map(|(vid, value)| match value {
                crate::mir::Value::Arg { index, .. } => Some((vid, *index as usize)),
                _ => None,
            })
            .collect();
        for (vid, index) in arg_values {
            func.values[vid] = crate::mir::Value::Inst(loads[index]);
        }
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

/// Returns the parameter and return types if the function can be absorbed.
///
/// Absorption relies on the fused encode: the body must produce its own
/// returndata, so a function that still returns MIR values (which would need
/// the caller to encode them) makes the whole pass bail. Parameters of any
/// type and any count are fine: each becomes a `calldataload` of its head
/// word, exactly the lazy load the backend used to materialize.
fn function_words(func: &Function) -> Option<(Vec<MirType>, Vec<MirType>)> {
    if !func.returns.is_empty() {
        return None;
    }
    Some((func.params.clone(), func.returns.clone()))
}
