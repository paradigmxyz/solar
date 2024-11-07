//! Functions for installing a custom panic hook.

use crate::diagnostics::{DiagCtxt, ExplicitBug, FatalAbort};
use std::panic::PanicHookInfo;

const BUG_REPORT_URL: &str =
    "https://github.com/paradigmxyz/solar/issues/new/?labels=C-bug%2C+I-ICE&template=ice.yml";

/// Install the compiler's default panic hook.
pub fn install() {
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    update_hook(|default_hook, info| {
        if info.payload().is::<FatalAbort>() {
            std::process::exit(1);
        }

        // Lock stderr to prevent interleaving of concurrent panics.
        let _guard = std::io::stderr().lock();

        default_hook(info);

        // Separate the output with an empty line.
        eprintln!();

        panic_hook(info);
    });
}

fn panic_hook(info: &PanicHookInfo<'_>) {
    let dcx = DiagCtxt::new_early();

    // An explicit `bug()` call has already printed what it wants to print.
    if !info.payload().is::<ExplicitBug>() {
        dcx.err("the compiler unexpectedly panicked; this is a bug.").emit();
    }

    dcx.note(format!("we would appreciate a bug report: {BUG_REPORT_URL}")).emit();
}

#[cfg(feature = "nightly")]
use std::panic::update_hook;

/// Polyfill for [`std::panic::update_hook`].
#[cfg(not(feature = "nightly"))]
fn update_hook<F>(hook_fn: F)
where
    F: Fn(&(dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static), &PanicHookInfo<'_>)
        + Sync
        + Send
        + 'static,
{
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| hook_fn(&default_hook, info)));
}
