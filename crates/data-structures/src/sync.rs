use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// Maps `f` over `items` and returns the results in item order, or the first error in item order.
///
/// When `enabled`, idle rayon workers help the calling thread: it claims items from a shared
/// counter and runs them itself, and spawns helpers that claim items the same way. A helper that
/// starts late finds nothing left and returns. Unlike a rayon join or parallel iterator, the
/// caller never waits inside rayon for helpers to finish items, so it does not pick up an
/// unrelated queued job and run it before its own continuation.
///
/// A serial map stops at the first error; a parallel one stops claiming items after one.
pub fn par_try_map<T, R, E, F>(enabled: bool, items: Vec<T>, f: F) -> Result<Vec<R>, E>
where
    T: Send,
    R: Send,
    E: Send,
    F: Fn(T) -> Result<R, E> + Sync,
{
    let len = items.len();
    if !enabled || len < 2 {
        let mut results = Vec::with_capacity(len);
        for item in items {
            results.push(f(item)?);
        }
        return Ok(results);
    }

    let inputs = items.into_iter().map(|item| Mutex::new(Some(item))).collect::<Vec<_>>();
    let outputs = (0..len).map(|_| Mutex::new(None)).collect::<Vec<_>>();
    let next = AtomicUsize::new(0);
    let active = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    // Leaves `active` and, on a panic, stops the other workers, so the caller's wait ends and
    // the scope can rethrow the panic.
    struct Active<'a> {
        active: &'a AtomicUsize,
        failed: &'a AtomicBool,
    }
    impl Drop for Active<'_> {
        fn drop(&mut self) {
            if std::thread::panicking() {
                self.failed.store(true, Ordering::Relaxed);
            }
            self.active.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let work = || {
        active.fetch_add(1, Ordering::AcqRel);
        let _active = Active { active: &active, failed: &failed };
        while !failed.load(Ordering::Relaxed) {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(input) = inputs.get(index) else { break };
            let result = f(input.lock().take().expect("item claimed once"));
            if result.is_err() {
                failed.store(true, Ordering::Relaxed);
            }
            *outputs[index].lock() = Some(result);
        }
    };
    rayon::scope(|scope| {
        for _ in 1..len.min(rayon::current_num_threads()) {
            scope.spawn(|_| work());
        }
        work();
        // A helper that claimed an item has counted itself in `active` first.
        while active.load(Ordering::Acquire) != 0 {
            std::thread::yield_now();
        }
    });

    outputs
        .into_iter()
        .map(|output| output.into_inner().expect("items before the first error ran"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_try_map_keeps_item_order() {
        let items = (0..200u32).collect::<Vec<_>>();
        let mapped = par_try_map(true, items.clone(), |item| Ok::<_, ()>(item * 2));
        assert_eq!(mapped, Ok(items.iter().map(|item| item * 2).collect()));
    }

    #[test]
    fn par_try_map_returns_first_error_in_item_order() {
        for enabled in [false, true] {
            let items = (0..200u32).collect::<Vec<_>>();
            let mapped = par_try_map(enabled, items, |item| {
                if item % 50 == 7 {
                    std::thread::sleep(std::time::Duration::from_millis(u64::from(200 - item)));
                    Err(item)
                } else {
                    Ok(item)
                }
            });
            assert_eq!(mapped, Err(7));
        }
    }
}
