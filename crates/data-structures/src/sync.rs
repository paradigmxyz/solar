pub use parking_lot::{
    MappedMutexGuard, MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, RwLock,
    RwLockReadGuard, RwLockWriteGuard,
};

/// Executes the given expressions in parallel.
#[macro_export]
macro_rules! parallel {
    ($sess:expr, $($blocks:expr),+ $(,)?) => {
        $crate::sync::par_fns($sess.is_parallel(), &mut [$(&mut || { $blocks }),+])
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
    if enabled { rayon::scope(|scope| op(Scope::new(Some(scope)))) } else { op(Scope::new(None)) }
}

/// Runs the closures in parallel, where the current thread will run the first closure.
///
/// Support function for the [`parallel!`] macro.
pub fn par_fns(parallel: bool, funcs: &mut [&mut (dyn FnMut() + Send)]) {
    if parallel {
        rayon::scope(|s| {
            let Some((first, rest)) = funcs.split_first_mut() else {
                return;
            };
            for f in rest.iter_mut() {
                s.spawn(move |_| f());
            }
            first();
        });
    } else {
        // Run the rest first, then the first, to match the parallel execution order where
        // the rest are spawned before the first runs on the current thread.
        if let Some((first, rest)) = funcs.split_first_mut() {
            for f in rest.iter_mut() {
                f();
            }
            first();
        }
    }
}
