//! Native plugin loading shared by compiler stages.

use crate::{
    __private::{ABI_SYMBOL, ENTRY_SYMBOL, Entry, Exports, Receiver, build_fingerprint},
    Expansion, ExpansionRequest, FrontendManifest, Manifest, Request, Response,
};
use std::{path::Path, sync::Mutex};

#[cfg(any(unix, windows))]
use libloading::Library;

/// A loaded native library. Its callbacks are serialized and the library stays loaded until drop.
pub struct NativePlugin {
    exports: &'static Exports,
    calls: Mutex<()>,
    // Drop the library only after every borrowed export and callback has finished.
    #[cfg(any(unix, windows))]
    _library: Library,
}

impl NativePlugin {
    /// Loads explicitly requested native code after checking its SDK and toolchain fingerprint.
    pub fn load(path: &Path) -> Result<Self, String> {
        #[cfg(any(unix, windows))]
        {
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
            Ok(Self { exports, calls: Mutex::new(()), _library: library })
        }
        #[cfg(not(any(unix, windows)))]
        Err(format!("native pass plugins are not supported on this target: `{}`", path.display()))
    }

    /// Returns the plugin's registration data.
    pub fn manifest(&self) -> Result<Manifest, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.manifest)(respond))
    }

    /// Runs one pass and copies its response into the host.
    pub fn run(&self, request: &Request) -> Result<Response, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.run)(request, respond))
    }

    /// Returns the syntax extensions registered by this library.
    pub fn frontend_manifest(&self) -> Result<FrontendManifest, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.frontend_manifest)(respond))
    }

    /// Runs an extension and copies the result into the host.
    pub fn expand(&self, request: &ExpansionRequest) -> Result<Expansion, String> {
        let _guard = self.calls.lock().map_err(|_| "plugin call lock is poisoned")?;
        receive(|respond| (self.exports.expand)(request, respond))
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
