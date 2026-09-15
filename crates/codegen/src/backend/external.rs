//! Invokes external native toolchains using Standard JSON.
//!
//! Yul uses solc's supported process interface. Errors, including failed process
//! exits and missing artifacts, propagate to the compiler diagnostics.

use super::{assembler::ImmutableRef, evm::EvmArtifact};
use crate::mir::{ImmutableId, Module, TypeSize};
use alloy_primitives::hex;
use serde_json::{Value, json};
use solar_config::OptimizationMode;
use solar_sema::Gcx;
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub(super) fn yul(gcx: Gcx<'_>, module: &Module, text: &str) -> Result<EvmArtifact, String> {
    let opts = &gcx.sess.opts;
    let input = json!({
        "language": "Yul", "sources": {"input.yul": {"content": text}},
        "settings": {
            "evmVersion": opts.evm_version.to_string(),
            "optimizer": {"enabled": !matches!(opts.optimization, OptimizationMode::None), "runs": opts.optimizer_runs.unwrap_or(if opts.optimization.is_size() {1} else {200})},
            "outputSelection": {"*": {"*": ["evm.bytecode.object", "evm.deployedBytecode.object", "evm.deployedBytecode.immutableReferences"]}}
        }
    });
    let executable = std::env::var_os("SOLAR_SOLC").unwrap_or_else(|| "solc".into());
    let mut child = Command::new(&executable)
        .arg("--standard-json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {executable:?}: {e}"))?;
    let input = serde_json::to_vec(&input).map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("missing compiler stdin")?;
    let output = std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(&input));
        let output = child.wait_with_output().map_err(|e| e.to_string());
        writer
            .join()
            .map_err(|_| "compiler input writer panicked".to_owned())?
            .map_err(|e| e.to_string())?;
        output
    })?;
    if !output.status.success() {
        return Err(format!(
            "solc exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let output: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid solc response: {e}"))?;
    if let Some(errors) = output["errors"].as_array() {
        let errors = errors
            .iter()
            .filter(|e| e["severity"] == "error")
            .map(|e| {
                e["formattedMessage"]
                    .as_str()
                    .or(e["message"].as_str())
                    .unwrap_or("unknown solc error")
            })
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
    }
    let evm = &output["contracts"]["input.yul"]["Contract"]["evm"];
    let decode = |kind: &str| {
        hex::decode(evm[kind]["object"].as_str().ok_or_else(|| format!("solc omitted {kind}"))?)
            .map_err(|e| e.to_string())
    };
    let mut runtime = decode("deployedBytecode")?;
    let mut immutable_references = Vec::new();
    if let Some(references) = evm["deployedBytecode"]["immutableReferences"].as_object() {
        for (name, locations) in references {
            let id = name
                .strip_prefix('i')
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or("invalid Yul immutable identifier")?;
            for location in locations.as_array().ok_or("invalid Yul immutable locations")? {
                let start = location["start"].as_u64().ok_or("invalid Yul immutable offset")?;
                if id >= module.immutable_count()
                    || start == 0
                    || start.checked_add(32).is_none_or(|end| end > runtime.len() as u64)
                    || runtime.get(start as usize - 1) != Some(&0x7f)
                    || location["length"].as_u64() != Some(32)
                {
                    return Err("invalid Yul immutable relocation".into());
                }
                immutable_references.push(ImmutableRef {
                    id: ImmutableId::from_usize(id),
                    code_offset: start as usize - 1,
                    type_size: TypeSize::new_int_bits(256),
                });
            }
        }
    }
    super::alternative::append_runtime_tail(module, &mut runtime);
    Ok(EvmArtifact {
        immutable_references,
        deployment: decode("bytecode")?,
        runtime,
        ..Default::default()
    })
}
