mod lock;
pub use lock::{Lock, LockGuard, Mode};

mod mode;
pub use mode::{is_dyn_thread_safe, might_be_dyn_thread_safe, set_dyn_thread_safe_mode};

#[cfg(not(feature = "parallel"))]
pub use no_sync::*;
#[cfg(feature = "parallel")]
pub use sync::*;

#[cfg(not(feature = "parallel"))]
mod no_sync {
    use std::{cell::Cell, ops::Add, sync::atomic::Ordering};

    /// This is a single threaded variant of `AtomicU64`, `AtomicUsize`, etc.
    /// It has explicit ordering arguments and is only intended for use with
    /// the native atomic types.
    /// You should use this type through the `AtomicU64`, `AtomicUsize`, etc, type aliases
    /// as it's not intended to be used separately.
    #[derive(Debug, Default)]
    pub struct Atomic<T: Copy>(Cell<T>);

    impl<T: Copy> Atomic<T> {
        #[inline]
        pub fn new(v: T) -> Self {
            Atomic(Cell::new(v))
        }

        #[inline]
        pub fn into_inner(self) -> T {
            self.0.into_inner()
        }

        #[inline]
        pub fn load(&self, _: Ordering) -> T {
            self.0.get()
        }

        #[inline]
        pub fn store(&self, val: T, _: Ordering) {
            self.0.set(val)
        }

        #[inline]
        pub fn swap(&self, val: T, _: Ordering) -> T {
            self.0.replace(val)
        }
    }

    impl Atomic<bool> {
        pub fn fetch_or(&self, val: bool, _: Ordering) -> bool {
            let old = self.0.get();
            self.0.set(val | old);
            old
        }

        pub fn fetch_and(&self, val: bool, _: Ordering) -> bool {
            let old = self.0.get();
            self.0.set(val & old);
            old
        }
    }

    impl<T: Copy + PartialEq> Atomic<T> {
        #[inline]
        pub fn compare_exchange(
            &self,
            current: T,
            new: T,
            _: Ordering,
            _: Ordering,
        ) -> Result<T, T> {
            let read = self.0.get();
            if read == current {
                self.0.set(new);
                Ok(read)
            } else {
                Err(read)
            }
        }
    }

    impl<T: Add<Output = T> + Copy> Atomic<T> {
        #[inline]
        pub fn fetch_add(&self, val: T, _: Ordering) -> T {
            let old = self.0.get();
            self.0.set(old + val);
            old
        }
    }

    pub type AtomicUsize = Atomic<usize>;
    pub type AtomicBool = Atomic<bool>;
    pub type AtomicU32 = Atomic<u32>;
    pub type AtomicU64 = Atomic<u64>;

    pub(super) use std::cell::RefCell as InnerRwLock;
    pub use std::{
        cell::{
            OnceCell as OnceLock, Ref as ReadGuard, Ref as MappedReadGuard, RefMut as WriteGuard,
            RefMut as MappedWriteGuard, RefMut as MappedLockGuard,
        },
        rc::{Rc as Lrc, Weak},
    };
}

#[cfg(feature = "parallel")]
mod sync {
    pub use parking_lot::{
        MappedMutexGuard as MappedLockGuard, MappedRwLockReadGuard as MappedReadGuard,
        MappedRwLockWriteGuard as MappedWriteGuard, RwLockReadGuard as ReadGuard,
        RwLockWriteGuard as WriteGuard,
    };

    pub(super) use parking_lot::RwLock as InnerRwLock;

    pub use std::sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize},
        Arc as Lrc, OnceLock, Weak,
    };
}

#[derive(Debug, Default)]
pub struct RwLock<T>(InnerRwLock<T>);

impl<T> RwLock<T> {
    #[inline(always)]
    pub fn new(inner: T) -> Self {
        RwLock(InnerRwLock::new(inner))
    }

    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }

    #[inline(always)]
    #[track_caller]
    pub fn read(&self) -> ReadGuard<'_, T> {
        #[cfg(not(feature = "parallel"))]
        return self.0.borrow();
        #[cfg(feature = "parallel")]
        return self.0.read();
    }

    #[inline(always)]
    #[track_caller]
    pub fn with_read_lock<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&*self.read())
    }

    #[inline(always)]
    pub fn try_write(&self) -> Result<WriteGuard<'_, T>, ()> {
        #[cfg(not(feature = "parallel"))]
        return self.0.try_borrow_mut().map_err(drop);
        #[cfg(feature = "parallel")]
        return self.0.try_write().ok_or(());
    }

    #[inline(always)]
    #[track_caller]
    pub fn write(&self) -> WriteGuard<'_, T> {
        #[cfg(not(feature = "parallel"))]
        return self.0.borrow_mut();
        #[cfg(feature = "parallel")]
        return self.0.write();
    }

    #[inline(always)]
    #[track_caller]
    pub fn with_write_lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut *self.write())
    }

    #[inline(always)]
    #[track_caller]
    pub fn borrow(&self) -> ReadGuard<'_, T> {
        self.read()
    }

    #[inline(always)]
    #[track_caller]
    pub fn borrow_mut(&self) -> WriteGuard<'_, T> {
        self.write()
    }

    #[inline(always)]
    pub fn leak(&self) -> &T {
        #[cfg(all(feature = "nightly", not(feature = "parallel")))]
        return ReadGuard::leak(self.read());
        #[cfg(not(all(feature = "nightly", not(feature = "parallel"))))]
        return {
            let guard = self.read();
            let ret = unsafe { &*(&*guard as *const T) };
            std::mem::forget(guard);
            ret
        };
    }
}
