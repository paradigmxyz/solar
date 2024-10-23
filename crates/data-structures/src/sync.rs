pub use parking_lot::{
    MappedMutexGuard as MappedLockGuard, MappedRwLockReadGuard as MappedReadGuard,
    MappedRwLockWriteGuard as MappedWriteGuard, Mutex as Lock, RwLock,
    RwLockReadGuard as ReadGuard, RwLockWriteGuard as WriteGuard,
};

/// Executes the given expressions in parallel.
#[macro_export]
macro_rules! parallel {
    ($sess:expr, $first:expr $(, $blocks:expr)+ $(,)?) => {
        $sess.scope(|scope| {
            $(
                scope.spawn(|_| $blocks);
            )+
            $first;
        })
    };
}

/// A dynamic fork-join scope.
///
/// See [`rayon::scope`] for more details.
pub struct Scope<'r, 'scope>(Option<&'r rayon::Scope<'scope>>);

impl<'r, 'scope> Scope<'r, 'scope> {
    /// Creates a new scope.
    #[inline]
    fn new(scope: Option<&'r rayon::Scope<'scope>>) -> Self {
        Self(scope)
    }

    /// Spawns a job into the fork-join scope `self`.
    #[inline]
    pub fn spawn<BODY>(&self, body: BODY)
    where
        BODY: FnOnce(Scope<'_, 'scope>) + Send + 'scope,
    {
        match self.0 {
            Some(scope) => scope.spawn(|scope| body(Scope::new(Some(scope)))),
            None => body(Scope::new(None)),
        }
    }
}

/// Creates a new fork-join scope.
///
/// See [`rayon::scope`] for more details.
#[inline]
pub fn scope<'scope, OP, R>(enabled: bool, op: OP) -> R
where
    OP: FnOnce(Scope<'_, 'scope>) -> R + Send,
    R: Send,
{
    if enabled {
        rayon::scope(|scope| op(Scope::new(Some(scope))))
    } else {
        op(Scope::new(None))
    }
}
