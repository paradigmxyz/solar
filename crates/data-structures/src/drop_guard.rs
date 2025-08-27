use std::mem::ManuallyDrop;

/// Returns a structure that calls `f` when dropped.
#[inline]
pub const fn defer<F: FnOnce()>(f: F) -> DropGuard<(), impl FnOnce(())> {
    DropGuard::new((), move |()| f())
}

/// Runs `F` on `T` when the instance is dropped.
///
/// Equivalent of `std::mem::DropGuard`.
#[must_use]
pub struct DropGuard<T, F: FnOnce(T)> {
    inner: ManuallyDrop<T>,
    f: ManuallyDrop<F>,
}

impl<T, F: FnOnce(T)> DropGuard<T, F> {
    /// Creates a new `OnDrop` instance.
    #[inline]
    pub const fn new(value: T, f: F) -> Self {
        Self { inner: ManuallyDrop::new(value), f: ManuallyDrop::new(f) }
    }

    /// Consumes the `DropGuard`, returning the wrapped value.
    ///
    /// This will not execute the closure. This is implemented as an associated
    /// function to prevent any potential conflicts with any other methods called
    /// `into_inner` from the `Deref` and `DerefMut` impls.
    ///
    /// It is typically preferred to call this function instead of `mem::forget`
    /// because it will return the stored value and drop variables captured
    /// by the closure instead of leaking their owned resources.
    #[inline]
    #[must_use]
    pub fn into_inner(guard: Self) -> T {
        // First we ensure that dropping the guard will not trigger
        // its destructor
        let mut guard = ManuallyDrop::new(guard);

        // Next we manually read the stored value from the guard.
        //
        // SAFETY: this is safe because we've taken ownership of the guard.
        let value = unsafe { ManuallyDrop::take(&mut guard.inner) };

        // Finally we drop the stored closure. We do this *after* having read
        // the value, so that even if the closure's `drop` function panics,
        // unwinding still tries to drop the value.
        //
        // SAFETY: this is safe because we've taken ownership of the guard.
        unsafe { ManuallyDrop::drop(&mut guard.f) };
        value
    }
}

impl<T, F: FnOnce(T)> std::ops::Deref for DropGuard<T, F> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, F: FnOnce(T)> std::ops::DerefMut for DropGuard<T, F> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T, F: FnOnce(T)> Drop for DropGuard<T, F> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: `DropGuard` is in the process of being dropped.
        let inner = unsafe { ManuallyDrop::take(&mut self.inner) };

        // SAFETY: `DropGuard` is in the process of being dropped.
        let f = unsafe { ManuallyDrop::take(&mut self.f) };

        f(inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DropCheck<'a>(&'a mut usize);
    impl Drop for DropCheck<'_> {
        fn drop(&mut self) {
            *self.0 += 1;
        }
    }

    #[test]
    fn test_defer() {
        let mut x = 0usize;
        let defer = defer(|| x += 1);
        drop(defer);
        assert_eq!(x, 1);
    }

    #[test]
    fn drop_guard() {
        let mut x = 0usize;
        let guard = DropGuard::new(&mut x, |x| *x += 1);
        assert_eq!(**guard, 0);
        drop(guard);
        assert_eq!(x, 1);
    }

    #[test]
    fn normal() {
        let mut dropped = 0;
        let mut closure_called = 0;
        let mut closure_dropped = 0;
        let closure_drop_check = DropCheck(&mut closure_dropped);
        let guard = DropGuard::new(DropCheck(&mut dropped), |_s| {
            drop(closure_drop_check);
            closure_called += 1;
        });
        drop(guard);
        assert_eq!(dropped, 1);
        assert_eq!(closure_called, 1);
        assert_eq!(closure_dropped, 1);
    }

    #[test]
    fn disable() {
        let mut dropped = 0;
        let mut closure_called = 0;
        let mut closure_dropped = 0;
        let closure_drop_check = DropCheck(&mut closure_dropped);
        let guard = DropGuard::new(DropCheck(&mut dropped), |_s| {
            drop(closure_drop_check);
            closure_called += 1;
        });
        let value = DropGuard::into_inner(guard);
        assert_eq!(*value.0, 0);
        assert_eq!(closure_called, 0);
        assert_eq!(closure_dropped, 1);

        drop(value);
        assert_eq!(dropped, 1);
    }
}
