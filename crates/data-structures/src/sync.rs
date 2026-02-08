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
