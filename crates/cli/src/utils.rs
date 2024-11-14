//! Utility functions used by the Solar CLI.

use solar_interface::diagnostics::DiagCtxt;

#[cfg(all(feature = "jemalloc", unix))]
use tikv_jemallocator as _;

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
        pub(super) type WrappedAllocator = tracing_tracy::client::ProfiledAllocator<AllocatorInner>;
        pub(super) const fn new_wrapped_allocator() -> WrappedAllocator {
            Allocator::new(AllocatorInner {}, 100)
        }
    } else {
        pub(super) type WrappedAllocator = AllocatorInner;
        pub(super) const fn new_wrapped_allocator() -> WrappedAllocator {
            AllocatorInner {}
        }
    }
}

/// The global allocator used by the compiler.
pub type Allocator = WrappedAllocator;

/// Create a new instance of the global allocator.
pub const fn new_allocator() -> Allocator {
    new_wrapped_allocator()
}

/// Initialize the tracing logger.
pub fn init_logger() -> impl Sized {
    match try_init_logger() {
        Ok(guard) => guard,
        Err(e) => DiagCtxt::new_early().fatal(e).emit(),
    }
}

fn try_init_logger() -> Result<impl Sized, String> {
    use tracing_subscriber::prelude::*;

    let (profile_layer, guard) = match std::env::var("SOLAR_PROFILE").as_deref() {
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
        Ok(s) => return Err(format!("unknown profiler '{s}'; valid values: 'chrome', 'tracy'")),
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
    struct Config(tracing_subscriber::fmt::format::DefaultFields);
    impl tracing_tracy::Config for Config {
        type Formatter = tracing_subscriber::fmt::format::DefaultFields;
        fn formatter(&self) -> &Self::Formatter {
            &self.0
        }
        fn format_fields_in_zone_name(&self) -> bool {
            false
        }
    }

    tracing_tracy::client::register_demangler!();

    tracing_tracy::TracyLayer::new(Config(Default::default()))
}

#[cfg(not(feature = "tracy"))]
fn tracy_layer() -> tracing_subscriber::layer::Identity {
    tracing_subscriber::layer::Identity::new()
}

#[cfg(feature = "tracing-chrome")]
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
