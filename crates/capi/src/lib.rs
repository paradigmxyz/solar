#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use solar_cli::{
    Opts,
    standard_json::{ReadCallbackResult, StandardJsonReadCallback, compile_standard_json},
};
use std::{
    alloc::{Layout, alloc, dealloc},
    ffi::{CStr, c_char, c_void},
    mem::align_of,
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

type CStyleReadFileCallback = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const c_char,
    *mut *mut c_char,
    *mut *mut c_char,
);

const LICENSE: &[u8] = concat!(env!("CARGO_PKG_LICENSE"), "\0").as_bytes();
const VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
static ALLOCATIONS: AtomicPtr<AllocationHeader> = AtomicPtr::new(ptr::null_mut());

#[repr(C)]
struct AllocationHeader {
    size: usize,
    prev: *mut Self,
    next: *mut Self,
}

#[unsafe(no_mangle)]
pub extern "C" fn solidity_license() -> *const c_char {
    LICENSE.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn solidity_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn solidity_alloc(size: usize) -> *mut c_char {
    alloc_with_header(size).cast()
}

/// Explicitly frees memory allocated by `solidity_alloc` or returned by `solidity_compile`.
///
/// # Safety
///
/// `data` must be null or a pointer returned by `solidity_alloc` or `solidity_compile` that has
/// not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn solidity_free(data: *mut c_char) {
    if data.is_null() {
        return;
    }
    unsafe {
        free_with_header(data.cast());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn solidity_reset() {
    unsafe {
        reset_allocations();
    }
}

/// Compiles a UTF-8 Standard JSON input string and returns a newly allocated output string.
///
/// # Safety
///
/// `input` must be null or point to a valid null-terminated UTF-8 string. If `read_callback` is
/// provided, it must follow the `CStyleReadFileCallback` ABI and write only null pointers or
/// pointers allocated with `solidity_alloc` to its output parameters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn solidity_compile(
    input: *const c_char,
    read_callback: Option<CStyleReadFileCallback>,
    read_context: *mut c_void,
) -> *mut c_char {
    let input =
        if input.is_null() { "" } else { unsafe { CStr::from_ptr(input) }.to_str().unwrap_or("") };
    let read_callback = read_callback.map(|read_callback| {
        std::sync::Arc::new(CReadCallback { read_callback, read_context })
            as std::sync::Arc<dyn StandardJsonReadCallback>
    });
    let mut output = Vec::new();
    compile_standard_json(input, Opts::default(), read_callback, &mut output);
    allocate_c_string(&output)
}

fn allocate_c_string(value: &[u8]) -> *mut c_char {
    let allocation = solidity_alloc(value.len() + 1).cast::<u8>();
    if allocation.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(value.as_ptr(), allocation, value.len());
        allocation.add(value.len()).write(0);
    }
    allocation.cast()
}

fn alloc_with_header(size: usize) -> *mut u8 {
    let Some(total) = size_of_header().checked_add(size) else {
        return ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(total, align_of::<AllocationHeader>()) else {
        return ptr::null_mut();
    };
    unsafe {
        let base = alloc(layout);
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

struct CReadCallback {
    read_callback: CStyleReadFileCallback,
    read_context: *mut c_void,
}

unsafe impl Send for CReadCallback {}
unsafe impl Sync for CReadCallback {}

impl StandardJsonReadCallback for CReadCallback {
    fn read(&self, kind: &str, data: &str) -> ReadCallbackResult {
        let kind = c_string_bytes(kind);
        let data = c_string_bytes(data);
        let mut contents = ptr::null_mut();
        let mut error = ptr::null_mut();
        unsafe {
            (self.read_callback)(
                self.read_context,
                kind.as_ptr().cast(),
                data.as_ptr().cast(),
                &mut contents,
                &mut error,
            );
        }

        let result = if !contents.is_null() {
            ReadCallbackResult::Success(unsafe { take_c_string(contents) })
        } else if !error.is_null() {
            ReadCallbackResult::Error(unsafe { take_c_string(error) })
        } else {
            ReadCallbackResult::Unsupported
        };

        if !contents.is_null() {
            unsafe {
                solidity_free(contents);
            }
        }
        if !error.is_null() {
            unsafe {
                solidity_free(error);
            }
        }

        result
    }
}

fn c_string_bytes(value: &str) -> Vec<u8> {
    let mut bytes = value.as_bytes().iter().copied().filter(|&byte| byte != 0).collect::<Vec<_>>();
    bytes.push(0);
    bytes
}

unsafe fn take_c_string(ptr: *mut c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ALLOCATOR_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn allocator_test_lock() -> MutexGuard<'static, ()> {
        let lock = ALLOCATOR_TEST_LOCK.lock().unwrap();
        solidity_reset();
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
    fn api_strings_are_available() {
        assert_eq!(
            unsafe { CStr::from_ptr(solidity_license()) }.to_str().unwrap(),
            "MIT OR Apache-2.0"
        );
        assert_eq!(
            unsafe { CStr::from_ptr(solidity_version()) }.to_str().unwrap(),
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn allocator_roundtrip() {
        let _lock = allocator_test_lock();
        let ptr = solidity_alloc(4).cast::<u8>();
        assert!(!ptr.is_null());
        assert_eq!(allocation_count(), 1);
        unsafe {
            ptr.copy_from_nonoverlapping([1, 2, 3, 4].as_ptr(), 4);
            solidity_free(ptr.cast());
        }
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn reset_frees_tracked_allocations() {
        let _lock = allocator_test_lock();
        let a = solidity_alloc(4);
        let b = solidity_alloc(8);
        assert!(!a.is_null());
        assert!(!b.is_null());
        assert_eq!(allocation_count(), 2);
        solidity_reset();
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn free_middle_allocation_keeps_list_linked() {
        let _lock = allocator_test_lock();
        let a = solidity_alloc(4);
        let b = solidity_alloc(8);
        let c = solidity_alloc(16);
        assert!(!a.is_null());
        assert!(!b.is_null());
        assert!(!c.is_null());
        assert_eq!(allocation_count(), 3);

        unsafe {
            solidity_free(b);
        }
        assert_eq!(allocation_count(), 2);

        solidity_reset();
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn free_head_and_tail_allocations() {
        let _lock = allocator_test_lock();
        let a = solidity_alloc(4);
        let b = solidity_alloc(8);
        let c = solidity_alloc(16);
        assert!(!a.is_null());
        assert!(!b.is_null());
        assert!(!c.is_null());
        assert_eq!(allocation_count(), 3);

        unsafe {
            solidity_free(c);
            solidity_free(a);
        }
        assert_eq!(allocation_count(), 1);

        solidity_reset();
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn zero_size_allocation_is_tracked() {
        let _lock = allocator_test_lock();
        let ptr = solidity_alloc(0);
        assert!(!ptr.is_null());
        assert_eq!(allocation_count(), 1);
        unsafe {
            solidity_free(ptr);
        }
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }

    #[test]
    fn oversized_allocation_returns_null_without_tracking() {
        let _lock = allocator_test_lock();
        let ptr = solidity_alloc(usize::MAX);
        assert!(ptr.is_null());
        assert!(ALLOCATIONS.load(Ordering::Relaxed).is_null());
    }
}
