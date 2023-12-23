// use rsolc_data_structures::sync::{Lock, Lrc};

/// Per-session global variables: this struct is stored in thread-local storage
/// in such a way that it is accessible without any kind of handle to all
/// threads within the compilation session, but is not accessible outside the
/// session.
pub struct SessionGlobals {
    pub(crate) symbol_interner: crate::symbol::Interner,
    // /// A reference to the source map in the `Session`. It's an `Option`
    // /// because it can't be initialized until `Session` is created, which
    // /// happens after `SessionGlobals`. `set_source_map` does the
    // /// initialization.
    // ///
    // /// This field should only be used in places where the `Session` is truly
    // /// not available, such as `<Span as Debug>::fmt`.
    // source_map: Lock<Option<Lrc<SourceMap>>>,
}

impl Default for SessionGlobals {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionGlobals {
    pub fn new() -> Self {
        Self {
            symbol_interner: crate::symbol::Interner::fresh(),
            // source_map: Lock::new(None),
        }
    }
}

#[inline]
pub fn create_session_globals_then<R>(f: impl FnOnce() -> R) -> R {
    assert!(
        !SESSION_GLOBALS.is_set(),
        "SESSION_GLOBALS should never be overwritten! \
         Use another thread if you need another SessionGlobals"
    );
    let session_globals = SessionGlobals::new();
    SESSION_GLOBALS.set(&session_globals, f)
}

#[inline]
pub fn set_session_globals_then<R>(session_globals: &SessionGlobals, f: impl FnOnce() -> R) -> R {
    assert!(
        !SESSION_GLOBALS.is_set(),
        "SESSION_GLOBALS should never be overwritten! \
         Use another thread if you need another SessionGlobals"
    );
    SESSION_GLOBALS.set(session_globals, f)
}

#[inline]
pub fn create_default_session_if_not_set_then<R, F>(f: F) -> R
where
    F: FnOnce(&SessionGlobals) -> R,
{
    if !SESSION_GLOBALS.is_set() {
        let session_globals = SessionGlobals::new();
        SESSION_GLOBALS.set(&session_globals, || SESSION_GLOBALS.with(f))
    } else {
        SESSION_GLOBALS.with(f)
    }
}

#[inline]
pub fn with_session_globals<R, F>(f: F) -> R
where
    F: FnOnce(&SessionGlobals) -> R,
{
    SESSION_GLOBALS.with(f)
}

// If this ever becomes non thread-local, `decode_syntax_context`
// and `decode_expn_id` will need to be updated to handle concurrent
// deserialization.
scoped_tls::scoped_thread_local!(static SESSION_GLOBALS: SessionGlobals);
