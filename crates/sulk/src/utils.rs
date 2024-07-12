use std::panic::PanicInfo;
use sulk_interface::{
    diagnostics::{DiagCtxt, ExplicitBug},
    SessionGlobals,
};

const BUG_REPORT_URL: &str =
    "https://github.com/paradigmxyz/sulk/issues/new/?labels=C-bug%2C+I-ICE&template=ice.yml";

fn early_dcx() -> DiagCtxt {
    DiagCtxt::with_tty_emitter(None)
}

#[must_use]
pub(crate) fn init_logger() -> impl Sized {
    use tracing_subscriber::prelude::*;

    let (chrome_layer, guard) = chrome_layer();
    let registry = tracing_subscriber::Registry::default()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracy_layer())
        .with(chrome_layer)
        .with(tracing_subscriber::fmt::layer());
    match registry.try_init() {
        Ok(()) => guard,
        Err(e) => {
            early_dcx().err(e.to_string()).emit();
            Default::default()
        }
    }
}

#[cfg(feature = "tracy")]
fn tracy_layer() -> Option<tracing_tracy::TracyLayer<impl tracing_tracy::Config>> {
    struct Config(tracing_subscriber::fmt::format::DefaultFields, bool);
    impl tracing_tracy::Config for Config {
        type Formatter = tracing_subscriber::fmt::format::DefaultFields;
        fn formatter(&self) -> &Self::Formatter {
            &self.0
        }
        fn format_fields_in_zone_name(&self) -> bool {
            self.1
        }
    }

    if env_to_bool(std::env::var_os("SULK_PROFILE").as_deref()) {
        let capture_args = env_to_bool(std::env::var_os("SULK_PROFILE_CAPTURE_ARGS").as_deref());
        Some(tracing_tracy::TracyLayer::new(Config(Default::default(), capture_args)))
    } else {
        None
    }
}

#[cfg(not(feature = "tracy"))]
fn tracy_layer() -> tracing_subscriber::layer::Identity {
    tracing_subscriber::layer::Identity::new()
}

#[cfg(feature = "tracing-chrome")]
#[allow(clippy::disallowed_methods)]
fn chrome_layer<S>() -> (Option<tracing_chrome::ChromeLayer<S>>, Option<tracing_chrome::FlushGuard>)
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    if env_to_bool(std::env::var_os("SULK_PROFILE").as_deref()) {
        let capture_args = env_to_bool(std::env::var_os("SULK_PROFILE_CAPTURE_ARGS").as_deref());
        let (layer, guard) =
            tracing_chrome::ChromeLayerBuilder::new().include_args(capture_args).build();
        (Some(layer), Some(guard))
    } else {
        (None, None)
    }
}

#[cfg(not(feature = "tracing-chrome"))]
fn chrome_layer() -> (tracing_subscriber::layer::Identity, ()) {
    (tracing_subscriber::layer::Identity::new(), ())
}

#[allow(dead_code)]
pub(crate) fn env_to_bool(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| value == "1" || value == "true")
}

pub(crate) fn install_panic_hook() {
    update_hook(|default_hook, info| {
        // Lock stderr to prevent interleaving of concurrent panics.
        let _guard = std::io::stderr().lock();

        if std::env::var_os("RUST_BACKTRACE").is_none() {
            std::env::set_var("RUST_BACKTRACE", "1");
        }

        default_hook(info);

        // Separate the output with an empty line.
        eprintln!();

        panic_hook(info);
    });
}

fn panic_hook(info: &PanicInfo<'_>) {
    let dcx = early_dcx().set_flags(|f| f.track_diagnostics = false);

    // An explicit `bug()` call has already printed what it wants to print.
    if !info.payload().is::<ExplicitBug>() {
        dcx.err("the compiler unexpectedly panicked; this is a bug.").emit();
    }

    dcx.note(format!("we would appreciate a bug report: {BUG_REPORT_URL}")).emit();
}

pub(crate) fn run_in_thread_pool_with_globals<R: Send>(
    threads: usize,
    f: impl FnOnce(usize) -> R + Send,
) -> R {
    let mut builder =
        rayon::ThreadPoolBuilder::new().thread_name(|i| format!("sulk-{i}")).num_threads(threads);
    // We still want to use a rayon thread pool with 1 thread so that `ParallelIterator` don't
    // install their own thread pool.
    if threads == 1 {
        builder = builder.use_current_thread();
    }

    // We create the session globals on the main thread, then create the thread pool. Upon creation,
    // each worker thread created gets a copy of the session globals in TLS. This is possible
    // because `SessionGlobals` impls `Send`.
    SessionGlobals::new().set(|| {
        SessionGlobals::with(|session_globals| {
            builder
                .build_scoped(
                    // Initialize each new worker thread when created.
                    move |thread| session_globals.set(|| thread.run()),
                    // Run `f` on the first thread in the thread pool.
                    move |pool| pool.install(|| f(pool.current_num_threads())),
                )
                .unwrap()
        })
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
