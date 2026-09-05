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
mod deployment;
mod disasm;
mod entry_layout;
mod indexed;
pub mod ir;
mod machine;
pub(crate) mod op;
mod scheduler;
mod spills;
mod storage;
mod switches;
pub use disasm::{disassemble, disassemble_standard_json};

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
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Creates an independent generator.
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, capture_evm_ir: false, capture_mir: false }
    }

    /// Enables physical IR captures.
    pub fn set_capture_evm_ir(&mut self, capture: bool) {
        self.capture_evm_ir = capture;
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
        let _changed = crate::pass::run_pipeline(self.gcx, module, None);
        self.gcx.dcx().has_errors()?;
        for (_, function) in module.iter_functions() {
            if function
                .instructions()
                .any(|inst| matches!(function.inst(inst).kind, mir::InstKind::StoreImmutable(..)))
            {
                return Err(self
                    .gcx
                    .dcx()
                    .err("immutable assignments must be lowered before EVM codegen")
                    .span(module.name.span)
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
        self.gcx.dcx().has_errors()?;
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
        self.gcx.dcx().has_errors()?;
        let deployment = assembly::assemble(self.gcx, &deployment_ir)?;
        resolve_capture(&mut runtime_ir, runtime.bytes.len());
        resolve_capture(&mut deployment_ir, deployment.bytes.len());
        Ok(EvmArtifact {
            deployment: deployment.bytes,
            runtime: runtime.bytes,
            runtime_evm_ir: self.capture_evm_ir.then_some(runtime_ir),
            deployment_evm_ir: self.capture_evm_ir.then_some(deployment_ir),
            immutable_references: runtime.immutable_references,
        })
    }
}

/// Resolves generated deferred values before exporting physical IR captures.
fn resolve_capture(module: &mut ir::Module, program_size: usize) {
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
    module.program_size_id = None;
    module.appendix_start_id = None;
    module.appendix.clear();
}
