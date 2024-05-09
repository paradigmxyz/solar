pub use parking_lot::{
    MappedMutexGuard as MappedLockGuard, MappedRwLockReadGuard as MappedReadGuard,
    MappedRwLockWriteGuard as MappedWriteGuard, Mutex as Lock, RwLock,
    RwLockReadGuard as ReadGuard, RwLockWriteGuard as WriteGuard,
};

pub use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize},
    Arc as Lrc, OnceLock, Weak,
};
