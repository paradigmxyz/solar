//! EVM backend integration and artifact ownership.
//!
//! The driver runs retained progressive MIR lowering, schedules runtime and
//! constructor programs independently, and publishes artifacts only after both
//! programs assemble successfully. Captures resolve generated deferred values
//! while preserving ordinary physical block and data identities.

use crate::{Backend, mir};
use solar_interface::Result;
use solar_sema::Gcx;

mod assembly;
mod calls;
mod debug_info;
mod deployment;
mod disasm;
mod entry_layout;
mod indexed;
pub mod ir;
mod machine;
pub(crate) mod op;
mod parallel_copy;
mod scheduler;
mod spills;
mod storage;
mod switches;
pub use debug_info::{DebugFunction, DebugFunctionExit, DebugInstruction};
pub use disasm::{disassemble, disassemble_standard_json};

/// Returns the canonical mnemonic for a defined opcode.
pub const fn opcode_mnemonic(opcode: u8) -> Option<&'static str> {
    op::name(opcode)
}

/// Machine-code output and optional physical IR captures for one contract.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Constructor prefix and embedded runtime bytes.
    pub deployment: Vec<u8>,
    /// Deployed runtime bytes.
    pub runtime: Vec<u8>,
    /// Deployment prefix before encoding.
    pub deployment_evm_ir: Option<ir::Module>,
    /// Runtime before encoding.
    pub runtime_evm_ir: Option<ir::Module>,
    /// Final deployment instruction locations when debug output is requested.
    pub deployment_debug_info: Option<Vec<DebugInstruction>>,
    /// Final runtime instruction locations when debug output is requested.
    pub runtime_debug_info: Option<Vec<DebugInstruction>>,
    /// Immutable placeholders, indexed at their PUSH opcodes.
    pub(crate) immutable_references: Vec<ImmutableReference>,
}

/// One fixed-width immutable placeholder in runtime code.
#[derive(Clone, Debug)]
pub(crate) struct ImmutableReference {
    pub(crate) id: mir::ImmutableId,
    pub(crate) code_offset: usize,
    pub(crate) type_size: mir::TypeSize,
}

/// Converts a MIR module into an EVM artifact.
pub struct EvmCodegen<'gcx> {
    gcx: Gcx<'gcx>,
    capture_evm_ir: bool,
    capture_mir: bool,
    capture_debug_info: bool,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Creates an independent generator.
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, capture_evm_ir: false, capture_mir: false, capture_debug_info: false }
    }

    /// Enables physical IR captures.
    pub fn set_capture_evm_ir(&mut self, capture: bool) {
        self.capture_evm_ir = capture;
    }

    /// Enables final instruction debug captures without changing generated code.
    pub fn set_capture_debug_info(&mut self, capture: bool) {
        self.capture_debug_info = capture;
    }

    pub(crate) fn set_capture_mir(&mut self, capture: bool) {
        self.capture_mir = capture;
    }

    /// Generates deployment and runtime bytes, emitting diagnostics on failure.
    pub fn generate_deployment_bytecode(&mut self, module: &mut mir::Module) -> (Vec<u8>, Vec<u8>) {
        let artifact = self.lower_module(module);
        (artifact.deployment, artifact.runtime)
    }
}

impl Backend for EvmCodegen<'_> {
    type Output = EvmArtifact;

    fn lower_module(&mut self, module: &mut mir::Module) -> EvmArtifact {
        self.generate(module).unwrap_or_default()
    }
}

impl EvmCodegen<'_> {
    fn generate(&mut self, module: &mut mir::Module) -> Result<EvmArtifact> {
        if module.is_interface {
            return Ok(EvmArtifact::default());
        }
        module.set_debug_info_tracked(self.capture_debug_info);
        let _changed = crate::pass::run_pipeline(self.gcx, module, None);
        self.gcx.dcx().has_errors()?;
        for (_, function) in module.iter_functions() {
            if let Some(instruction) = function
                .instructions()
                .map(|inst| function.inst(inst))
                .find(|inst| matches!(inst.kind, mir::InstKind::StoreImmutable(..)))
            {
                return Err(self
                    .gcx
                    .dcx()
                    .err("immutable assignments must be lowered before EVM codegen")
                    .span(instruction.metadata.source_span().unwrap_or(module.name.span))
                    .note(format!("remaining MIR slice is in function `{}`", function.name))
                    .emit());
            }
        }
        if module.phase != mir::MirPhase::EvmShaped {
            return Err(self
                .gcx
                .dcx()
                .err(format!(
                    "EVM code generation requires evm-shaped MIR, found `{}`",
                    module.phase.name()
                ))
                .emit());
        }
        let entry = module
            .dispatch_entry()
            .ok_or_else(|| self.gcx.dcx().err("MIR module has no runtime entry").emit())?;
        let version = self.gcx.sess.opts.evm_version;
        let optimization = self.gcx.sess.opts.optimization;
        let unstable = &self.gcx.sess.opts.unstable;
        let mut switches = switches::Planner::new(
            unstable.switch_lowering,
            optimization,
            version,
            unstable.switch_max_gas_code_growth,
            unstable.switch_max_bit_slice_gas_code_growth,
        );
        let mut runtime_ir = machine::lower(module, entry, version, optimization, &mut switches)
            .map_err(|message| self.gcx.dcx().err(message).emit())?
            .ir;
        runtime_ir.name = solar_interface::Symbol::intern(&format!("{}_runtime", module.name));
        let _changed = ir::run_pipeline(self.gcx, &mut runtime_ir, None);
        ir::finish_lowering(self.gcx, &mut runtime_ir)?;
        let runtime = assembly::assemble(self.gcx, &runtime_ir)?;
        let mut deployment_ir = deployment::lower(
            module,
            runtime.bytes.clone(),
            &runtime.immutable_references,
            version,
            optimization,
            &mut switches,
        )
        .map_err(|message| self.gcx.dcx().err(message).emit())?;
        let _changed = ir::run_pipeline(self.gcx, &mut deployment_ir, None);
        ir::finish_lowering(self.gcx, &mut deployment_ir)?;
        let deployment = assembly::assemble(self.gcx, &deployment_ir)?;
        resolve_capture(&mut runtime_ir, runtime.bytes.len());
        resolve_capture(&mut deployment_ir, deployment.bytes.len());
        Ok(EvmArtifact {
            deployment_debug_info: deployment.debug_info,
            runtime_debug_info: runtime.debug_info,
            deployment: deployment.bytes,
            runtime: runtime.bytes,
            runtime_evm_ir: self.capture_evm_ir.then_some(runtime_ir),
            deployment_evm_ir: self.capture_evm_ir.then_some(deployment_ir),
            immutable_references: runtime.immutable_references,
        })
    }
}

/// Resolves deferred values and preserves runtime trailers in physical IR captures.
fn resolve_capture(module: &mut ir::Module, program_size: usize) {
    // Public captures carry only the guarantees represented in their text format.
    module.private_control_labels = false;
    for block in &mut module.blocks {
        for instruction in &mut block.insts {
            if let ir::InstKind::PushDeferred(id) = instruction.kind {
                let value = if module.program_size_id == Some(id) {
                    Some(alloy_primitives::U256::from(program_size))
                } else if module.appendix_start_id == Some(id) {
                    Some(alloy_primitives::U256::from(program_size - module.appendix.len()))
                } else {
                    module.deferred.get(&id).copied()
                };
                if let Some(value) = value {
                    // push <resolved deferred value>
                    instruction.kind = ir::InstKind::Push(value);
                }
            }
        }
    }
    if module.appendix_start_id.is_none() && !module.appendix.is_empty() {
        // <runtime code>; <referenced data>; <opaque trailer as final captured data>
        module.data.push(ir::Data { name: None, bytes: std::mem::take(&mut module.appendix) });
    }
    module.program_size_id = None;
    module.appendix_start_id = None;
    module.appendix.clear();
}

#[cfg(test)]
mod tests {
    use super::{ir, resolve_capture};

    #[test]
    fn capture_drops_private_control_label_proof() {
        let untrusted = ir::Module::default();
        assert!(!untrusted.private_control_labels);
        let mut generated = ir::Module { private_control_labels: true, ..untrusted.clone() };
        resolve_capture(&mut generated, 0);
        assert_eq!(generated, untrusted);
    }
}
