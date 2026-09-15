use crate::SourceMap;
use std::{cell::RefCell, sync::Arc};

thread_local! {
    static SESSION_GLOBALS: RefCell<Option<Arc<SessionGlobals>>> = const { RefCell::new(None) };
}

struct RestoreGlobals(Option<Arc<SessionGlobals>>);

impl Drop for RestoreGlobals {
    fn drop(&mut self) {
        SessionGlobals::replace(self.0.take());
    }
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
    pub(crate) fn set<R>(self: &Arc<Self>, f: impl FnOnce() -> R) -> R {
        self.check_overwrite();
        let _restore = RestoreGlobals(Self::replace(Some(self.clone())));
        f()
    }

    pub(crate) fn replace(globals: Option<Arc<Self>>) -> Option<Arc<Self>> {
        SESSION_GLOBALS.replace(globals)
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

    /// Calls the given closure with the current session globals. The closure must not replace them.
    ///
    /// # Panics
    ///
    /// Panics if `set` has not previously been called.
    #[inline]
    #[track_caller]
    pub(crate) fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        SESSION_GLOBALS.with_borrow(|globals| {
            f(globals.as_deref().expect("session globals not set; call Session::enter first"))
        })
    }

    /// Calls the given closure with the current session globals if they have been set, otherwise
    /// creates a new instance, sets it, and calls the closure with it.
    #[inline]
    #[track_caller]
    pub(crate) fn with_or_default<R>(f: impl FnOnce(&Self) -> R) -> R {
        if let Some(globals) = SESSION_GLOBALS.with_borrow(Clone::clone) {
            f(&globals)
        } else {
            let globals = Arc::<Self>::default();
            globals.set(|| f(&globals))
        }
    }

    pub(crate) fn try_with<R>(f: impl FnOnce(Option<&Self>) -> R) -> R {
        let globals = SESSION_GLOBALS.with_borrow(Clone::clone);
        f(globals.as_deref())
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
