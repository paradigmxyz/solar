use crate::standard_json::{ReadCallbackResult, StandardJsonReadCallback, compile_standard_json};
use solar_config::Opts;
use std::{
    alloc::{Layout, alloc, dealloc},
    ffi::{CStr, c_char, c_void},
    mem::{align_of, size_of},
    ptr,
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

#[unsafe(no_mangle)]
pub(crate) extern "C" fn solidity_license() -> *const c_char {
    LICENSE.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub(crate) extern "C" fn solidity_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub(crate) extern "C" fn solidity_alloc(size: usize) -> *mut c_char {
    alloc_with_header(size).cast()
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn solidity_free(data: *mut c_char) {
    if data.is_null() {
        return;
    }
    unsafe {
        free_with_header(data.cast());
    }
}

#[unsafe(no_mangle)]
pub(crate) extern "C" fn solidity_reset() {}

/// Compiles a UTF-8 Standard JSON input string and returns a newly allocated output string.
///
/// # Safety
///
/// `input` must be null or point to a valid null-terminated UTF-8 string. If `read_callback` is
/// provided, it must follow the `CStyleReadFileCallback` ABI and write only null pointers or
/// pointers allocated with `solidity_alloc` to its output parameters.
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn solidity_compile(
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
    if let Err(error) = compile_standard_json(input, Opts::default(), read_callback, &mut output) {
        output = format!(
            r#"{{"errors":[{{"severity":"error","type":"InternalCompilerError","message":"{error}"}}]}}"#
        )
        .into_bytes();
    }
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
    let Some(total) = header_size().checked_add(size) else {
        return ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(total, align_of::<usize>()) else {
        return ptr::null_mut();
    };
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            return ptr::null_mut();
        }
        base.cast::<usize>().write(size);
        base.add(header_size())
    }
}

unsafe fn free_with_header(data: *mut u8) {
    let header = header_size();
    let base = unsafe { data.sub(header) };
    let size = unsafe { base.cast::<usize>().read() };
    let total = header + size;
    let layout = Layout::from_size_align(total, align_of::<usize>()).unwrap();
    unsafe {
        dealloc(base, layout);
    }
}

const fn header_size() -> usize {
    size_of::<usize>()
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
        let ptr = solidity_alloc(4).cast::<u8>();
        assert!(!ptr.is_null());
        unsafe {
            ptr.copy_from_nonoverlapping([1, 2, 3, 4].as_ptr(), 4);
            solidity_free(ptr.cast());
        }
    }
}
