//! Selects optional MIR backends and reports target failures as diagnostics.
//!
//! All backends consume the same canonical MIR pipeline. They must either emit
//! their own artifact or report an error; selection never falls back to EVM.

use super::evm::EvmArtifact;
use crate::mir::{MirPhase, Module, pass::run_pipeline};
#[cfg(any(
    feature = "codegen-yul",
    feature = "codegen-sonatina",
    feature = "codegen-sir",
    feature = "codegen-llvm"
))]
use solar_config::CodegenBackend;
use solar_sema::Gcx;

pub(crate) fn compile(gcx: Gcx<'_>, module: &mut Module) -> EvmArtifact {
    if module.is_interface {
        return EvmArtifact::default();
    }
    let _changed = run_pipeline(gcx, module, None);
    if gcx.dcx().has_errors().is_err() {
        return EvmArtifact::default();
    }
    // NOTE: These backends do not translate source metadata. Their debug artifacts
    // remain absent; requesting debug output must not change executable lowering.
    let result: Result<EvmArtifact, String> = if module.phase != MirPhase::EvmShaped {
        Err(format!("codegen requires `evm-shaped` MIR, found `{}`", module.phase.name()))
    } else {
        match gcx.sess.opts.codegen_backend {
            #[cfg(feature = "codegen-yul")]
            CodegenBackend::Yul => super::yul::lower(module).and_then(|text| {
                super::external::yul(gcx, &text).map(|mut artifact| {
                    artifact.backend_ir = Some(text);
                    artifact
                })
            }),
            #[cfg(feature = "codegen-sonatina")]
            CodegenBackend::Sonatina
                if gcx.sess.opts.evm_version == solar_config::EvmVersion::Osaka =>
            {
                super::sonatina::compile(module, gcx.sess.opts.optimization)
            }
            #[cfg(feature = "codegen-sonatina")]
            CodegenBackend::Sonatina => {
                Err("Sonatina currently requires --evm-version osaka".into())
            }
            #[cfg(feature = "codegen-sir")]
            CodegenBackend::Sir if gcx.sess.opts.evm_version == solar_config::EvmVersion::Osaka => {
                super::sir::compile(module, gcx.sess.opts.optimization)
            }
            #[cfg(feature = "codegen-sir")]
            CodegenBackend::Sir => Err("SIR currently requires --evm-version osaka".into()),
            #[cfg(feature = "codegen-llvm")]
            CodegenBackend::Llvm
                if gcx.sess.opts.evm_version == solar_config::EvmVersion::Osaka =>
            {
                super::llvm::compile(module, gcx.sess.opts.optimization)
            }
            #[cfg(feature = "codegen-llvm")]
            CodegenBackend::Llvm => Err("LLVM currently requires --evm-version osaka".into()),
            backend => Err(format!("backend `{backend}` is unavailable in this build")),
        }
    };
    match result {
        Ok(mut artifact) => {
            if !gcx
                .sess
                .opts
                .unstable
                .dump
                .as_ref()
                .is_some_and(|dump| dump.kinds.contains(&solar_config::DumpKind::BackendIr))
            {
                artifact.backend_ir = None;
            }
            artifact
        }
        Err(error) => {
            gcx.dcx()
                .err(format!("{} backend: {error}", gcx.sess.opts.codegen_backend))
                .span(module.name.span)
                .emit();
            EvmArtifact::default()
        }
    }
}
