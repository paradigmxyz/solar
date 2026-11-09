//! Native plugin loading shared by compiler stages. Library contents are hashed before loading
//! and checked after initialization and registration callbacks. The captured digest stays with
//! the loaded library so later metadata never hashes a replacement at the original path.

use crate::{
    __private::{ABI_SYMBOL, ENTRY_SYMBOL, Entry, Exports, Receiver, build_fingerprint},
    BuiltinBody, Expansion, ExpansionRequest, FrontendManifest, Manifest, Request, Response,
};
use alloy_primitives::{B256, keccak256};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[cfg(any(unix, windows))]
use libloading::Library;

/// A loaded native library. Its callbacks are serialized and the library stays loaded until drop.
pub struct NativePlugin {
    exports: &'static Exports,
    path: PathBuf,
    digest: B256,
    calls: Mutex<()>,
    manifest: OnceLock<Result<Manifest, String>>,
    frontend: OnceLock<Result<FrontendManifest, String>>,
    // Drop the library only after every borrowed export and callback has finished.
    #[cfg(any(unix, windows))]
    _library: Library,
}

impl NativePlugin {
    /// Loads explicitly requested native code after checking its SDK and toolchain fingerprint.
    pub fn load(path: &Path) -> Result<Self, String> {
        #[cfg(any(unix, windows))]
        {
            let digest = library_digest(path)?;
            // SAFETY: Loading was explicitly requested. Libraries must use the Rust SDK,
            // including its export adapter and initialization/destruction contract.
            let library = unsafe { Library::new(path) }.map_err(|error| {
                format!("cannot load pass plugin `{}`: {error}", path.display())
            })?;
            // SAFETY: The SDK exports this identity as exactly 32 bytes. Inspect it before
            // interpreting any Rust-layout values or calling an entry point.
            let fingerprint =
                unsafe { library.get::<*const [u8; 32]>(ABI_SYMBOL) }.map_err(|_| {
                    format!("pass plugin `{}` is missing the Rust SDK fingerprint", path.display())
                })?;
            // SAFETY: A trusted SDK library exports the fixed-size byte array above.
            if unsafe { &**fingerprint } != &build_fingerprint(cfg!(panic = "unwind")) {
                return Err(format!(
                    "pass plugin `{}` was built with an incompatible Rust SDK or toolchain",
                    path.display()
                ));
            }
            // SAFETY: The fingerprint matches this SDK and Rust build. The export macro fixes
            // the Rust signature, and the library stays loaded while its exports are borrowed.
            let entry = unsafe { library.get::<Entry>(ENTRY_SYMBOL) }.map_err(|error| {
                format!(
                    "pass plugin `{}` has no `solar_rust_pass_plugin_v1` entry point: {error}",
                    path.display()
                )
            })?;
            let exports = entry();
            let plugin = Self {
                exports,
                path: path.to_owned(),
                digest,
                calls: Mutex::new(()),
                manifest: OnceLock::new(),
                frontend: OnceLock::new(),
                _library: library,
            };
            plugin.verify_identity()?;
            Ok(plugin)
        }
        #[cfg(not(any(unix, windows)))]
        Err(format!("native pass plugins are not supported on this target: `{}`", path.display()))
    }

    /// Returns the plugin's registration data.
    pub fn manifest(&self) -> Result<Manifest, String> {
        self.manifest
            .get_or_init(|| {
                let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
                let manifest = receive(|respond| (self.exports.manifest)(respond))?;
                self.verify_identity()?;
                Ok(manifest)
            })
            .clone()
    }

    /// Runs one pass and copies its response into the host.
    pub fn run(&self, request: &Request) -> Result<Response, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.run)(request, respond))
    }

    /// Requests a builtin's MIR recipe from its owning library.
    pub fn lower_builtin(&self, name: &str) -> Result<BuiltinBody, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.lower_builtin)(name, respond))
    }

    /// Returns the syntax extensions registered by this library.
    pub fn frontend_manifest(&self) -> Result<FrontendManifest, String> {
        self.frontend
            .get_or_init(|| {
                let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
                let frontend = receive(|respond| (self.exports.frontend_manifest)(respond))?;
                self.verify_identity()?;
                Ok(frontend)
            })
            .clone()
    }

    /// Runs an extension and copies the result into the host.
    pub fn expand(&self, request: &ExpansionRequest<'_>) -> Result<Expansion, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.expand)(request, respond))
    }

    /// Returns the library content hash captured before loading and initialization.
    pub fn content_hash(&self) -> B256 {
        self.digest
    }

    fn verify_identity(&self) -> Result<(), String> {
        if library_digest(&self.path)? != self.digest {
            return Err("pass plugin library changed while loading".into());
        }
        Ok(())
    }
}

fn receive<T: Clone>(call: impl FnOnce(&mut Receiver<'_, T>)) -> Result<T, String> {
    let mut response = None;
    let mut duplicate = false;
    call(&mut |value| {
        if response.is_some() {
            duplicate = true;
        } else {
            response = Some(value.cloned().map_err(str::to_owned));
        }
    });
    if duplicate {
        return Err("Rust plugin returned more than one response".into());
    }
    response.ok_or_else(|| "Rust plugin did not return a response".to_owned())?
}

fn library_digest(path: &Path) -> Result<B256, String> {
    // Native libraries are build inputs, outside the virtual Solidity source loader.
    #[allow(clippy::disallowed_methods)]
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read pass plugin: {error}"))?;
    Ok(keccak256(bytes))
}
