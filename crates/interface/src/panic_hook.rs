//! Functions for installing a custom panic hook.

use crate::diagnostics::{DiagCtxt, ExplicitBug, FatalAbort, flush_withheld};
use std::panic::PanicHookInfo;

const BUG_REPORT_URL: &str =
    "https://github.com/paradigmxyz/solar/issues/new/?labels=C-bug%2C+I-ICE&template=ice.yml";

/// Install the compiler's default panic hook.
pub fn install() {
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }

    update_hook(|default_hook, info| {
        if info.payload().is::<FatalAbort>() {
            std::process::exit(1);
        }

        // Release the diagnostics that parallel work still withholds, as the work will not finish.
        flush_withheld();

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

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};
    use std::process::Command;

    const TEST: &str = "panic_hook::tests::ice_prints_withheld_diagnostics_first";
    const CHILD: &str = "SOLAR_PANIC_HOOK_TEST_CHILD";

    /// A panic while a parallel phase withholds diagnostics prints them before the panic message.
    #[test]
    fn ice_prints_withheld_diagnostics_first() {
        if std::env::var_os(CHILD).is_some() {
            install();
            let dcx = DiagCtxt::with_stderr_emitter(None).with_flags(|flags| {
                flags.track_diagnostics = false;
            });
            // A finished item whose diagnostics wait for the end of the phase.
            let (_, _withheld) = dcx.defer(|| dcx.err("earlier error").emit());
            panic!("boom");
        }

        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_data_eq!(
            String::from_utf8(output.stderr).unwrap(),
            str![[r#"
...
error: earlier error

...
boom
...
error: the compiler unexpectedly panicked; this is a bug.
...
"#]]
        );
    }
}
