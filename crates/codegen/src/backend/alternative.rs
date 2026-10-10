//! Selects optional MIR backends and reports target failures as diagnostics.
//!
//! All backends consume the same canonical MIR pipeline. They must either emit
//! their own artifact or report an error; selection never falls back to EVM.

use super::evm::EvmArtifact;
use crate::{
    link::EmbeddedBytecodes,
    mir::{Module, pass::run_pipeline},
};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

#[cfg(any(
    feature = "codegen-yul",
    feature = "codegen-sonatina",
    feature = "codegen-sir",
    feature = "codegen-llvm"
))]
use solar_config::CodegenBackend;

pub(crate) fn compile(
    gcx: Gcx<'_>,
    module: &mut Module,
    children: &EmbeddedBytecodes,
) -> EvmArtifact {
    if module.is_interface {
        return EvmArtifact::default();
    }
    for data in &mut module.data {
        if let crate::link::DataBytes::Deferred(code) = data.bytes {
            let bytecode = code.bytecode(children);
            data.library_relocations = bytecode.relocations_in(&mut module.libraries);
            data.bytes = crate::link::DataBytes::Known(bytecode.bytes.clone());
        }
    }
    for function in &mut module.functions {
        let sizes = function
            .instructions()
            .filter_map(|id| {
                if let crate::mir::InstKind::DataSize(size) = function.inst(id).kind {
                    Some((id, size.value(module.data[size.data].bytes.linked().len())))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut replacements = FxHashMap::default();
        for (id, value) in &sizes {
            let value = function
                .alloc_value(crate::mir::Value::Immediate(crate::mir::Immediate::I256(*value)));
            replacements
                .insert(function.inst_result_value(*id).expect("data size has a result"), value);
        }
        function.replace_uses(&replacements);
        for block in &mut function.blocks {
            block.instructions.retain(|id| !sizes.iter().any(|(size, _)| size == id));
        }
    }
    let _changed = run_pipeline(gcx, module, None);
    if gcx.dcx().has_errors().is_err() {
        return EvmArtifact::default();
    }
    // NOTE: These backends do not translate source metadata. Their debug artifacts
    // remain absent; requesting debug output must not change executable lowering.
    if !module.libraries.is_empty() {
        gcx.dcx()
            .err(format!(
                "{} backend requires library addresses to be supplied before code generation",
                gcx.sess.opts.codegen_backend
            ))
            .span(module.name.span)
            .help("set Standard JSON settings.libraries to the deployed library addresses")
            .emit();
        return EvmArtifact::default();
    }
    let Ok(module) = module.as_lowered(gcx.dcx()) else { return EvmArtifact::default() };
    let module = &*module;
    let result: Result<EvmArtifact, String> = {
        match gcx.sess.opts.codegen_backend {
            #[cfg(feature = "codegen-yul")]
            CodegenBackend::Yul => super::yul::compile(gcx, module),
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

/// Reserves the shared return area above MIR's fixed locals and immutable staging.
#[cfg(any(
    feature = "codegen-yul",
    feature = "codegen-sonatina",
    feature = "codegen-sir",
    feature = "codegen-llvm"
))]
pub(super) fn memory_layout(module: &Module) -> (u64, u64) {
    let fixed_end = module
        .functions
        .iter()
        .map(|f| 128 + f.internal_frame_size.max(f.external_static_return_size))
        .max()
        .unwrap_or(128);
    let staging_end = crate::mir::immutable::immutable_staging_end(
        crate::mir::immutable::immutable_staging_base(module),
        module.immutable_count(),
    );
    let returns = module.functions.iter().map(|f| f.return_components().len()).max().unwrap_or(0);
    let return_base = fixed_end.max(staging_end);
    (return_base, return_base + if returns > 1 { returns as u64 * 32 } else { 0 })
}

/// Returns the activation size when a function still uses explicit internal-frame addresses.
#[cfg(any(
    feature = "codegen-yul",
    feature = "codegen-sonatina",
    feature = "codegen-sir",
    feature = "codegen-llvm"
))]
pub(super) fn frame_size(f: &crate::mir::Function) -> u64 {
    if f.instructions()
        .any(|i| matches!(f.inst(i).kind, crate::mir::InstKind::InternalFrameAddr(_)))
    {
        64 + (f.params.len() + f.return_components().len()) as u64 * 32 + f.internal_frame_size
    } else {
        0
    }
}

/// Appends fixed-width immutable placeholders after native runtime code.
#[cfg(any(feature = "codegen-sir", feature = "codegen-sonatina"))]
pub(super) fn append_immutable_data(
    module: &Module,
    runtime: &mut Vec<u8>,
) -> Vec<super::assembler::ImmutableRef> {
    let mut references = Vec::new();
    // PUSH32 0: the operand is patched during deployment and read with CODECOPY.
    for (id, _) in module.iter_immutables() {
        references.push(super::assembler::ImmutableRef {
            id,
            code_offset: runtime.len(),
            type_size: crate::mir::TypeSize::new_int_bits(256),
        });
        runtime.push(0x7f);
        runtime.resize(runtime.len() + 32, 0);
    }
    append_runtime_tail(module, runtime);
    references
}

/// Returns the size of opaque data that must remain at the runtime's end.
#[cfg(any(feature = "codegen-sonatina", feature = "codegen-sir"))]
pub(super) fn runtime_tail_size(module: &Module) -> usize {
    module
        .data
        .iter()
        .filter(|data| data.emit_in_runtime)
        .map(|data| data.bytes.linked().len())
        .sum()
}

/// Preserves the caller's opaque runtime suffix after native code and immutable data.
#[cfg(any(
    feature = "codegen-yul",
    feature = "codegen-sonatina",
    feature = "codegen-sir",
    feature = "codegen-llvm"
))]
pub(super) fn append_runtime_tail(module: &Module, runtime: &mut Vec<u8>) {
    for (id, data) in module.data.iter_enumerated() {
        let bytes = data.bytes.linked();
        if module.data[id].emit_in_runtime {
            runtime.extend_from_slice(bytes);
        }
    }
}
