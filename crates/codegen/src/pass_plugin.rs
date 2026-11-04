//! Native pass registration and loading.
//!
//! Libraries export a versioned C entry point speaking the `solar-pass-plugin` JSON protocol.
//! Registration is cached per compilation; libraries outlive their callbacks and each compilation
//! serializes calls into its library handles. MIR and EVM IR adapters submit edits through the
//! normal pass managers, without granting foreign code access to Rust compiler objects. The host
//! owns response buffers and rejects missing, duplicate, oversized, malformed, or incompatible
//! replies. Native libraries are trusted executable code, not a sandbox. Validation checks IR
//! invariants, not semantic equivalence of a third-party transformation.

use crate::{backend::evm::CodegenState, mir::MirPhase};
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_pass_plugin::{API_VERSION, Entry, FAILURE, Ir, Manifest, Request, Response, SUCCESS};
use solar_sema::Gcx;
use std::{
    ffi::c_void,
    path::Path,
    sync::{Arc, Mutex},
};

#[cfg(any(unix, windows))]
use libloading::Library;

mod evm;
mod mir;

pub(crate) use evm::EvmPluginPass;
pub(crate) use mir::MirPluginPass;

#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) mir: Vec<MirPluginPass>,
    pub(crate) evm: Vec<EvmPluginPass>,
}

struct NativePlugin {
    // The library must remain loaded until every entry-point call has returned.
    #[cfg(any(unix, windows))]
    _library: Library,
    entry: Entry,
    calls: Mutex<()>,
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
        let request = Request::Run {
            api_version: API_VERSION,
            pass: self.local_name.clone(),
            evm_version: gcx.sess.opts.evm_version.to_string(),
            optimization,
            module,
        };
        let bytes = self.plugin.call(&request)?;
        serde_json::from_slice(&bytes).map_err(|error| format!("invalid pass response: {error}"))
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
        for path in &gcx.sess.opts.unstable.load_pass_plugin {
            registry.load(path)?;
        }
        Ok(registry)
    });
    loaded.as_ref().map(Some).map_err(|error| gcx.dcx().err(error.clone()).emit())
}

impl Registry {
    fn load(&mut self, path: &Path) -> Result<(), String> {
        // Native libraries must exist on the host filesystem, outside virtual source loaders.
        #[allow(clippy::disallowed_methods)]
        let path = std::fs::canonicalize(path)
            .map_err(|error| format!("cannot open pass plugin `{}`: {error}", path.display()))?;
        let plugin = Arc::new(NativePlugin::load(&path)?);
        let bytes = plugin.call(&Request::Describe { api_version: API_VERSION })?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|error| {
            format!("invalid plugin manifest from `{}`: {error}", path.display())
        })?;
        if manifest.api_version != API_VERSION {
            return Err(format!(
                "plugin `{}` requires API {}, expected {API_VERSION}",
                manifest.name, manifest.api_version
            ));
        }
        if !valid_name(&manifest.name) || manifest.version.is_empty() || manifest.passes.is_empty()
        {
            return Err(format!(
                "invalid plugin identity or empty pass list in `{}`",
                path.display()
            ));
        }
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

impl NativePlugin {
    fn load(path: &Path) -> Result<Self, String> {
        #[cfg(any(unix, windows))]
        {
            // SAFETY: Loading was explicitly requested by the user. The library must obey
            // the documented entry-point ABI, including its initialization/destruction code.
            let library = unsafe { Library::new(path) }.map_err(|error| {
                format!("cannot load pass plugin `{}`: {error}", path.display())
            })?;
            // SAFETY: Version 1 fixes this symbol's C signature. Keep the library with it.
            let entry = unsafe { library.get::<Entry>(solar_pass_plugin::ENTRY_SYMBOL) }.map_err(
                |error| {
                    format!(
                        "pass plugin `{}` has no `solar_pass_plugin_v1` entry point: {error}",
                        path.display()
                    )
                },
            )?;
            Ok(Self { entry: *entry, _library: library, calls: Mutex::new(()) })
        }
        #[cfg(not(any(unix, windows)))]
        Err(format!("native pass plugins are not supported on this target: `{}`", path.display()))
    }

    fn call(&self, request: &Request) -> Result<Vec<u8>, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        let request = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        let mut response = ResponseBuffer::default();
        // SAFETY: The input and response context outlive this synchronous call. Plugins must
        // not retain pointers, unwind, or invoke the writer asynchronously.
        let status = unsafe {
            (self.entry)(
                request.as_ptr(),
                request.len(),
                write_response,
                (&mut response as *mut ResponseBuffer).cast(),
            )
        };
        if response.invalid {
            return Err("plugin wrote an invalid, duplicate, or oversized response".into());
        }
        let bytes = response.bytes.ok_or("plugin did not write a response")?;
        match status {
            SUCCESS => Ok(bytes),
            FAILURE => Err(String::from_utf8_lossy(&bytes).into_owned()),
            _ => Err(format!("plugin returned unknown status {status}")),
        }
    }
}

#[derive(Default)]
struct ResponseBuffer {
    bytes: Option<Vec<u8>>,
    invalid: bool,
}

unsafe extern "C" fn write_response(context: *mut c_void, bytes: *const u8, length: usize) -> u32 {
    // SAFETY: NativePlugin::call owns this context until the entry point returns.
    let response = unsafe { &mut *context.cast::<ResponseBuffer>() };
    if response.bytes.is_some() || length > 64 * 1024 * 1024 || (length != 0 && bytes.is_null()) {
        response.invalid = true;
        return FAILURE;
    }
    // SAFETY: The plugin lends a readable buffer for this call. Empty buffers need no pointer.
    response.bytes = Some(if length == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec()
    });
    SUCCESS
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
