//! Which functions `llm-optimize` offers to its rewriter.
//!
//! A function is offered when the interpreter can run everything a call to it can reach, so that
//! candidates can be tested against it, and when its interface is a few words, so that the tests
//! can generate its inputs:
//!
//! - It is an internal function reachable from an entry, and not the dispatch entry, an external
//!   entry, an ABI wrapper, or a function-pointer dispatcher, whose bodies implement protocols the
//!   interpreter does not model.
//! - It is not recursive, takes explicit word parameters (`iN` or `memptr`), and returns at most
//!   one word: the backend passes further results through a memory buffer of its own.
//! - It neither runs in the constructor nor can call a recursive function. The backend keeps the
//!   frames of such calls on the heap, where a call writes memory above the free memory pointer
//!   and, when the callee takes or returns memory, leaves the pointer raised past its frame; the
//!   tests model neither.
//! - It has at most [`MAX_INSTRUCTIONS`] instructions.
//! - It and every function it can reach use only operations the tests run ([`equivalence::runs`])
//!   and terminators [`interp::supports_terminator`] accepts, no `undef` or error values, and
//!   single results. The tests run storage, logs, and most context reads, but not calls to other
//!   contracts, `gas`, or reads of code.
//!
//! A module that reads `msize` offers nothing: a candidate may touch memory its original does not,
//! which `msize` anywhere else would observe.

use super::equivalence;
use crate::mir::{
    Function, FunctionId, InstKind, MirType, Module, Terminator, Value, analysis::CallGraphInfo,
    transform::lower_evm_shaped::constructor_reachable, utils::interp,
};

/// The most instructions an offered function may have, keeping prompts and tests small.
pub(super) const MAX_INSTRUCTIONS: usize = 256;

/// Returns why `module` offers no function, if one reason covers them all.
pub(super) fn module_exclusion(module: &Module) -> Option<&'static str> {
    let reads_msize = module.functions.iter().any(|function| {
        function.instructions().any(|inst| matches!(function.inst(inst).kind, InstKind::MSize))
    });
    reads_msize.then_some("the module reads `msize`")
}

/// Returns why function `id` is not offered, or `None` when it is.
pub(super) fn exclusion(module: &Module, graph: &CallGraphInfo, id: FunctionId) -> Option<String> {
    let function = module.function(id);
    if module.dispatch_entry() == Some(id) {
        return Some("is the dispatch entry".into());
    }
    if function.is_external_entry() || function.attributes.is_abi_wrapper {
        return Some("is an external entry".into());
    }
    if function.attributes.is_function_pointer_dispatcher {
        return Some("is a function-pointer dispatcher".into());
    }
    if !graph.reachable_from_entries().contains(id) {
        return Some("is unreachable".into());
    }
    if graph.is_recursive(id) {
        return Some("is recursive".into());
    }
    // The backend keeps the frames of recursive functions, and of every call the constructor
    // makes, on the heap: a call there writes memory above the free memory pointer, and leaves
    // the pointer raised past the frame when the callee takes or returns memory. The tests model
    // neither, so a candidate that drops a call, or changes its own frame, would pass unseen.
    if constructor_reachable(module, graph).contains(id) {
        return Some("runs in the constructor, which keeps call frames on the heap".into());
    }
    if let Some(callee) =
        graph.reachable_callees_from([id]).iter().find(|&callee| graph.is_recursive(callee))
    {
        let name = module.function(callee).name;
        return Some(format!(
            "calls `@{name}`, which is recursive and keeps its frame on the heap"
        ));
    }
    if function.instructions().count() > MAX_INSTRUCTIONS {
        return Some(format!("has more than {MAX_INSTRUCTIONS} instructions"));
    }
    if let Some(ty) = function.params.iter().find(|&&ty| !is_word(ty)) {
        return Some(format!("takes a `{ty}`"));
    }
    if let [ty] = function.return_components()
        && !is_word(*ty)
    {
        return Some(format!("returns a `{ty}`"));
    }
    if let Some(reason) = body_exclusion(function) {
        return Some(reason);
    }
    graph.reachable_callees_from([id]).iter().find_map(|callee| {
        let callee = module.function(callee);
        body_exclusion(callee).map(|reason| format!("calls `@{}`, which {reason}", callee.name))
    })
}

/// Returns why the interpreter cannot run `function`, or `None` when it can.
fn body_exclusion(function: &Function) -> Option<String> {
    if function.arg_indices().count() != function.params.len() {
        return Some("reads implicit arguments".into());
    }
    if function.return_components().len() > 1 {
        return Some("returns several values".into());
    }
    for inst in function.instructions() {
        let kind = &function.inst(inst).kind;
        if !equivalence::runs(kind) {
            return Some(format!("uses `{}`", kind.op_def().mnemonic));
        }
    }
    for value in function.live_values() {
        match function.value(value) {
            Value::Undef(_) => return Some("uses `undef`".into()),
            Value::Error(_) => return Some("uses an error value".into()),
            _ => {}
        }
    }
    for block in &function.blocks {
        match &block.terminator {
            Some(terminator) if interp::supports_terminator(terminator) => {}
            Some(Terminator::SelfDestruct { .. }) => return Some("uses `selfdestruct`".into()),
            Some(_) => return Some("uses `revert_returndata`".into()),
            None => return Some("has a block without a terminator".into()),
        }
    }
    None
}

/// Returns whether values of `ty` are single words the tests can generate.
pub(super) fn is_word(ty: MirType) -> bool {
    matches!(ty, MirType::Int(_) | MirType::MemPtr)
}
