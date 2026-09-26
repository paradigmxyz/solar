//! The `solarBuild` output: what a contract's bytecode depends on besides its sources.
//!
//! A compiler-owned module under `solar:core/` ships with the compiler, and the compiler lowers
//! its functions itself, so the version in an import path does not pin a build: the same source
//! can compile differently under another build, another target, or with intrinsic lowering off.
//! This output records all of it for a contract: the compiler's version and the commit it was
//! built from, the target EVM version, the optimization settings, the features that change code
//! generation, and the keccak256 of every compiler-owned module the contract's source reaches
//! through its imports.

use super::{data::FxIndexMap, metadata::collect_referenced_sources};
use alloy_primitives::keccak256;
use serde::Serialize;
use solar_config::version::{COMMIT_SHA, SEMVER_VERSION};
use solar_interface::source_map::FileName;
use solar_sema::{Gcx, core::is_core_file, hir::ContractId};

/// The build a contract was compiled by.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BuildOutput {
    compiler: Compiler,
    evm_version: String,
    optimization: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    optimizer_runs: Option<u64>,
    features: Features,
    /// The keccak256 of each compiler-owned module the contract's source reaches, by path.
    core_modules: FxIndexMap<String, String>,
}

#[derive(Debug, Serialize)]
struct Compiler {
    version: &'static str,
    commit: &'static str,
}

/// The settings that change what the modules compile to.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Features {
    /// Whether module functions are lowered by the compiler rather than compiled from their
    /// bodies (`-Zno-core-intrinsics` turns this off).
    core_intrinsics: bool,
}

/// The build output of the contract `id`.
pub(super) fn build_output(gcx: Gcx<'_>, id: ContractId) -> BuildOutput {
    let opts = &gcx.sess.opts;
    let mut core_modules = FxIndexMap::default();
    for source in collect_referenced_sources(gcx, gcx.hir.contract(id).source) {
        let file = &gcx.hir.source(source).file;
        if let FileName::Custom(path) = &file.name
            && is_core_file(&file.name)
        {
            core_modules.insert(path.clone(), format!("{:#x}", keccak256(file.src.as_bytes())));
        }
    }
    BuildOutput {
        compiler: Compiler { version: SEMVER_VERSION, commit: COMMIT_SHA },
        evm_version: opts.evm_version.to_string(),
        optimization: opts.optimization.to_string(),
        optimizer_runs: opts.optimizer_runs,
        features: Features { core_intrinsics: !opts.unstable.no_core_intrinsics },
        core_modules,
    }
}
