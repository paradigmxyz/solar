//! The backend protocol.
//!
//! MIR is the target-agnostic middle-end boundary; a backend consumes a
//! [`Module`] and lowers it to a target artifact. Other backends implement
//! [`Backend`] to plug in; [`EvmCodegen`](crate::EvmCodegen) is the reference.

use crate::mir::Module;

pub mod evm;

/// A code generation backend that lowers MIR to a target artifact.
pub trait Backend {
    /// The artifact this backend produces from a module.
    type Output;

    /// A short identifier for this backend, e.g. `"evm"`.
    fn name(&self) -> &str;

    /// Lowers a module to this backend's output artifact. Takes `&mut` so the
    /// backend can run its own target-specific passes over the MIR first.
    fn lower_module(&mut self, module: &mut Module) -> Self::Output;
}
