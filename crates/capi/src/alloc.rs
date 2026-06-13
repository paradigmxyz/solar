use std::{
    alloc::{Layout, alloc as std_alloc, dealloc},
    ffi::c_char,
    mem::align_of,
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

static ALLOCATIONS: AtomicPtr<AllocationHeader> = AtomicPtr::new(ptr::null_mut());

#[repr(C)]
struct AllocationHeader {
    size: usize,
    prev: *mut Self,
    next: *mut Self,
}

pub(crate) fn alloc(size: usize) -> *mut c_char {
    alloc_with_header(size).cast()
}

pub(crate) fn allocate_c_string(value: &[u8]) -> *mut c_char {
    let allocation = alloc(value.len() + 1).cast::<u8>();
    if allocation.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(value.as_ptr(), allocation, value.len());
        allocation.add(value.len()).write(0);
    }
    allocation.cast()
}

/// # Safety
///
/// `data` must be null or a pointer returned by `alloc` that has not already been freed.
pub(crate) unsafe fn free(data: *mut c_char) {
    if data.is_null() {
        return;
    }
    unsafe {
        free_with_header(data.cast());
    }
}

pub(crate) fn reset() {
    unsafe {
        reset_allocations();
    }
}

fn alloc_with_header(size: usize) -> *mut u8 {
    let Some(total) = size_of_header().checked_add(size) else {
        return ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(total, align_of::<AllocationHeader>()) else {
        return ptr::null_mut();
    };
    unsafe {
        let base = std_alloc(layout);
        if base.is_null() {
            return ptr::null_mut();
        }
        let header = base.cast::<AllocationHeader>();
        header.write(AllocationHeader { size, prev: ptr::null_mut(), next: ptr::null_mut() });
        insert_allocation(header);
        base.add(size_of_header())
    }
}

unsafe fn free_with_header(data: *mut u8) {
    let header = unsafe { data.sub(size_of_header()).cast::<AllocationHeader>() };
    unsafe {
        unlink_allocation(header);
        dealloc_allocation(header);
    }
}

unsafe fn insert_allocation(header: *mut AllocationHeader) {
    let head = ALLOCATIONS.load(Ordering::Relaxed);
    unsafe {
        (*header).next = head;
        if !head.is_null() {
            (*head).prev = header;
        }
    }
    ALLOCATIONS.store(header, Ordering::Relaxed);
}

unsafe fn unlink_allocation(header: *mut AllocationHeader) {
    unsafe {
        if (*header).prev.is_null() {
            ALLOCATIONS.store((*header).next, Ordering::Relaxed);
        } else {
            (*(*header).prev).next = (*header).next;
        }
        if !(*header).next.is_null() {
            (*(*header).next).prev = (*header).prev;
        }
        (*header).prev = ptr::null_mut();
        (*header).next = ptr::null_mut();
    }
}

unsafe fn reset_allocations() {
    let mut current = ALLOCATIONS.swap(ptr::null_mut(), Ordering::Relaxed);
    while !current.is_null() {
        unsafe {
            let next = (*current).next;
            (*current).prev = ptr::null_mut();
            (*current).next = ptr::null_mut();
            dealloc_allocation(current);
            current = next;
        }
    }
}

unsafe fn dealloc_allocation(header: *mut AllocationHeader) {
    let total = size_of_header() + unsafe { (*header).size };
    let layout = Layout::from_size_align(total, align_of::<AllocationHeader>()).unwrap();
    unsafe {
        dealloc(header.cast(), layout);
    }
}

const fn size_of_header() -> usize {
    std::mem::size_of::<AllocationHeader>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ALLOCATOR_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn allocator_test_lock() -> MutexGuard<'static, ()> {
        let lock = ALLOCATOR_TEST_LOCK.lock().unwrap();
        reset();
        lock
    }

    fn allocation_count() -> usize {
        let mut count = 0;
        let mut current = ALLOCATIONS.load(Ordering::Relaxed);
        while !current.is_null() {
            count += 1;
            unsafe {
                current = (*current).next;
            }
        }
        count
    }

    #[test]
    fn allocator_roundtrip() {
        let _lock = allocator_test_lock();
        let ptr = alloc(4).cast::<u8>();
        assert!(!ptr.is_null());
        assert_eq!(allocation_count(), 1);
        unsafe {
            ptr.copy_from_nonoverlapping([1, 2, 3, 4].as_ptr(), 4);
            free(ptr.cast());
        }
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn reset_frees_tracked_allocations() {
        let _lock = allocator_test_lock();
        let a = alloc(4);
        let b = alloc(8);
        assert!(!a.is_null());
        assert!(!b.is_null());
        assert_eq!(allocation_count(), 2);
        reset();
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn free_middle_allocation_keeps_list_linked() {
        let _lock = allocator_test_lock();
        let a = alloc(4);
        let b = alloc(8);
        let c = alloc(16);
        assert!(!a.is_null());
        assert!(!b.is_null());
        assert!(!c.is_null());
        assert_eq!(allocation_count(), 3);

        unsafe {
            free(b);
        }
        assert_eq!(allocation_count(), 2);

        reset();
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn free_head_and_tail_allocations() {
        let _lock = allocator_test_lock();
        let a = alloc(4);
        let b = alloc(8);
        let c = alloc(16);
        assert!(!a.is_null());
        assert!(!b.is_null());
        assert!(!c.is_null());
        assert_eq!(allocation_count(), 3);

        unsafe {
            free(c);
            free(a);
        }
        assert_eq!(allocation_count(), 1);

        reset();
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn zero_size_allocation_is_tracked() {
        let _lock = allocator_test_lock();
        let ptr = alloc(0);
        assert!(!ptr.is_null());
        assert_eq!(allocation_count(), 1);
        unsafe {
            free(ptr);
        }
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn oversized_allocation_returns_null_without_tracking() {
        let _lock = allocator_test_lock();
        let ptr = alloc(usize::MAX);
        assert!(ptr.is_null());
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }
}
