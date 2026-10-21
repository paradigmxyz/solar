use rayon::prelude::*;

pub use parking_lot::{
    MappedMutexGuard, MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, RwLock,
    RwLockReadGuard, RwLockWriteGuard,
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
    if enabled { rayon::scope(|scope| op(Scope::new(Some(scope)))) } else { op(Scope::new(None)) }
}

/// Maps `f` over `items`, one rayon job per item when `enabled`, and returns the results in
/// item order, or the first error in item order.
///
/// A serial map stops at the first error; a parallel one still runs every item.
#[inline]
pub fn par_try_map<T, R, E, F>(enabled: bool, items: &[T], f: F) -> Result<Vec<R>, E>
where
    T: Sync,
    R: Send,
    E: Send,
    F: Fn(&T) -> Result<R, E> + Sync + Send,
{
    if enabled {
        items.par_iter().with_max_len(1).map(f).collect::<Vec<_>>().into_iter().collect()
    } else {
        items.iter().map(f).collect()
    }
}
