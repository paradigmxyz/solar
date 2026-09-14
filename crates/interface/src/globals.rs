use crate::SourceMap;
use std::{cell::RefCell, sync::Arc};

scoped_tls::scoped_thread_local!(static SESSION_GLOBALS: SessionGlobals);

thread_local! {
    static POOL_GLOBALS: RefCell<Option<Arc<SessionGlobals>>> = const { RefCell::new(None) };
}

/// Per-session global variables.
///
/// This struct is stored in thread-local storage in such a way that it is accessible without any
/// kind of handle to all threads within the compilation session, but is not accessible outside the
/// session.
///
/// These should only be used when `Session` is truly not available, such as `Symbol::intern` and
/// `<Span as Debug>::fmt`.
pub(crate) struct SessionGlobals {
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
    pub(crate) fn new(source_map: Arc<SourceMap>) -> Self {
        Self { symbol_interner: crate::symbol::Interner::fresh(), source_map }
    }

    /// Sets this instance as the global instance for the duration of the closure.
    pub(crate) fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        self.check_overwrite();
        SESSION_GLOBALS.set(self, f)
    }

    fn check_overwrite(&self) {
        Self::try_with(|prev| {
            if let Some(prev) = prev
                && !prev.maybe_eq(self)
            {
                overwrite_log();
            }
        });
    }

    /// Calls the given closure with the current session globals.
    ///
    /// # Panics
    ///
    /// Panics if `set` has not previously been called.
    #[inline]
    #[track_caller]
    pub(crate) fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        debug_assert!(
            Self::is_set(),
            "cannot access a scoped thread local variable without calling `set` first; \
             did you forget to call `Session::enter`?"
        );
        if SESSION_GLOBALS.is_set() {
            SESSION_GLOBALS.with(f)
        } else {
            POOL_GLOBALS
                .with_borrow(|globals| f(globals.as_ref().expect("session globals not set")))
        }
    }

    /// Calls the given closure with the current session globals if they have been set, otherwise
    /// creates a new instance, sets it, and calls the closure with it.
    #[inline]
    #[track_caller]
    pub(crate) fn with_or_default<R>(f: impl FnOnce(&Self) -> R) -> R {
        if Self::is_set() { Self::with(f) } else { Self::default().set(|| Self::with(f)) }
    }

    /// Returns `true` if the session globals have been set.
    #[inline]
    pub(crate) fn is_set() -> bool {
        SESSION_GLOBALS.is_set() || Self::in_shared_pool()
    }

    pub(crate) fn try_with<R>(f: impl FnOnce(Option<&Self>) -> R) -> R {
        if Self::is_set() { Self::with(|g| f(Some(g))) } else { f(None) }
    }

    pub(crate) fn in_shared_pool() -> bool {
        POOL_GLOBALS.with_borrow(Option::is_some)
    }

    pub(crate) fn is_pool_session(&self) -> bool {
        POOL_GLOBALS.with_borrow(|globals| globals.as_ref().is_some_and(|g| g.maybe_eq(self)))
    }

    pub(crate) fn set_pool_globals(globals: Option<Arc<Self>>) {
        POOL_GLOBALS.set(globals);
    }

    pub(crate) fn maybe_eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

#[inline(never)]
#[cold]
fn overwrite_log() {
    debug!(
        "overwriting SESSION_GLOBALS; \
         this might be due to manual incorrect usage of `SessionGlobals`, \
         or entering multiple different nested `Session`s, which may cause unexpected behavior"
    );
}
