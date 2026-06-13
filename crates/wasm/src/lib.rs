#![allow(unused_crate_dependencies)]

use solar_cli::standard_json::{
    ReadCallbackResult, StandardJsonReadCallback, compile_standard_json,
};
use solar_config::Opts;
use std::{
    collections::HashMap,
    ffi::{CStr, c_char, c_void},
    ptr,
    sync::{Mutex, OnceLock},
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

static ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Box<[u8]>>>> = OnceLock::new();

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
    let mut allocation = vec![0; size].into_boxed_slice();
    let ptr = allocation.as_mut_ptr();
    allocations().lock().unwrap().insert(ptr.addr(), allocation);
    ptr.cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn solidity_free(data: *mut c_char) {
    if data.is_null() {
        return;
    }
    if allocations().lock().unwrap().remove(&data.addr()).is_none() {
        std::process::abort();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn solidity_reset() {
    allocations().lock().unwrap().clear();
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
    let output = match compile_standard_json(input, Opts::default(), read_callback) {
        Ok(output) => output,
        Err(error) => format!(
            r#"{{"errors":[{{"severity":"error","type":"InternalCompilerError","message":"{error}"}}]}}"#
        ),
    };
    allocate_c_string(&output)
}

fn allocations() -> &'static Mutex<HashMap<usize, Box<[u8]>>> {
    ALLOCATIONS.get_or_init(Default::default)
}

fn allocate_c_string(value: &str) -> *mut c_char {
    let mut allocation = Vec::with_capacity(value.len() + 1);
    allocation.extend_from_slice(value.as_bytes());
    allocation.push(0);
    let mut allocation = allocation.into_boxed_slice();
    let ptr = allocation.as_mut_ptr();
    allocations().lock().unwrap().insert(ptr.addr(), allocation);
    ptr.cast()
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
            solidity_free(contents);
        }
        if !error.is_null() {
            solidity_free(error);
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
}
