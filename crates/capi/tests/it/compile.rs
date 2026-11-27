use solar_capi::{solidity_alloc, solidity_compile, solidity_free};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

const INPUT: &CStr = cr#"{
  "language": "Solidity",
  "sources": { "A.sol": { "content": "contract A { function f() public pure returns (uint) { return 1; } }" } },
  "settings": { "outputSelection": { "*": { "*": ["abi", "evm.bytecode.object"] } } }
}"#;

fn compile() -> String {
    unsafe {
        let output = solidity_compile(INPUT.as_ptr(), None, ptr::null_mut());
        let s = CStr::from_ptr(output).to_str().unwrap().to_owned();
        solidity_free(output);
        s
    }
}

/// Each call must free its thread pool instead of leaking one per call.
#[test]
fn compile_frees_thread_pool() {
    let threads = || std::fs::read_dir("/proc/self/task").unwrap().count();
    let before = threads();
    let output = compile();
    assert!(output.contains("\"bytecode\""), "{output}");
    for _ in 0..10 {
        compile();
    }
    // Pool threads exit asynchronously after the pool drops.
    let deadline = Instant::now() + Duration::from_secs(30);
    while threads() > before && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(threads(), before);
}

#[derive(Default)]
struct Callbacks {
    active: AtomicUsize,
    max_active: AtomicUsize,
}

unsafe extern "C" fn read(
    context: *mut c_void,
    _kind: *const c_char,
    path: *const c_char,
    contents: *mut *mut c_char,
    _error: *mut *mut c_char,
) {
    let callbacks = unsafe { &*context.cast::<Callbacks>() };
    let active = callbacks.active.fetch_add(1, Ordering::SeqCst) + 1;
    callbacks.max_active.fetch_max(active, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(5));

    let name = unsafe { CStr::from_ptr(path) }.to_str().unwrap().trim_end_matches(".sol");
    let source = match name.strip_prefix('B') {
        Some(i) => format!("import \"C{i}.sol\"; contract {name} {{}}\0"),
        None => format!("contract {name} {{}}\0"),
    };
    unsafe {
        let data = solidity_alloc(source.len());
        data.cast::<u8>().copy_from_nonoverlapping(source.as_ptr(), source.len());
        *contents = data;
    }
    callbacks.active.fetch_sub(1, Ordering::SeqCst);
}

/// The compiler reads imports on several threads, but the callback must never run concurrently
/// and its `solidity_alloc` outputs must not corrupt the allocation list.
#[test]
fn read_callback_is_serialized() {
    let imports = (0..16).map(|i| format!("import \\\"B{i}.sol\\\";")).collect::<String>();
    let input = CString::new(format!(
        r#"{{"language":"Solidity","sources":{{"A.sol":{{"content":"{imports} contract A {{}}"}}}},"settings":{{"outputSelection":{{"*":{{"*":["abi"]}}}}}}}}"#
    ))
    .unwrap();
    let callbacks = Callbacks::default();
    let output = unsafe {
        let output = solidity_compile(input.as_ptr(), Some(read), ptr::addr_of!(callbacks) as _);
        let s = CStr::from_ptr(output).to_str().unwrap().to_owned();
        solidity_free(output);
        s
    };
    assert!(!output.contains("\"errors\""), "{output}");
    assert!(output.contains("\"C15.sol\""), "{output}");
    assert_eq!(callbacks.max_active.load(Ordering::SeqCst), 1);
}
