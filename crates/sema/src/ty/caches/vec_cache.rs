//! VecCache maintains a mapping from K -> (V, I) pairing. K and I must be roughly u32-sized, and V
//! must be Copy.
//!
//! VecCache supports efficient concurrent put/get across the key space, with write-once semantics
//! (i.e., a given key can only be put once).
//!
//! This is adapted from rustc_data_structures::vec_cache.

#![allow(clippy::manual_map, clippy::use_self, clippy::zero_prefixed_literal)]

use solar_data_structures::index::Idx;
use std::{
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Index, IndexMut},
    sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering},
};

struct Slot<V> {
    // We never construct &Slot<V> so it's fine for this to not be in an UnsafeCell.
    value: V,
    // This is both an index and a once-lock.
    //
    // 0: not yet initialized.
    // 1: lock held, initializing.
    // 2..u32::MAX - 2: initialized.
    index_and_lock: AtomicU32,
}

/// This uniquely identifies a single `Slot<V>` entry in the buckets map, and provides accessors for
/// either getting the value or putting a value.
#[derive(Copy, Clone, Debug)]
struct SlotIndex {
    // The index of the bucket in VecCache (0 to 20).
    bucket_idx: BucketIndex,
    // The index of the slot within the bucket.
    index_in_bucket: usize,
}

const ENTRIES_BY_BUCKET: [usize; BUCKETS] = {
    let mut entries = [0; BUCKETS];
    let mut key = 0;
    loop {
        let si = SlotIndex::from_index(key);
        entries[si.bucket_idx.to_usize()] = si.bucket_idx.capacity();
        if key == 0 {
            key = 1;
        } else if key == (1 << 31) {
            break;
        } else {
            key <<= 1;
        }
    }
    entries
};

const BUCKETS: usize = 21;

impl SlotIndex {
    /// Unpacks a flat 32-bit index into a [`BucketIndex`] and a slot offset within that bucket.
    #[inline]
    const fn from_index(idx: u32) -> Self {
        let (bucket_idx, index_in_bucket) = BucketIndex::from_flat_index(idx as usize);
        SlotIndex { bucket_idx, index_in_bucket }
    }

    // SAFETY: Buckets must be managed solely by functions here (i.e. get/put on SlotIndex) and
    // `self` comes from SlotIndex::from_index.
    #[inline]
    unsafe fn get<V: Copy>(&self, buckets: &[AtomicPtr<Slot<V>>; BUCKETS]) -> Option<(V, u32)> {
        let bucket = &buckets[self.bucket_idx];
        let ptr = bucket.load(Ordering::Acquire);
        // Bucket is not yet initialized: then we obviously won't find this entry in that bucket.
        if ptr.is_null() {
            return None;
        }
        debug_assert!(self.index_in_bucket < self.bucket_idx.capacity());
        // SAFETY: `bucket` was allocated to hold `entries`, so this must be inbounds.
        let slot = unsafe { ptr.add(self.index_in_bucket) };

        // SAFETY: initialized bucket has zeroed all memory within the bucket, so we are valid for
        // AtomicU32 access.
        let index_and_lock = unsafe { &(*slot).index_and_lock };
        let current = index_and_lock.load(Ordering::Acquire);
        let index = match current {
            0 => return None,
            // Treat "initializing" as not initialized from the load side.
            1 => return None,
            _ => current - 2,
        };

        // SAFETY:
        // * slot is a valid pointer.
        // * value is initialized since we saw a >= 2 index above.
        // * `V: Copy`, so safe to read.
        let value = unsafe { (*slot).value };
        Some((value, index))
    }

    fn bucket_ptr<V>(&self, bucket: &AtomicPtr<Slot<V>>) -> *mut Slot<V> {
        let ptr = bucket.load(Ordering::Acquire);
        if ptr.is_null() { Self::initialize_bucket(bucket, self.bucket_idx) } else { ptr }
    }

    #[cold]
    #[inline(never)]
    fn initialize_bucket<V>(bucket: &AtomicPtr<Slot<V>>, bucket_idx: BucketIndex) -> *mut Slot<V> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        let _allocator_guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ptr = bucket.load(Ordering::Acquire);
        if ptr.is_null() {
            let bucket_layout =
                std::alloc::Layout::array::<Slot<V>>(bucket_idx.capacity()).unwrap();
            assert!(bucket_layout.size() > 0);
            // SAFETY: Just checked that size is non-zero.
            let allocated = unsafe { std::alloc::alloc_zeroed(bucket_layout).cast::<Slot<V>>() };
            if allocated.is_null() {
                std::alloc::handle_alloc_error(bucket_layout);
            }
            bucket.store(allocated, Ordering::Release);
            allocated
        } else {
            ptr
        }
    }

    /// Returns true if this successfully put into the map.
    #[inline]
    fn put<V>(&self, buckets: &[AtomicPtr<Slot<V>>; BUCKETS], value: V, extra: u32) -> bool {
        let bucket = &buckets[self.bucket_idx];
        let ptr = self.bucket_ptr(bucket);

        debug_assert!(self.index_in_bucket < self.bucket_idx.capacity());
        // SAFETY: `bucket` was allocated to hold `entries`, so this must be inbounds.
        let slot = unsafe { ptr.add(self.index_in_bucket) };

        // SAFETY: initialized bucket has zeroed all memory within the bucket, so we are valid for
        // AtomicU32 access.
        let index_and_lock = unsafe { &(*slot).index_and_lock };
        match index_and_lock.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                // We have acquired the initialization lock. It is our job to write `value` and
                // then set the lock to the real index.
                unsafe {
                    (&raw mut (*slot).value).write(value);
                }

                index_and_lock.store(extra.checked_add(2).unwrap(), Ordering::Release);
                true
            }
            Err(1) => {
                while index_and_lock.load(Ordering::Acquire) == 1 {
                    std::hint::spin_loop();
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Inserts into the map, given that the slot is unique, so it won't race with other threads.
    #[inline]
    unsafe fn put_unique<V>(&self, buckets: &[AtomicPtr<Slot<V>>; BUCKETS], value: V, extra: u32) {
        let bucket = &buckets[self.bucket_idx];
        let ptr = self.bucket_ptr(bucket);

        debug_assert!(self.index_in_bucket < self.bucket_idx.capacity());
        // SAFETY: `bucket` was allocated to hold `entries`, so this must be inbounds.
        let slot = unsafe { ptr.add(self.index_in_bucket) };

        // SAFETY: We know our slot is unique as a precondition of this function, so this can't
        // race.
        unsafe {
            (&raw mut (*slot).value).write(value);
        }

        // SAFETY: initialized bucket has zeroed all memory within the bucket, so we are valid for
        // AtomicU32 access.
        let index_and_lock = unsafe { &(*slot).index_and_lock };

        index_and_lock.store(extra.checked_add(2).unwrap(), Ordering::Release);
    }
}

/// In-memory cache for queries whose keys are densely-numbered IDs.
pub(in crate::ty) struct VecCache<K: Idx, V, I> {
    // Entries per bucket:
    // Bucket  0:       4096 2^12
    // Bucket  1:       4096 2^12
    // Bucket  2:       8192
    // Bucket  3:      16384
    // ...
    // Bucket 19: 1073741824
    // Bucket 20: 2147483648
    // The total number of entries if all buckets are initialized is 2^32.
    buckets: [AtomicPtr<Slot<V>>; BUCKETS],

    // In the compiler's current usage these are only *read* during incremental and self-profiling.
    // They are an optimization over iterating the full buckets array.
    present: [AtomicPtr<Slot<()>>; BUCKETS],
    len: AtomicUsize,

    key: PhantomData<(K, I)>,
}

impl<K: Idx, V, I> Default for VecCache<K, V, I> {
    fn default() -> Self {
        VecCache {
            buckets: Default::default(),
            key: PhantomData,
            len: Default::default(),
            present: Default::default(),
        }
    }
}

impl<K: Idx, V, I> Drop for VecCache<K, V, I> {
    fn drop(&mut self) {
        // We have unique ownership, so no locks etc. are needed. Since `K` and `V` are both Copy,
        // we are also guaranteed to just need to deallocate any large arrays.
        assert!(!std::mem::needs_drop::<K>());
        assert!(!std::mem::needs_drop::<V>());

        for (idx, bucket) in BucketIndex::enumerate_buckets(&self.buckets) {
            let bucket = bucket.load(Ordering::Acquire);
            if !bucket.is_null() {
                let layout = std::alloc::Layout::array::<Slot<V>>(ENTRIES_BY_BUCKET[idx]).unwrap();
                unsafe {
                    std::alloc::dealloc(bucket.cast(), layout);
                }
            }
        }

        for (idx, bucket) in BucketIndex::enumerate_buckets(&self.present) {
            let bucket = bucket.load(Ordering::Acquire);
            if !bucket.is_null() {
                let layout = std::alloc::Layout::array::<Slot<()>>(ENTRIES_BY_BUCKET[idx]).unwrap();
                unsafe {
                    std::alloc::dealloc(bucket.cast(), layout);
                }
            }
        }
    }
}

impl<K, V, I> VecCache<K, V, I>
where
    K: Eq + Idx + Copy + Debug,
    V: Copy,
    I: Idx + Copy,
{
    #[inline(always)]
    pub(super) fn lookup(&self, key: &K) -> Option<(V, I)> {
        let key = u32::try_from(key.index()).unwrap();
        let slot_idx = SlotIndex::from_index(key);
        match unsafe { slot_idx.get(&self.buckets) } {
            Some((value, idx)) => Some((value, I::from_usize(idx as usize))),
            None => None,
        }
    }

    #[inline]
    pub(super) fn complete(&self, key: K, value: V, index: I) {
        let key = u32::try_from(key.index()).unwrap();
        let slot_idx = SlotIndex::from_index(key);
        if slot_idx.put(&self.buckets, value, index.index() as u32) {
            let present_idx = self.len.fetch_add(1, Ordering::Relaxed);
            let slot = SlotIndex::from_index(u32::try_from(present_idx).unwrap());
            // SAFETY: We should always be uniquely putting due to `len` fetch_add returning unique
            // values.
            unsafe { slot.put_unique(&self.present, (), key) };
        }
    }

    #[allow(dead_code)]
    pub(super) fn for_each(&self, f: &mut dyn FnMut(&K, &V, I)) {
        for idx in 0..self.len.load(Ordering::Acquire) {
            let key = SlotIndex::from_index(idx as u32);
            match unsafe { key.get(&self.present) } {
                None => unreachable!(),
                Some(((), key)) => {
                    let key = K::from_usize(key as usize);
                    let value = self.lookup(&key).unwrap();
                    f(&key, &value.0, value.1);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }
}

/// Index into an array of buckets.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum BucketIndex {
    Bucket00,
    Bucket01,
    Bucket02,
    Bucket03,
    Bucket04,
    Bucket05,
    Bucket06,
    Bucket07,
    Bucket08,
    Bucket09,
    Bucket10,
    Bucket11,
    Bucket12,
    Bucket13,
    Bucket14,
    Bucket15,
    Bucket16,
    Bucket17,
    Bucket18,
    Bucket19,
    Bucket20,
}

impl Debug for BucketIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.to_usize(), f)
    }
}

impl BucketIndex {
    /// Capacity of bucket 0 (and also of bucket 1).
    const BUCKET_0_CAPACITY: usize = 1 << (Self::NONZERO_BUCKET_SHIFT_ADJUST + 1);
    /// Adjustment factor from the highest-set-bit-position of a flat index,
    /// to its corresponding bucket number.
    const NONZERO_BUCKET_SHIFT_ADJUST: usize = 11;

    #[inline(always)]
    const fn to_usize(self) -> usize {
        self as usize
    }

    #[inline(always)]
    const fn from_raw(raw: usize) -> Self {
        match raw {
            00 => Self::Bucket00,
            01 => Self::Bucket01,
            02 => Self::Bucket02,
            03 => Self::Bucket03,
            04 => Self::Bucket04,
            05 => Self::Bucket05,
            06 => Self::Bucket06,
            07 => Self::Bucket07,
            08 => Self::Bucket08,
            09 => Self::Bucket09,
            10 => Self::Bucket10,
            11 => Self::Bucket11,
            12 => Self::Bucket12,
            13 => Self::Bucket13,
            14 => Self::Bucket14,
            15 => Self::Bucket15,
            16 => Self::Bucket16,
            17 => Self::Bucket17,
            18 => Self::Bucket18,
            19 => Self::Bucket19,
            20 => Self::Bucket20,
            _ => panic!("bucket index out of range"),
        }
    }

    /// Total number of slots in this bucket.
    #[inline(always)]
    const fn capacity(self) -> usize {
        match self {
            Self::Bucket00 => Self::BUCKET_0_CAPACITY,
            _ => 1 << (self.to_usize() + Self::NONZERO_BUCKET_SHIFT_ADJUST),
        }
    }

    /// Converts a flat index in the range `0..=u32::MAX` into a bucket index,
    /// and a slot offset within that bucket.
    #[inline(always)]
    const fn from_flat_index(flat: usize) -> (Self, usize) {
        if flat > u32::MAX as usize {
            panic!();
        }

        if flat < Self::BUCKET_0_CAPACITY {
            return (Self::Bucket00, flat);
        }

        let highest_bit_pos = flat.ilog2() as usize;
        let bucket_index =
            BucketIndex::from_raw(highest_bit_pos - Self::NONZERO_BUCKET_SHIFT_ADJUST);
        let slot_offset = flat - (1 << highest_bit_pos);

        (bucket_index, slot_offset)
    }

    #[inline(always)]
    fn iter_all() -> impl ExactSizeIterator<Item = Self> {
        (0usize..BUCKETS).map(BucketIndex::from_raw)
    }

    #[inline(always)]
    fn enumerate_buckets<T>(buckets: &[T; BUCKETS]) -> impl ExactSizeIterator<Item = (usize, &T)> {
        BucketIndex::iter_all().map(BucketIndex::to_usize).zip(buckets)
    }
}

impl<T> Index<BucketIndex> for [T; BUCKETS] {
    type Output = T;

    #[inline(always)]
    fn index(&self, index: BucketIndex) -> &Self::Output {
        &self[index.to_usize()]
    }
}

impl<T> IndexMut<BucketIndex> for [T; BUCKETS] {
    #[inline(always)]
    fn index_mut(&mut self, index: BucketIndex) -> &mut Self::Output {
        &mut self[index.to_usize()]
    }
}
