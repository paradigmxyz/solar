use std::panic::PanicInfo;
use sulk_interface::{
    diagnostics::{DiagCtxt, ExplicitBug},
    Result, SessionGlobals,
};

const BUG_REPORT_URL: &str = "https://github.com/paradigmxyz/sulk/issues/new/choose";

fn early_dcx() -> DiagCtxt {
    DiagCtxt::with_tty_emitter(None)
}

pub(crate) fn init_logger() -> Result {
    try_init_logger().map_err(|e| early_dcx().err(e.to_string()).emit())
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
    // If the user has not explicitly overridden "RUST_BACKTRACE", then produce full backtraces.
    // When a compiler ICE happens, we want to gather as much information as possible
    // to present in the issue opened by the user.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "full");
    }

    update_hook(|default_hook, info| {
        default_hook(info);

        // Separate the output with an empty line.
        eprintln!();

        panic_hook(info);
    });
}

fn panic_hook(info: &PanicInfo<'_>) {
    let dcx = early_dcx().set_flags(|f| f.track_diagnostics = false);

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

fn init_stack_size() -> usize {
    const DEFAULT_STACK_SIZE: usize = 1024 * 1024 * 8;
    DEFAULT_STACK_SIZE
}

pub(crate) fn run_in_thread_pool_with_globals<R: Send>(
    threads: usize,
    f: impl FnOnce() -> R + Send,
) -> R {
    if threads == 1 {
        return run_in_thread_with_globals(f);
    }

    let builder = rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("sulk-{i}"))
        .num_threads(threads)
        .stack_size(init_stack_size());

    // We create the session globals on the main thread, then create the thread
    // pool. Upon creation, each worker thread created gets a copy of the
    // session globals in TLS. This is possible because `SessionGlobals` impls
    // `Send` in the parallel compiler.
    SessionGlobals::new().set(|| {
        SessionGlobals::with(|session_globals| {
            builder
                .build_scoped(
                    // Initialize each new worker thread when created.
                    move |thread| session_globals.set(|| thread.run()),
                    // Run `f` on the first thread in the thread pool.
                    move |pool| pool.install(f),
                )
                .unwrap()
        })
    })
}

fn run_in_thread_with_globals<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    let builder = std::thread::Builder::new().name("sulk".into()).stack_size(init_stack_size());
    std::thread::scope(|s| {
        let r = builder.spawn_scoped(s, move || SessionGlobals::new().set(f)).unwrap().join();
        match r {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    })
}

#[cfg(feature = "nightly")]
use std::panic::update_hook;

#[cfg(not(feature = "nightly"))]
fn update_hook<F>(hook_fn: F)
where
    F: Fn(&(dyn Fn(&PanicInfo<'_>) + Send + Sync + 'static), &PanicInfo<'_>)
        + Sync
        + Send
        + 'static,
{
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| hook_fn(&default_hook, info)));
}
