use crate::SourceMap;
use sulk_data_structures::sync::{Lock, Lrc};

scoped_tls::scoped_thread_local!(static SESSION_GLOBALS: SessionGlobals);

/// Per-session global variables: this struct is stored in thread-local storage
/// in such a way that it is accessible without any kind of handle to all
/// threads within the compilation session, but is not accessible outside the
/// session.
pub struct SessionGlobals {
    pub(crate) symbol_interner: crate::symbol::Interner,
    /// A reference to the source map in the `Session`. It's an `Option`
    /// because it can't be initialized until `Session` is created, which
    /// happens after `SessionGlobals`. `set_source_map` does the
    /// initialization.
    ///
    /// This field should only be used in places where the `Session` is truly
    /// not available, such as `<Span as Debug>::fmt`.
    pub(crate) source_map: Lock<Option<Lrc<SourceMap>>>,
}

impl Default for SessionGlobals {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionGlobals {
    /// Creates a new session globals object.
    pub fn new() -> Self {
        Self { symbol_interner: crate::symbol::Interner::fresh(), source_map: Lock::new(None) }
    }

    /// Sets this instance as the global instance for the duration of the closure.
    #[inline]
    pub fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        if SESSION_GLOBALS.is_set() {
            panic_overwrite();
        }
        SESSION_GLOBALS.set(self, f)
    }

    /// Calls the given closure with the current session globals.
    ///
    /// # Panics
    ///
    /// Panics if `set` has not previously been called.
    #[inline]
    pub fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        SESSION_GLOBALS.with(f)
    }

    /// Calls the given closure with the current session globals if they have been set, otherwise
    /// creates a new instance, sets it, and calls the closure with it.
    #[inline]
    pub fn with_or_default<R>(f: impl FnOnce(&Self) -> R) -> R {
        if Self::is_set() {
            Self::with(f)
        } else {
            Self::new().set(|| Self::with(f))
        }
    }

    /// Returns `true` if the session globals have been set.
    #[inline]
    pub fn is_set() -> bool {
        SESSION_GLOBALS.is_set()
    }
}

#[cold]
#[inline(never)]
const fn panic_overwrite() -> ! {
    panic!(
        "SESSION_GLOBALS should never be overwritten! \
         Use another thread if you need another SessionGlobals"
    );
}
