#![allow(deprecated)] // PanicInfo -> PanicInfoHook since 1.82

use std::panic::PanicInfo;
use sulk_interface::{
    diagnostics::{DiagCtxt, ExplicitBug, FatalAbort},
    SessionGlobals,
};

const BUG_REPORT_URL: &str =
    "https://github.com/paradigmxyz/sulk/issues/new/?labels=C-bug%2C+I-ICE&template=ice.yml";

// We use jemalloc for performance reasons.
// Except in tests, where we spawn a ton of processes and jemalloc has a higher startup cost.
cfg_if::cfg_if! {
    if #[cfg(all(feature = "jemalloc", unix, not(debug_assertions)))] {
        type AllocatorInner = tikv_jemallocator::Jemalloc;
    } else {
        type AllocatorInner = std::alloc::System;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "tracy-allocator")] {
        pub(super) type Allocator = tracing_tracy::client::ProfiledAllocator<AllocatorInner>;
        pub(super) const fn new_allocator() -> Allocator {
            Allocator::new(AllocatorInner {}, 100)
        }
    } else {
        pub(super) type Allocator = AllocatorInner;
        pub(super) const fn new_allocator() -> Allocator {
            AllocatorInner {}
        }
    }
}

fn early_dcx() -> DiagCtxt {
    DiagCtxt::with_tty_emitter(None).set_flags(|flags| flags.track_diagnostics = false)
}

pub(crate) fn init_logger() -> impl Sized {
    match try_init_logger() {
        Ok(guard) => guard,
        Err(e) => early_dcx().fatal(e).emit(),
    }
}

fn try_init_logger() -> std::result::Result<impl Sized, String> {
    use tracing_subscriber::prelude::*;

    let (profile_layer, guard) = match std::env::var("SULK_PROFILE").as_deref() {
        Ok("chrome") => {
            if !cfg!(feature = "tracing-chrome") {
                return Err("chrome profiler support is not compiled in".to_string());
            }
            let (layer, guard) = chrome_layer();
            (Some(layer.boxed()), Some(guard))
        }
        Ok("tracy") => {
            if !cfg!(feature = "tracy") {
                return Err("tracy profiler support is not compiled in".to_string());
            }
            (Some(tracy_layer().boxed()), Default::default())
        }
        Ok(s) => return Err(format!("unknown profiler '{s}'")),
        Err(_) => Default::default(),
    };
    tracing_subscriber::Registry::default()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(profile_layer)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .map(|()| guard)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "tracy")]
fn tracy_layer() -> tracing_tracy::TracyLayer<impl tracing_tracy::Config> {
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

    // Disable demangling as it shows up a lot in allocations.
    #[cfg(feature = "tracy-allocator")]
    #[no_mangle]
    unsafe extern "C" fn ___tracy_demangle(
        _mangled: *const std::ffi::c_char,
    ) -> *const std::ffi::c_char {
        std::ptr::null()
    }

    #[cfg(not(feature = "tracy-allocator"))]
    tracing_tracy::client::register_demangler!();

    tracing_tracy::TracyLayer::new(Config(Default::default(), false))
}

#[cfg(not(feature = "tracy"))]
fn tracy_layer() -> tracing_subscriber::layer::Identity {
    tracing_subscriber::layer::Identity::new()
}

#[cfg(feature = "tracing-chrome")]
#[allow(clippy::disallowed_methods)]
fn chrome_layer<S>() -> (tracing_chrome::ChromeLayer<S>, tracing_chrome::FlushGuard)
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    tracing_chrome::ChromeLayerBuilder::new().include_args(true).build()
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
        if info.payload().is::<FatalAbort>() {
            std::process::exit(1);
        }

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
