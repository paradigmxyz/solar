//! Native pass registration and loading.
//!
//! Rust libraries implement the SDK's `Plugin` trait and use its export macro. The loader checks
//! the SDK and Rust build fingerprint before using typed Rust callbacks. No byte protocol or
//! foreign-language interface is exposed.
//! Registration is cached per compilation; libraries outlive their callbacks and each compilation
//! serializes calls into its library handles. MIR and EVM IR adapters submit edits through the
//! normal pass managers, without granting foreign code access to Rust compiler objects. The host
//! clones borrowed responses and rejects missing, duplicate, or incompatible replies. Native
//! libraries are trusted executable code, not a sandbox. Validation checks IR
//! invariants, not semantic equivalence of a third-party transformation.
//! Library contents are hashed before initialization and checked again before registration.

use crate::{backend::evm::CodegenState, mir::MirPhase};
use alloy_primitives::{B256, keccak256};
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_pass_plugin::{API_VERSION, Ir, Request, Response, host::NativePlugin};
use solar_sema::Gcx;
use std::{path::Path, sync::Arc};

mod evm;
mod mir;

pub(crate) use evm::EvmPluginPass;
pub(crate) use mir::MirPluginPass;

#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) mir: Vec<MirPluginPass>,
    pub(crate) evm: Vec<EvmPluginPass>,
    identities: Vec<String>,
}

struct Pass {
    name: String,
    local_name: String,
    default: bool,
    phase: Option<MirPhase>,
    plugin: Arc<NativePlugin>,
}

impl Pass {
    fn run(
        &self,
        gcx: Gcx<'_>,
        optimization: String,
        module: solar_pass_plugin::Snapshot,
    ) -> Result<Response, String> {
        let request = Request {
            pass: self.local_name.clone(),
            evm_version: gcx.sess.opts.evm_version.to_string(),
            optimization,
            module,
        };
        self.plugin.run(&request)
    }

    fn error(&self, gcx: Gcx<'_>, error: impl std::fmt::Display) -> ErrorGuaranteed {
        gcx.dcx().err(format!("plugin pass `{}` failed: {error}", self.name)).emit()
    }
}

pub(crate) fn registry(gcx: Gcx<'_>) -> Result<Option<&Registry>, ErrorGuaranteed> {
    if gcx.sess.opts.unstable.load_pass_plugin.is_empty() {
        return Ok(None);
    }
    let loaded = gcx.codegen_state::<CodegenState>().plugins.get_or_init(|| {
        let mut registry = Registry::default();
        let plugins = gcx.plugins().map_err(|_| "failed to load plugins".to_owned())?;
        for (path, plugin) in gcx.sess.opts.unstable.load_pass_plugin.iter().zip(plugins) {
            registry.load(path, Arc::clone(plugin))?;
        }
        Ok(registry)
    });
    loaded.as_ref().map(Some).map_err(|error| gcx.dcx().err(error.clone()).emit())
}

impl Registry {
    fn load(&mut self, path: &Path, plugin: Arc<NativePlugin>) -> Result<(), String> {
        // Native libraries must exist on the host filesystem, outside virtual source loaders.
        #[allow(clippy::disallowed_methods)]
        let path = std::fs::canonicalize(path)
            .map_err(|error| format!("cannot open pass plugin `{}`: {error}", path.display()))?;
        let digest = library_digest(&path)?;
        let manifest = plugin.manifest()?;
        // Loading and initialization can replace a writable library path. Reject changed
        // contents rather than record bytes that did not supply the loaded callbacks.
        if library_digest(&path)? != digest {
            return Err("pass plugin library changed while loading".into());
        }
        if manifest.api_version != API_VERSION {
            return Err(format!(
                "plugin `{}` requires API {}, expected {API_VERSION}",
                manifest.name, manifest.api_version
            ));
        }
        let frontend = plugin.frontend_manifest()?;
        if !valid_name(&manifest.name)
            || manifest.version.is_empty()
            || (manifest.passes.is_empty()
                && frontend.extensions.is_empty()
                && frontend.builtins.is_empty())
        {
            return Err(format!(
                "invalid plugin identity or empty pass list in `{}`",
                path.display()
            ));
        }
        self.identities.push(format!("{}@{}:{:#x}", manifest.name, manifest.version, digest));
        for definition in manifest.passes {
            let name = format!("{}::{}", manifest.name, definition.name);
            if !valid_name(&definition.name)
                || self.mir.iter().any(|pass| pass.0.name == name)
                || self.evm.iter().any(|pass| pass.0.name == name)
            {
                return Err(format!("invalid or duplicate plugin pass `{name}`"));
            }
            let phase = match (definition.ir, definition.phase.as_deref()) {
                (Ir::Mir, Some("semantic")) => Some(MirPhase::Semantic),
                (Ir::Mir, Some("lowered")) => Some(MirPhase::Lowered),
                (Ir::EvmIr, None) => None,
                _ => return Err(format!("invalid IR phase for plugin pass `{name}`")),
            };
            let pass = Pass {
                name,
                local_name: definition.name,
                default: definition.enabled_by_default,
                phase,
                plugin: Arc::clone(&plugin),
            };
            match definition.ir {
                Ir::Mir => self.mir.push(MirPluginPass(pass)),
                Ir::EvmIr => self.evm.push(EvmPluginPass(pass)),
            }
        }
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Ordered plugin identities, including the digest of each loaded native library.
pub fn build_inputs(gcx: Gcx<'_>) -> Result<Vec<String>, ErrorGuaranteed> {
    Ok(registry(gcx)?.map_or_else(Vec::new, |registry| registry.identities.clone()))
}

fn library_digest(path: &Path) -> Result<B256, String> {
    // Native libraries are build inputs, outside the virtual Solidity source loader.
    #[allow(clippy::disallowed_methods)]
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read pass plugin: {error}"))?;
    Ok(keccak256(bytes))
}
