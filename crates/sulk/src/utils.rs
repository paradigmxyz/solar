use std::panic::PanicInfo;
use sulk_interface::{
    diagnostics::{DiagCtxt, ExplicitBug},
    SessionGlobals,
};

const BUG_REPORT_URL: &str = "https://github.com/paradigmxyz/sulk/issues/new/choose";

pub(crate) fn init_logger(early_dcx: &DiagCtxt) {
    if let Err(e) = try_init_logger() {
        early_dcx.fatal(e.to_string()).emit();
    }
}

fn try_init_logger() -> std::result::Result<(), impl std::fmt::Display> {
    use tracing_subscriber::prelude::*;

    tracing_subscriber::Registry::default()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_error::ErrorLayer::default())
        .with(tracing_subscriber::fmt::layer())
        .try_init()
}

pub(crate) fn install_panic_hook() {
    // If the user has not explicitly overridden "RUST_BACKTRACE", then produce
    // full backtraces. When a compiler ICE happens, we want to gather
    // as much information as possible to present in the issue opened
    // by the user. Compiler developers and other rustc users can
    // opt in to less-verbose backtraces by manually setting "RUST_BACKTRACE"
    // (e.g. `RUST_BACKTRACE=1`)
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "full");
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);

        // Separate the output with an empty line.
        eprintln!();

        panic_hook(info);
    }));
}

fn panic_hook(info: &PanicInfo<'_>) {
    let dcx = DiagCtxt::with_tty_emitter(None).set_flags(|f| f.track_diagnostics = false);

    // If the error was caused by a broken pipe then this is not a bug.
    // Write the error and return immediately. See #98700.
    #[cfg(windows)]
    if let Some(msg) = info.payload().downcast_ref::<String>() {
        if msg.starts_with("failed printing to stdout: ") && msg.ends_with("(os error 232)") {
            // the error code is already going to be reported when the panic unwinds up the stack
            let _ = dcx.err(msg.clone()).emit();
            return;
        }
    };

    // An explicit `bug()` call has already printed what it wants to print.
    if !info.payload().is::<ExplicitBug>() {
        dcx.err("the compiler unexpectedly panicked; this is a bug.").emit();
    }

    dcx.note(format!("we would appreciate a bug report: {BUG_REPORT_URL}")).emit();
}

pub(crate) fn run_in_thread_with_globals<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    const STACK_SIZE: usize = 1024 * 1024 * 8;

    let builder = std::thread::Builder::new().name("sulk".into()).stack_size(STACK_SIZE);
    std::thread::scope(|s| {
        let r = builder.spawn_scoped(s, move || SessionGlobals::new().set(f)).unwrap().join();
        match r {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    })
}
