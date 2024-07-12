use std::mem::ManuallyDrop;

/// Returns a structure that calls `f` when dropped.
#[inline(always)]
pub fn defer<F: FnOnce()>(f: F) -> OnDrop<(), impl FnOnce(())> {
    OnDrop::new((), move |()| f())
}

/// Runs `F` on `T` when the instance is dropped.
pub struct OnDrop<T, F: FnOnce(T)>(ManuallyDrop<(T, Option<F>)>);

impl<T, F: FnOnce(T)> OnDrop<T, F> {
    /// Creates a new `OnDrop` instance.
    #[inline(always)]
    pub fn new(value: T, f: F) -> Self {
        Self(ManuallyDrop::new((value, Some(f))))
    }

    /// Consumes the instance and returns the inner value.
    #[inline(always)]
    pub fn into_inner(mut self) -> T {
        unsafe {
            std::ptr::drop_in_place(&mut self.0 .1);
            std::ptr::read(&self.0 .0)
        }
    }

    /// Consumes the instance without running `F` on the inner value.
    #[inline(always)]
    pub fn disable(self) {
        let _ = self.into_inner();
    }
}

impl<T, F: FnOnce(T)> std::ops::Deref for OnDrop<T, F> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0 .0
    }
}

impl<T, F: FnOnce(T)> std::ops::DerefMut for OnDrop<T, F> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0 .0
    }
}

impl<T, F: FnOnce(T)> Drop for OnDrop<T, F> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            if let Some(f) = self.0 .1.take() {
                f(std::ptr::read(&self.0 .0));
            } else {
                ManuallyDrop::drop(&mut self.0);
            }
        }
    }
}
