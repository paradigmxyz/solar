use crate::SourceMap;
use std::sync::Arc;

scoped_tls::scoped_thread_local!(static SESSION_GLOBALS: SessionGlobals);

/// Per-session global variables.
///
/// This struct is stored in thread-local storage in such a way that it is accessible without any
/// kind of handle to all threads within the compilation session, but is not accessible outside the
/// session.
///
/// These should only be used when `Session` is truly not available, such as `Symbol::intern` and
/// `<Span as Debug>::fmt`.
pub struct SessionGlobals {
    pub(crate) symbol_interner: crate::symbol::Interner,
    pub(crate) source_map: Arc<SourceMap>,
}

impl Default for SessionGlobals {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl SessionGlobals {
    /// Creates a new session globals object.
    pub fn new(source_map: Arc<SourceMap>) -> Self {
        Self { symbol_interner: crate::symbol::Interner::fresh(), source_map }
    }

    /// Sets this instance as the global instance for the duration of the closure.
    #[inline]
    #[track_caller]
    pub fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        if cfg!(debug_assertions) && SESSION_GLOBALS.is_set() {
            check_overwrite(self);
        }
        SESSION_GLOBALS.set(self, f)
    }

    /// Insert `source_map` into the session globals for the duration of the closure's execution.
    #[deprecated(note = "does nothing")]
    #[track_caller]
    pub fn with_source_map<R>(_source_map: Arc<SourceMap>, f: impl FnOnce() -> R) -> R {
        f()
    }

    /// Calls the given closure with the current session globals.
    ///
    /// # Panics
    ///
    /// Panics if `set` has not previously been called.
    #[inline]
    #[track_caller]
    pub fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        #[cfg(debug_assertions)]
        if !SESSION_GLOBALS.is_set() {
            let msg = if rayon::current_thread_index().is_some() {
                "cannot access a scoped thread local variable without calling `set` first;\n\
                 did you forget to call `Session::enter_parallel`?"
            } else {
                "cannot access a scoped thread local variable without calling `set` first;\n\
                 did you forget to call `Session::enter`, or `Session::enter_parallel` \
                 if using Rayon?"
            };
            panic!("{msg}");
        }
        SESSION_GLOBALS.with(f)
    }

    /// Calls the given closure with the current session globals if they have been set, otherwise
    /// creates a new instance, sets it, and calls the closure with it.
    #[inline]
    #[track_caller]
    pub fn with_or_default<R>(f: impl FnOnce(&Self) -> R) -> R {
        if Self::is_set() { Self::with(f) } else { Self::default().set(|| Self::with(f)) }
    }

    /// Returns `true` if the session globals have been set.
    #[inline]
    pub fn is_set() -> bool {
        SESSION_GLOBALS.is_set()
    }

    fn maybe_eq(&self, other: &Self) -> bool {
        // Extra check for test usage of `enter`:
        // we allow replacing empty source maps with eachother.
        std::ptr::eq(self, other) || (self.is_default() && other.is_default())
    }

    fn is_default(&self) -> bool {
        self.source_map.is_empty()
    }
}

#[cold]
#[inline(never)]
#[cfg_attr(debug_assertions, track_caller)]
fn check_overwrite(new: &SessionGlobals) {
    SessionGlobals::with(|old| {
        if !old.maybe_eq(new) {
            panic!(
                "SESSION_GLOBALS should never be overwritten!\n\
                 This is likely either due to manual incorrect usage of `SessionGlobals`, \
                 or entering multiple nested `Session`s, which is not supported"
            );
        }
    })
}
